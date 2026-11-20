use std::collections::BTreeMap;

use super::*;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    Sizable,
    input::{Input, Textarea},
};
use writer::{
    DocumentKind, ProjectError,
    outline::{Outline, SceneTiming},
    timeline::{Event, Relation, Timeline},
};

const CARD_WIDTH: f32 = 148.;
const CARD_HEIGHT: f32 = 36.;
const COLUMN_WIDTH: f32 = 252.;
const ROW_HEIGHT: f32 = 72.;
const GRAPH_MARGIN: f32 = 18.;

struct NetworkArrow {
    start: Point<Pixels>,
    control_a: Point<Pixels>,
    control_b: Point<Pixels>,
    end: Point<Pixels>,
    forward: bool,
    kind: Relation,
}

impl NetworkArrow {
    fn between(from: Point<Pixels>, to: Point<Pixels>, kind: Relation) -> Self {
        let offset = if kind == Relation::Cause { 10. } else { -10. };
        let start_y = from.y + px(CARD_HEIGHT / 2. + offset);
        let end_y = to.y + px(CARD_HEIGHT / 2. + offset);
        let forward = from.x < to.x;
        let (start, control_a, control_b, end) = if from.x == to.x {
            let side = from.x + px(CARD_WIDTH);
            let outside = side + px(48.);
            (
                point(side, start_y - px(5.)),
                point(outside, start_y - px(5.)),
                point(outside, end_y + px(5.)),
                point(side, end_y + px(5.)),
            )
        } else {
            let (start, end) = if forward {
                (from.x + px(CARD_WIDTH), to.x)
            } else {
                (from.x, to.x + px(CARD_WIDTH))
            };
            let bend = (start + end) * 0.5;
            (
                point(start, start_y),
                point(bend, start_y),
                point(bend, end_y),
                point(end, end_y),
            )
        };
        Self {
            start,
            control_a,
            control_b,
            end,
            forward,
            kind,
        }
    }

    fn label_position(&self) -> Point<Pixels> {
        (self.start + self.control_a * 3. + self.control_b * 3. + self.end) * 0.125
    }

    fn paint(self, origin: Point<Pixels>, window: &mut Window, cx: &App) {
        let before = self.kind == Relation::Before;
        let color = if before {
            cx.theme().muted_foreground
        } else {
            cx.theme().primary
        };
        let mut stroke = PathBuilder::stroke(px(if before { 1.5 } else { 2. }));
        if before {
            stroke = stroke.dash_array(&[px(5.), px(3.)]);
        }
        let end = origin + self.end;
        stroke.move_to(origin + self.start);
        stroke.cubic_bezier_to(end, origin + self.control_a, origin + self.control_b);
        if let Ok(path) = stroke.build() {
            window.paint_path(path, color);
        }
        let inset = if self.forward { -7. } else { 7. };
        let mut head = PathBuilder::stroke(px(1.5));
        head.move_to(point(end.x + px(inset), end.y - px(4.)));
        head.line_to(end);
        head.line_to(point(end.x + px(inset), end.y + px(4.)));
        if let Ok(path) = head.build() {
            window.paint_path(path, color);
        }
    }
}

struct DragScene(Id);
struct DragChapter(Id);

struct OutlineDragPreview(String);

impl Render for OutlineDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(self.0.clone())
    }
}

fn drag_handle<T: 'static>(payload: T, id: String, title: &str) -> impl IntoElement {
    let title = title.to_owned();
    div()
        .id(id)
        .px_1()
        .cursor_move()
        .child("⋮⋮")
        .on_drag(payload, move |_, _, _, cx| {
            cx.new(|_| OutlineDragPreview(title.clone()))
        })
        .test_support()
}

impl ProjectView {
    pub(super) fn projects_mut(&mut self) -> Option<(&Project, &mut Store)> {
        self.project.as_ref().zip(self.store.as_mut())
    }

    fn persist_timeline(&mut self, timeline: &Timeline, id: Id, cx: &mut Context<Self>) -> bool {
        let Some((project, store)) = self.projects_mut() else {
            return false;
        };
        let result = timeline.stage_event(id, project, store);
        self.report(result, cx) && self.flush(cx)
    }

    fn persist_outline(&mut self, outline: &Outline, cx: &mut Context<Self>) -> bool {
        let Some((project, store)) = self.projects_mut() else {
            return false;
        };
        let result = outline.stage_changes(project, store);
        self.report(result, cx) && self.flush(cx)
    }

    pub(super) fn select_event(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        let timeline = Timeline::from_project(project);
        let Some(event) = timeline.event(id) else {
            return;
        };
        self.event = Some(id);
        self.event_target = None;
        self.event_title.update(cx, |input, cx| {
            input.set_value(event.title.clone(), window, cx)
        });
        self.form_body.update(cx, |input, cx| {
            input.set_value(event.body.clone(), window, cx)
        });
        self.event_time.update(cx, |input, cx| {
            input.set_value(event.time.clone().unwrap_or_default(), window, cx)
        });
        cx.notify();
    }

    fn save_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let title = self.event_title.read(cx).value();
        if title.trim().is_empty() {
            self.error = Some("Gib dem Ereignis einen Titel.".into());
            cx.notify();
            return;
        }
        let body = self.form_body.read(cx).value();
        let time = optional(self.event_time.read(cx).value().as_ref());
        let mut timeline = Timeline::from_project(project);
        let id = if let Some(id) = self.event {
            if let Err(error) = timeline.update_event(id, title.trim(), &body, time) {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
            id
        } else {
            timeline.create_event(title.trim(), &body, time)
        };
        if self.persist_timeline(&timeline, id, cx) {
            self.select_event(id, window, cx);
        }
    }

    fn relate(&mut self, kind: Relation, cx: &mut Context<Self>) {
        let Some((from, to)) = self.event.zip(self.event_target) else {
            return;
        };
        if !self.flush(cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let mut timeline = Timeline::from_project(project);
        let note = self.relation_note.read(cx).value().to_string();
        let time = optional(self.relation_time.read(cx).value().as_ref());
        match timeline.relate_at(from, to, kind, note, time) {
            Ok(()) => {
                self.persist_timeline(&timeline, from, cx);
            }
            Err(writer::timeline::TimelineError::Cycle { path }) => {
                self.error = Some(format!(
                    "Zeitkreis: {}",
                    path.iter()
                        .filter_map(|id| timeline.event(*id))
                        .map(|event| event.title.as_str())
                        .collect::<Vec<_>>()
                        .join(" → ")
                ));
                cx.notify();
            }
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn select_relation(
        &mut self,
        from: Id,
        to: Id,
        kind: Relation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.project else { return };
        let timeline = Timeline::from_project(project);
        let Some(link) = timeline
            .relations()
            .iter()
            .find(|link| (link.from, link.to, link.kind) == (from, to, kind))
        else {
            return;
        };
        self.select_event(from, window, cx);
        self.event_target = Some(to);
        self.relation_note.update(cx, |input, cx| {
            input.set_value(link.note.clone(), window, cx)
        });
        self.relation_time.update(cx, |input, cx| {
            input.set_value(link.time.clone().unwrap_or_default(), window, cx)
        });
        cx.notify();
    }

    fn event_button(&self, id: Id, title: &str, cx: &mut Context<Self>) -> Button {
        Button::new(format!("event-{id}"))
            .label(title.to_owned())
            .when(self.event == Some(id), |button| button.primary())
            .on_click(cx.listener(move |view, _, window, cx| view.select_event(id, window, cx)))
    }

    fn event_network(&self, timeline: &Timeline, events: &[&Event], cx: &mut Context<Self>) -> Div {
        let columns = timeline.event_columns();
        let mut positions = BTreeMap::new();
        for (column, cards) in columns.iter().enumerate() {
            for (row, event) in cards.iter().enumerate() {
                positions.insert(
                    event.id,
                    point(
                        px(column as f32 * COLUMN_WIDTH + GRAPH_MARGIN),
                        px(row as f32 * ROW_HEIGHT + GRAPH_MARGIN),
                    ),
                );
            }
        }
        let mut arrows = Vec::new();
        let mut labels = Vec::new();
        for link in timeline.relations() {
            let (Some(&from), Some(&to)) = (positions.get(&link.from), positions.get(&link.to))
            else {
                continue;
            };
            let arrow = NetworkArrow::between(from, to, link.kind);
            let position = arrow.label_position();
            let (from, to, kind) = (link.from, link.to, link.kind);
            let text = link.time.as_deref().unwrap_or_else(|| relation_label(kind));
            labels.push(
                div()
                    .absolute()
                    .left(position.x - px(45.))
                    .top(position.y - px(11.))
                    .w(px(90.))
                    .child(
                        Button::new(format!("network-link-{from}-{to}-{kind:?}"))
                            .small()
                            .outline()
                            .w_full()
                            .truncate()
                            .label(text.to_owned())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.select_relation(from, to, kind, window, cx)
                            })),
                    ),
            );
            arrows.push(arrow);
        }
        let isolated = events
            .iter()
            .filter(|event| !positions.contains_key(&event.id));
        let mut network = div().flex().flex_col().gap_2();
        if !columns.is_empty() {
            let width = columns.len() as f32 * COLUMN_WIDTH;
            let rows = columns.iter().map(Vec::len).max().unwrap_or_default();
            let height = (rows - 1) as f32 * ROW_HEIGHT + CARD_HEIGHT + 2. * GRAPH_MARGIN;
            network = network.child(
                div()
                    .id("event-network")
                    .min_w_0()
                    .overflow_x_scroll()
                    .child(
                        div()
                            .relative()
                            .w(px(width))
                            .h(px(height))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, (), window, cx| {
                                        for arrow in arrows {
                                            arrow.paint(bounds.origin, window, cx);
                                        }
                                    },
                                )
                                .absolute()
                                .top_0()
                                .left_0()
                                .size_full(),
                            )
                            .children(columns.iter().flatten().filter_map(|event| {
                                let position = positions.get(&event.id)?;
                                Some(
                                    div()
                                        .absolute()
                                        .left(position.x)
                                        .top(position.y)
                                        .w(px(CARD_WIDTH))
                                        .h(px(CARD_HEIGHT))
                                        .flex()
                                        .items_center()
                                        .child(
                                            self.event_button(event.id, &event.title, cx)
                                                .w_full()
                                                .h_full()
                                                .truncate(),
                                        ),
                                )
                            }))
                            .children(labels),
                    ),
            );
        }
        if isolated.clone().next().is_some() {
            network =
                network.child("Zeitlich offen").child(
                    div().flex().flex_wrap().gap_2().children(
                        isolated.map(|event| self.event_button(event.id, &event.title, cx)),
                    ),
                );
        }
        network
    }

    pub(super) fn timeline_view(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let timeline = Timeline::from_project(project);
        let events = timeline.ordered_events();
        let tiles = if self.event_list {
            div()
                .flex()
                .flex_col()
                .gap_2()
                .children(events.iter().map(|event| {
                    let edges = timeline
                        .relations()
                        .iter()
                        .filter(|edge| edge.from == event.id)
                        .filter_map(|edge| {
                            timeline.event(edge.to).map(|target| {
                                format!("{} {}", relation_label(edge.kind), target.title)
                            })
                        })
                        .collect::<Vec<_>>()
                        .join(" · ");
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(self.event_button(event.id, &event.title, cx))
                        .child(edges)
                }))
        } else {
            self.event_network(&timeline, &events, cx)
        };
        let mut content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children([("Netz", false), ("Liste", true)].into_iter().map(
                        |(label, list)| {
                            Button::new(label)
                                .label(label)
                                .when(self.event_list == list, |button| button.primary())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.event_list = list;
                                    cx.notify();
                                }))
                        },
                    ))
                    .child(
                        Button::new("new-event")
                            .label(if events.is_empty() {
                                "Erstes Ereignis anlegen"
                            } else {
                                "Neues Ereignis"
                            })
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.event = None;
                                view.event_target = None;
                                view.event_title.update(cx, |input, cx| {
                                    input.set_value("", window, cx);
                                });
                                view.form_body
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                view.event_time
                                    .update(cx, |input, cx| input.set_value("", window, cx));
                                cx.notify();
                            })),
                    ),
            )
            .child(tiles)
            .children(
                timeline
                    .issues()
                    .iter()
                    .map(|issue| div().child(issue.to_string())),
            )
            .child("Ereignistitel")
            .child(Input::new(&self.event_title))
            .child("Stichpunkte")
            .child(Textarea::new(&self.form_body))
            .child("Zeitangabe")
            .child(Input::new(&self.event_time))
            .child(
                Button::new("save-event")
                    .primary()
                    .label("Ereignis sichern")
                    .on_click(cx.listener(|view, _, window, cx| view.save_event(window, cx))),
            );
        if let Some(from) = self.event {
            content = content.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .children(
                        timeline
                            .relations()
                            .iter()
                            .filter(|edge| edge.from == from || edge.to == from)
                            .map(|edge| {
                                let title = |id| {
                                    timeline
                                        .event(id)
                                        .map_or_else(|| id.to_string(), |event| event.title.clone())
                                };
                                let (a, b, kind) = (edge.from, edge.to, edge.kind);
                                div()
                                    .flex()
                                    .gap_2()
                                    .items_center()
                                    .child(format!(
                                        "{} → {} · {} · {} {}",
                                        title(a),
                                        title(b),
                                        relation_label(kind),
                                        edge.note,
                                        edge.time.as_deref().unwrap_or_default()
                                    ))
                                    .child(
                                        Button::new(format!("unlink-{a}-{b}-{kind:?}"))
                                            .label("Verbindung lösen")
                                            .on_click(cx.listener(move |view, _, _, cx| {
                                                if !view.flush(cx) {
                                                    return;
                                                }
                                                if let Some(project) = &view.project {
                                                    let mut timeline =
                                                        Timeline::from_project(project);
                                                    if timeline.remove_relation(a, b, kind) {
                                                        view.persist_timeline(&timeline, a, cx);
                                                    }
                                                }
                                            })),
                                    )
                            }),
                    )
                    .child("Verknüpfen mit")
                    .child(
                        div().flex().flex_wrap().gap_2().children(
                            events
                                .into_iter()
                                .filter(|event| event.id != from)
                                .map(|event| {
                                    let id = event.id;
                                    Button::new(format!("target-{id}"))
                                        .label(event.title.clone())
                                        .when(self.event_target == Some(id), |button| {
                                            button.primary()
                                        })
                                        .on_click(cx.listener(move |view, _, _, cx| {
                                            view.event_target = Some(id);
                                            cx.notify();
                                        }))
                                }),
                        ),
                    )
                    .when(self.event_target.is_some(), |panel| {
                        panel
                            .child("Erläuterung")
                            .child(Input::new(&self.relation_note))
                            .child("Relative Zeitangabe")
                            .child(Input::new(&self.relation_time))
                            .child(div().flex().gap_2().children(
                                [Relation::Before, Relation::Cause].into_iter().map(|kind| {
                                    Button::new(relation_label(kind))
                                        .label(relation_label(kind))
                                        .on_click(
                                            cx.listener(move |view, _, _, cx| {
                                                view.relate(kind, cx)
                                            }),
                                        )
                                }),
                            ))
                    })
                    .child(
                        Button::new("brainstorm-from-event")
                            .label("Lücke erkunden")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.start_pair(from, window, cx)
                            })),
                    )
                    .child(
                        Button::new("derive-scene")
                            .label("Szene ableiten")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if !view.flush(cx) {
                                    return;
                                }
                                let Some(project) = &view.project else { return };
                                let mut outline = Outline::from_project(project);
                                let source = view.form_body.read(cx).value();
                                let id = outline.derive_scene(from, &source);
                                if view.persist_outline(&outline, cx) {
                                    view.select_scene(id, window, cx);
                                }
                            })),
                    ),
            );
        }
        content
    }

    pub(super) fn apply_outline(
        &mut self,
        action: impl FnOnce(&mut Outline) -> Result<(), writer::outline::OutlineError>,
        cx: &mut Context<Self>,
    ) {
        if !self.flush(cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let mut outline = Outline::from_project(project);
        match action(&mut outline) {
            Ok(()) => {
                self.persist_outline(&outline, cx);
            }
            Err(error) => {
                let problem = ProjectError::Storage {
                    path: project.root().into(),
                    reason: error.to_string(),
                };
                self.report(Err(problem), cx);
            }
        }
    }

    pub(super) fn outline_view(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let outline = Outline::from_project(project);
        let scene_line = |id,
                          title: &str,
                          position: Option<(Id, usize, usize)>,
                          cx: &mut Context<Self>| {
            div()
                .id(format!("scene-row-{id}"))
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(drag_handle(
                    DragScene(id),
                    format!("drag-scene-{id}"),
                    title,
                ))
                .child(
                    Button::new(format!("scene-{id}"))
                        .label(title.to_owned())
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.select_scene(id, window, cx)
                        })),
                )
                .when_some(self.chapter, |line, target| {
                    line.child(
                        Button::new(format!("place-{id}"))
                            .label("In gewähltes Kapitel")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.apply_outline(|outline| outline.append_scene(id, target), cx)
                            })),
                    )
                })
                .when_some(position, |line, (chapter, index, total)| {
                    line.drag_over::<DragScene>(|style, _, _, cx| style.bg(cx.theme().muted))
                        .on_drop(cx.listener(move |view, source: &DragScene, _, cx| {
                            view.apply_outline(
                                |outline| outline.place_at_scene(source.0, chapter, id),
                                cx,
                            )
                        }))
                        .when(index > 0, |line| {
                            line.child(
                                Button::new(format!("scene-up-{id}"))
                                    .label("Nach oben")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.apply_outline(
                                            |outline| outline.place(id, chapter, index - 1),
                                            cx,
                                        )
                                    })),
                            )
                        })
                        .when(index + 1 < total, |line| {
                            line.child(
                                Button::new(format!("scene-down-{id}"))
                                    .label("Nach unten")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.apply_outline(
                                            |outline| outline.place(id, chapter, index + 1),
                                            cx,
                                        )
                                    })),
                            )
                        })
                        .child(
                            Button::new(format!("unplace-{id}"))
                                .label("Aus Kapitel nehmen")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.apply_outline(
                                        |outline| outline.remove_from_chapter(id).map(|_| ()),
                                        cx,
                                    )
                                })),
                        )
                })
                .test_support()
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child("Kapitelname")
            .child(Input::new(&self.chapter_title))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("chapter-add")
                            .primary()
                            .label("Kapitel anlegen")
                            .on_click(cx.listener(|view, _, _, cx| {
                                let title = view.chapter_title.read(cx).value();
                                if !title.trim().is_empty() {
                                    view.apply_outline(
                                        |outline| {
                                            outline.create_chapter(title.trim());
                                            Ok(())
                                        },
                                        cx,
                                    );
                                }
                            })),
                    )
                    .when_some(self.chapter, |panel, id| {
                        panel.child(
                            Button::new("chapter-rename")
                                .label("Kapitel umbenennen")
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    let title = view.chapter_title.read(cx).value();
                                    if !title.trim().is_empty() {
                                        view.apply_outline(
                                            |outline| outline.rename_chapter(id, title.trim()),
                                            cx,
                                        );
                                    }
                                })),
                        )
                    }),
            )
            .children(
                outline
                    .issues()
                    .iter()
                    .map(|issue| div().child(issue.to_string())),
            )
            .children(
                outline
                    .chapters()
                    .iter()
                    .enumerate()
                    .map(|(index, chapter)| {
                        let id = chapter.id;
                        let title = chapter.title.clone();
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .pb_2()
                            .child(
                                div()
                                    .id(format!("chapter-row-{id}"))
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .drag_over::<DragChapter>(|style, _, _, cx| {
                                        style.bg(cx.theme().muted)
                                    })
                                    .drag_over::<DragScene>(|style, _, _, cx| {
                                        style.bg(cx.theme().muted)
                                    })
                                    .on_drop(cx.listener(
                                        move |view, source: &DragChapter, _, cx| {
                                            view.apply_outline(
                                                |outline| outline.move_chapter_to(source.0, id),
                                                cx,
                                            )
                                        },
                                    ))
                                    .on_drop(cx.listener(move |view, source: &DragScene, _, cx| {
                                        view.apply_outline(
                                            |outline| outline.append_scene(source.0, id),
                                            cx,
                                        )
                                    }))
                                    .child(drag_handle(
                                        DragChapter(id),
                                        format!("drag-chapter-{id}"),
                                        &title,
                                    ))
                                    .child(
                                        Button::new(format!("chapter-{id}"))
                                            .label(title.clone())
                                            .when(self.chapter == Some(id), |button| {
                                                button.primary()
                                            })
                                            .on_click(cx.listener(move |view, _, window, cx| {
                                                view.chapter = Some(id);
                                                view.chapter_title.update(cx, |input, cx| {
                                                    input.set_value(title.clone(), window, cx)
                                                });
                                                cx.notify();
                                            })),
                                    )
                                    .when(index > 0, |line| {
                                        line.child(
                                            Button::new(format!("ch-up-{id}"))
                                                .label("Kapitel nach oben")
                                                .on_click(cx.listener(move |view, _, _, cx| {
                                                    view.apply_outline(
                                                        |outline| {
                                                            outline.move_chapter(id, index - 1)
                                                        },
                                                        cx,
                                                    )
                                                })),
                                        )
                                    })
                                    .when(index + 1 < outline.chapters().len(), |line| {
                                        line.child(
                                            Button::new(format!("ch-down-{id}"))
                                                .label("Kapitel nach unten")
                                                .on_click(cx.listener(move |view, _, _, cx| {
                                                    view.apply_outline(
                                                        |outline| {
                                                            outline.move_chapter(id, index + 1)
                                                        },
                                                        cx,
                                                    )
                                                })),
                                        )
                                    })
                                    .test_support(),
                            )
                            .children(chapter.scenes.iter().enumerate().filter_map(
                                |(index, scene)| {
                                    Some(scene_line(
                                        *scene,
                                        &outline.scene(*scene)?.title,
                                        Some((id, index, chapter.scenes.len())),
                                        cx,
                                    ))
                                },
                            ))
                    }),
            )
            .child(
                div()
                    .id("unplaced-scenes")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .drag_over::<DragScene>(|style, _, _, cx| style.bg(cx.theme().muted))
                    .on_drop(cx.listener(|view, source: &DragScene, _, cx| {
                        view.apply_outline(
                            |outline| outline.remove_from_chapter(source.0).map(|_| ()),
                            cx,
                        )
                    }))
                    .child("Ohne Kapitel")
                    .children(
                        outline
                            .unplaced_scenes()
                            .map(|scene| scene_line(scene.id, &scene.title, None, cx)),
                    )
                    .test_support(),
            )
    }

    pub(super) fn load_scene_time(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        let outline = Outline::from_project(project);
        let time = outline
            .scene(id)
            .and_then(|scene| scene.timing.time.as_deref())
            .unwrap_or_default();
        self.scene_time
            .update(cx, |input, cx| input.set_value(time, window, cx));
    }

    pub(super) fn scene_timing(&self, project: &Project, id: Id, cx: &mut Context<Self>) -> Div {
        let outline = Outline::from_project(project);
        let Some(scene) = outline.scene(id) else {
            return div();
        };
        let chapter = outline
            .chapter_of(id)
            .and_then(|chapter| project.document(chapter))
            .map_or("Ohne Kapitel", |chapter| chapter.title());
        let event = scene
            .event
            .and_then(|event| project.document(event))
            .map_or("Offen", |event| event.title());
        let name = |id: Option<Id>| {
            id.and_then(|id| project.document(id))
                .map(|doc| doc.title().to_owned())
                .unwrap_or_else(|| "Offen".into())
        };
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(format!("Kapitel · {chapter}"))
            .child(format!("Chronologie · {event}"))
            .child(format!(
                "Nach {} · Vor {} · {}",
                name(scene.timing.after),
                name(scene.timing.before),
                scene.timing.time.as_deref().unwrap_or_default()
            ))
            .child("Zeitangabe präzisieren")
            .child(Input::new(&self.scene_time))
            .child(
                Button::new("set-scene-time")
                    .label("Zeitangabe sichern")
                    .on_click(cx.listener(move |view, _, _, cx| {
                        let time = optional(view.scene_time.read(cx).value().as_ref());
                        view.update_scene_metadata(id, move |timing, _| timing.time = time, cx);
                    })),
            );
        for (before, label) in [(false, "Nach Ereignis"), (true, "Vor Ereignis")] {
            panel = panel.child(label).child(
                div().flex().flex_wrap().gap_2().children(
                    project
                        .documents()
                        .filter(|doc| doc.kind() == DocumentKind::Event)
                        .map(|doc| {
                            let event = doc.id();
                            Button::new(format!("scene-time-{before}-{event}"))
                                .label(doc.title().to_owned())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.update_scene_metadata(
                                        id,
                                        move |timing, _| {
                                            let bound = if before {
                                                &mut timing.before
                                            } else {
                                                &mut timing.after
                                            };
                                            *bound = (*bound != Some(event)).then_some(event);
                                        },
                                        cx,
                                    )
                                }))
                        }),
                ),
            );
        }
        panel.child("Figuren").child(
            div().flex().flex_wrap().gap_2().children(
                project
                    .documents()
                    .filter(|doc| doc.kind() == DocumentKind::Character)
                    .map(|doc| {
                        let character = doc.id();
                        Button::new(format!("scene-character-{character}"))
                            .label(doc.title().to_owned())
                            .when(scene.characters.contains(&character), |button| {
                                button.primary()
                            })
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.update_scene_metadata(
                                    id,
                                    move |_, characters| {
                                        if let Some(index) =
                                            characters.iter().position(|id| *id == character)
                                        {
                                            characters.remove(index);
                                        } else {
                                            characters.push(character);
                                        }
                                    },
                                    cx,
                                )
                            }))
                    }),
            ),
        )
    }

    fn update_scene_metadata(
        &mut self,
        id: Id,
        update: impl FnOnce(&mut SceneTiming, &mut Vec<Id>),
        cx: &mut Context<Self>,
    ) {
        self.apply_outline(
            |outline| {
                let mut scene = outline
                    .scene(id)
                    .cloned()
                    .ok_or(writer::outline::OutlineError::MissingScene(id))?;
                update(&mut scene.timing, &mut scene.characters);
                outline.update_scene(
                    id,
                    &scene.title,
                    &scene.bullets,
                    scene.characters,
                    scene.timing,
                )
            },
            cx,
        );
    }
}

fn optional(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.trim().to_owned())
}

fn relation_label(kind: Relation) -> &'static str {
    match kind {
        Relation::Before => "Liegt vor",
        Relation::Cause => "Verursacht",
    }
}

#[cfg(test)]
#[path = "planning_tests.rs"]
mod tests;
