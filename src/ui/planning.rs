use std::collections::{BTreeMap, BTreeSet};

use super::*;
use gpui_kit::base::{StyledExt as _, TestSupportExt as _};
use gpui_kit::component::{
    IconName, Selectable, Sizable,
    button::ButtonGroup,
    input::{Input, Textarea},
    menu::{ContextMenuExt, DropdownMenu, PopupMenu, PopupMenuItem},
    resizable::{h_resizable, resizable_panel},
    tag::Tag,
};
use writer::{
    DocumentKind, ProjectError,
    outline::{Chapter, Outline, OutlineError, Scene, SceneTiming},
    timeline::{Event, Relation, Timeline},
};

const CARD_WIDTH: f32 = 148.;
const CARD_HEIGHT: f32 = 54.;
const COLUMN_WIDTH: f32 = 260.;
const ROW_HEIGHT: f32 = 92.;
const GRAPH_MARGIN: f32 = 18.;

#[derive(Default)]
pub(super) struct PlanningView {
    timeline_details: bool,
    timeline_origin: Option<InspectorOrigin>,
    links_open: bool,
    relation_kind: Option<Relation>,
    derived: Option<DerivedScene>,
    recent_scene: Option<Id>,
    scene: Option<Id>,
    outline_details: bool,
    outline_origin: Option<InspectorOrigin>,
    collapsed_chapters: BTreeSet<Id>,
    unplaced_collapsed: bool,
}

struct InspectorOrigin {
    trigger: ElementId,
    focus: FocusHandle,
}

impl InspectorOrigin {
    fn capture(trigger: impl Into<ElementId>, window: &Window, cx: &App) -> Option<Self> {
        Some(Self {
            trigger: trigger.into(),
            focus: window.focused(cx)?,
        })
    }

    fn bind(origin: Option<&Self>, id: impl Into<ElementId>) -> Button {
        let id = id.into();
        let focus = origin
            .filter(|origin| origin.trigger == id)
            .map(|origin| &origin.focus);
        Button::new(id).when_some(focus, |button, focus| button.track_focus(focus))
    }

    fn restore(origin: Option<&Self>, window: &mut Window, cx: &mut App) {
        if let Some(origin) = origin {
            origin.focus.focus(window, cx);
        }
    }
}

struct DerivedScene {
    event: Id,
    title: Entity<InputState>,
    bullets: Entity<TextareaState>,
    time: Entity<InputState>,
    after: Option<Id>,
    before: Option<Id>,
    characters: BTreeSet<Id>,
}

fn workspace_toolbar(title: &'static str, actions: impl IntoElement, cx: &App) -> Div {
    div()
        .flex()
        .flex_shrink_0()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap_2()
        .px_4()
        .py_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().text_lg().font_semibold().child(title))
        .child(actions)
}

fn workspace_panes(
    id: &'static str,
    content: Div,
    inspector: Option<Div>,
    window: &Window,
    cx: &App,
) -> Div {
    let rem = window.rem_size();
    let narrow = window.viewport_size().width < rem * 60.;
    let panes = div().flex().flex_1().min_h_0().min_w_0();
    let content = content
        .flex_1()
        .min_h_0()
        .min_w_0()
        .overflow_y_scrollbar()
        .id(format!("{id}-content"));
    let Some(inspector) = inspector else {
        return panes.child(content);
    };
    let inspector = inspector
        .flex_1()
        .min_h_0()
        .min_w_0()
        .bg(cx.theme().sidebar)
        .overflow_y_scrollbar()
        .id(format!("{id}-inspector"));
    if narrow {
        return panes.child(inspector);
    }
    panes.child(
        h_resizable(format!("{id}-split"))
            .child(resizable_panel().child(content))
            .child(
                resizable_panel()
                    .size(rem * 20.)
                    .size_range((rem * 16.)..(rem * 28.))
                    .fixed()
                    .child(inspector),
            ),
    )
}

fn field_label(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

fn inspector_heading(
    kind: &'static str,
    title: impl Into<SharedString>,
    close: Button,
    cx: &App,
) -> Div {
    div()
        .flex()
        .justify_between()
        .items_start()
        .gap_2()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(field_label(kind, cx))
                .child(div().text_lg().font_semibold().child(title.into())),
        )
        .child(close)
}

struct NetworkArrow {
    start: Point<Pixels>,
    control_a: Point<Pixels>,
    control_b: Point<Pixels>,
    end: Point<Pixels>,
    forward: bool,
    kind: Relation,
}

impl NetworkArrow {
    fn between(from: Point<Pixels>, to: Point<Pixels>, kind: Relation, scale: f32) -> Self {
        let card_width = px(CARD_WIDTH * scale);
        let card_height = CARD_HEIGHT * scale;
        let offset = if kind == Relation::Cause { 10. } else { -10. } * scale;
        let start_y = from.y + px(card_height / 2. + offset);
        let end_y = to.y + px(card_height / 2. + offset);
        let forward = from.x < to.x;
        let (start, control_a, control_b, end) = if from.x == to.x {
            let side = from.x + card_width;
            let outside = side + px(48. * scale);
            (
                point(side, start_y - px(5.)),
                point(outside, start_y - px(5.)),
                point(outside, end_y + px(5.)),
                point(side, end_y + px(5.)),
            )
        } else {
            let (start, end) = if forward {
                (from.x + card_width, to.x)
            } else {
                (from.x, to.x + card_width)
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
        if !before {
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

fn first_bullet(body: &str) -> Option<&str> {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .or_else(|| line.strip_prefix("+ "))
                .unwrap_or(line)
        })
}

fn paint_network_grid(bounds: Bounds<Pixels>, window: &mut Window, cx: &App) {
    let visible = bounds.intersect(&window.content_mask().bounds);
    let step = window.rem_size() * 1.5;
    let color = cx.theme().border.opacity(0.55);
    let mut x = bounds.left() + step * (((visible.left() - bounds.left()) / step).ceil());
    while x < visible.right() {
        window.paint_quad(fill(
            Bounds::new(point(x, visible.top()), size(px(1.), visible.size.height)),
            color,
        ));
        x += step;
    }

    let mut y = bounds.top() + step * (((visible.top() - bounds.top()) / step).ceil());
    while y < visible.bottom() {
        window.paint_quad(fill(
            Bounds::new(point(visible.left(), y), size(visible.size.width, px(1.))),
            color,
        ));
        y += step;
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
        self.planning.timeline_details = true;
        self.planning.links_open = false;
        self.planning.relation_kind = None;
        self.planning.derived = None;
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
        let result = self
            .planning
            .relation_kind
            .filter(|&previous| previous != kind)
            .map_or(Ok(()), |previous| {
                timeline.change_relation_kind(from, to, previous, kind)
            })
            .and_then(|()| timeline.relate_at(from, to, kind, note, time));
        match result {
            Ok(()) => {
                if self.persist_timeline(&timeline, from, cx) {
                    self.planning.relation_kind = Some(kind);
                }
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

    pub(super) fn select_relation(
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
        if self.event != Some(from) {
            self.select_event(from, window, cx);
        } else {
            self.planning.timeline_details = true;
            self.planning.derived = None;
        }
        self.event_target = Some(to);
        self.planning.links_open = true;
        self.planning.relation_kind = Some(kind);
        self.relation_note.update(cx, |input, cx| {
            input.set_value(link.note.clone(), window, cx)
        });
        self.relation_time.update(cx, |input, cx| {
            input.set_value(link.time.clone().unwrap_or_default(), window, cx)
        });
        cx.notify();
    }

    fn event_button(&self, id: Id, title: &str, cx: &mut Context<Self>) -> Button {
        self.event_trigger(id, cx).label(title.to_owned())
    }

    fn event_trigger(&self, id: Id, cx: &mut Context<Self>) -> Button {
        let trigger = format!("event-{id}");
        InspectorOrigin::bind(self.planning.timeline_origin.as_ref(), trigger.clone())
            .toggled(self.event == Some(id))
            .on_click(cx.listener(move |view, _, window, cx| {
                view.planning.timeline_origin =
                    InspectorOrigin::capture(trigger.clone(), window, cx);
                view.select_event(id, window, cx);
            }))
    }

    fn event_card(&self, event: &Event, cx: &mut Context<Self>) -> Button {
        let secondary = event.time.as_deref().or_else(|| first_bullet(&event.body));
        self.event_trigger(event.id, cx)
            .accessibility_label(event.title.clone())
            .w_full()
            .h_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .child(div().truncate().font_semibold().child(event.title.clone()))
                    .children(secondary.map(|line| {
                        div()
                            .text_xs()
                            .truncate()
                            .opacity(0.8)
                            .child(line.to_owned())
                    })),
            )
    }

    fn event_network(
        &self,
        timeline: &Timeline,
        events: &[&Event],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let scale = window.rem_size() / px(16.);
        let columns = timeline.event_columns();
        let mut positions = BTreeMap::new();
        for (column, cards) in columns.iter().enumerate() {
            for (row, event) in cards.iter().enumerate() {
                positions.insert(
                    event.id,
                    point(
                        px((column as f32 * COLUMN_WIDTH + GRAPH_MARGIN) * scale),
                        px((row as f32 * ROW_HEIGHT + GRAPH_MARGIN) * scale),
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
            let arrow = NetworkArrow::between(from, to, link.kind, scale);
            let position = arrow.label_position();
            let (from, to, kind) = (link.from, link.to, link.kind);
            let text = link.time.as_deref().unwrap_or_else(|| relation_label(kind));
            let trigger = format!("network-link-{from}-{to}-{kind:?}");
            labels.push(
                div()
                    .absolute()
                    .left(position.x - px(46. * scale))
                    .top(position.y - px(11. * scale))
                    .w(px(92. * scale))
                    .child(
                        InspectorOrigin::bind(
                            self.planning.timeline_origin.as_ref(),
                            trigger.clone(),
                        )
                        .xsmall()
                        .outline()
                        .w_full()
                        .truncate()
                        .label(text.to_owned())
                        .on_click(cx.listener(
                            move |view, _, window, cx| {
                                view.planning.timeline_origin =
                                    InspectorOrigin::capture(trigger.clone(), window, cx);
                                view.select_relation(from, to, kind, window, cx);
                            },
                        )),
                    ),
            );
            arrows.push(arrow);
        }
        let isolated = events
            .iter()
            .filter(|event| !positions.contains_key(&event.id));
        let mut network = div().flex().flex_col().gap_2();
        if !columns.is_empty() {
            let width =
                ((columns.len() - 1) as f32 * COLUMN_WIDTH + CARD_WIDTH + 2. * GRAPH_MARGIN)
                    * scale;
            let rows = columns.iter().map(Vec::len).max().unwrap_or_default();
            let height = ((rows - 1) as f32 * ROW_HEIGHT + CARD_HEIGHT + 2. * GRAPH_MARGIN)
                .max(128.)
                * scale;
            network = network.child(
                div()
                    .id("event-network")
                    .min_w_0()
                    .overflow_x_scroll()
                    .rounded_sm()
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().muted.opacity(0.35))
                    .test_support()
                    .child(
                        div()
                            .relative()
                            .w(px(width))
                            .min_w_full()
                            .h(px(height))
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, (), window, cx| {
                                        paint_network_grid(bounds, window, cx);
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
                                        .w(px(CARD_WIDTH * scale))
                                        .h(px(CARD_HEIGHT * scale))
                                        .flex()
                                        .items_center()
                                        .child(self.event_card(event, cx)),
                                )
                            }))
                            .children(labels),
                    ),
            );
        }
        if isolated.clone().next().is_some() {
            network = network.child(
                div()
                    .id("timeline-unordered")
                    .flex()
                    .flex_col()
                    .gap_2()
                    .py_2()
                    .child(field_label("Noch ohne zeitliche Einordnung", cx))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children(isolated.map(|event| {
                                div()
                                    .w(rems(CARD_WIDTH / 16.))
                                    .h(rems(CARD_HEIGHT / 16.))
                                    .child(self.event_card(event, cx))
                            })),
                    )
                    .test_support(),
            );
        }
        network.when(!columns.is_empty(), |network| {
            network.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_3()
                    .child(field_label("⟶ Zeitliche Reihenfolge", cx))
                    .child(field_label("┄➝ Ursache", cx)),
            )
        })
    }

    fn begin_event(&mut self, trigger: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.planning.timeline_origin = InspectorOrigin::capture(trigger, window, cx);
        self.event = None;
        self.event_target = None;
        self.planning.timeline_details = true;
        self.planning.links_open = false;
        self.planning.relation_kind = None;
        self.planning.derived = None;
        self.event_title.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus_handle(cx).focus(window, cx);
        });
        self.form_body
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.event_time
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    pub(super) fn timeline_view(
        &self,
        project: &Project,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let timeline = Timeline::from_project(project);
        let events = timeline.ordered_events();
        let is_empty = events.is_empty();
        let new_event = |id: &'static str, label, cx: &mut Context<Self>| {
            InspectorOrigin::bind(self.planning.timeline_origin.as_ref(), id)
                .label(label)
                .on_click(cx.listener(move |view, _, window, cx| view.begin_event(id, window, cx)))
        };
        let actions = div()
            .flex()
            .items_center()
            .gap_2()
            .when(!is_empty, |actions| {
                actions.child(
                    ButtonGroup::new("timeline-mode")
                        .small()
                        .outline()
                        .children([("Netz", false), ("Liste", true)].map(|(label, list)| {
                            Button::new(label)
                                .label(label)
                                .selected(self.event_list == list)
                        }))
                        .on_click(cx.listener(|view, selected: &Vec<usize>, _, cx| {
                            view.event_list = selected == &[1];
                            cx.notify();
                        })),
                )
            })
            .child(new_event("new-event", "+ Ereignis", cx).small());

        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .children(
                timeline
                    .issues()
                    .iter()
                    .map(|issue| div().child(issue.to_string())),
            )
            .when(is_empty, |content| {
                content.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .p_8()
                        .child(new_event("first-event", "Erstes Ereignis anlegen", cx)),
                )
            })
            .when(!is_empty && !self.event_list, |content| {
                content.child(self.event_network(&timeline, &events, window, cx))
            })
            .when(!is_empty && self.event_list, |content| {
                content.children(events.iter().map(|event| {
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
                        .pb_2()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(self.event_button(event.id, &event.title, cx).ghost())
                        .child(field_label(edges, cx))
                }))
            });
        let inspector = self.planning.timeline_details.then(|| {
            if let Some(derived) = &self.planning.derived {
                self.derived_scene_form(project, &timeline, derived, cx)
            } else {
                self.event_inspector(project, &timeline, &events, cx)
            }
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(workspace_toolbar("Timeline", actions, cx))
            .child(workspace_panes("timeline", content, inspector, window, cx))
    }

    fn event_inspector(
        &self,
        project: &Project,
        timeline: &Timeline,
        events: &[&Event],
        cx: &mut Context<Self>,
    ) -> Div {
        let active = self.event.and_then(|id| timeline.event(id));
        let close = Button::new("close-event-inspector")
            .ghost()
            .small()
            .label("Schließen")
            .on_click(cx.listener(|view, _, window, cx| {
                view.planning.timeline_details = false;
                InspectorOrigin::restore(view.planning.timeline_origin.as_ref(), window, cx);
                cx.notify();
            }));
        let mut inspector = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(inspector_heading(
                "Ereignis",
                active.map_or("Neues Ereignis", |event| &event.title),
                close,
                cx,
            ))
            .child(field_label("Titel", cx))
            .child(Input::new(&self.event_title).aria_label("Titel des Ereignisses"))
            .child(field_label("Stichpunkte", cx))
            .child(
                Textarea::new(&self.form_body)
                    .h(rems(4.5))
                    .aria_label("Stichpunkte"),
            )
            .child(field_label("Zeitangabe", cx))
            .child(Input::new(&self.event_time).aria_label("Zeitangabe"))
            .child(
                Button::new("save-event")
                    .primary()
                    .label("Ereignis sichern")
                    .on_click(cx.listener(|view, _, window, cx| view.save_event(window, cx))),
            );
        let Some(event) = active else {
            return inspector;
        };
        let from = event.id;
        let scenes = Outline::from_project(project);
        let linked_scenes = scenes
            .chapters()
            .iter()
            .flat_map(|chapter| chapter.scenes.iter().filter_map(|id| scenes.scene(*id)))
            .chain(scenes.unplaced_scenes())
            .filter(|scene| scene.event == Some(from));
        inspector = inspector
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("derive-scene")
                            .small()
                            .label("Szene ableiten")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.begin_derived_scene(from, window, cx);
                            })),
                    )
                    .child(
                        Button::new("brainstorm-from-event")
                            .small()
                            .label("Lücke erkunden")
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.start_pair(from, window, cx)
                            })),
                    ),
            )
            .children(linked_scenes.map(|scene| {
                let id = scene.id;
                let (button_id, label) = if self.planning.recent_scene == Some(id) {
                    ("open-derived-scene".to_owned(), "Szene öffnen".to_owned())
                } else {
                    (format!("event-scene-{id}"), scene.title.clone())
                };
                Button::new(button_id)
                    .ghost()
                    .small()
                    .label(label)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.select_scene(id, window, cx);
                    }))
            }));
        if events.len() > 1 {
            inspector = inspector.child(
                Button::new("event-link-picker")
                    .small()
                    .label(if self.planning.links_open {
                        "Verbindungen ausblenden"
                    } else {
                        "Verknüpfen …"
                    })
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.planning.links_open = !view.planning.links_open;
                        cx.notify();
                    })),
            );
        }
        if self.planning.links_open {
            inspector = inspector.child(self.event_links(from, timeline, events, cx));
        }
        inspector
    }

    fn event_links(
        &self,
        from: Id,
        timeline: &Timeline,
        events: &[&Event],
        cx: &mut Context<Self>,
    ) -> Div {
        let title = |id| {
            timeline
                .event(id)
                .map_or_else(|| id.to_string(), |event| event.title.clone())
        };
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
                        let (a, b, kind) = (edge.from, edge.to, edge.kind);
                        let label =
                            format!("{} → {} · {}", title(a), title(b), relation_label(kind));
                        div()
                            .flex()
                            .flex_col()
                            .items_start()
                            .gap_1()
                            .child(
                                Button::new(format!("relation-{a}-{b}-{kind:?}"))
                                    .label(label)
                                    .ghost()
                                    .small()
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.select_relation(a, b, kind, window, cx)
                                    })),
                            )
                            .child(
                                Button::new(format!("unlink-{a}-{b}-{kind:?}"))
                                    .label("Verbindung lösen")
                                    .small()
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if !view.flush(cx) {
                                            return;
                                        }
                                        if let Some(project) = &view.project {
                                            let mut timeline = Timeline::from_project(project);
                                            if timeline.remove_relation(a, b, kind)
                                                && view.persist_timeline(&timeline, a, cx)
                                                && view.event == Some(a)
                                                && view.event_target == Some(b)
                                                && view.planning.relation_kind == Some(kind)
                                            {
                                                view.event_target = None;
                                                view.planning.relation_kind = None;
                                            }
                                        }
                                    })),
                            )
                    }),
            )
            .child(field_label("Zielereignis", cx))
            .child(div().flex().flex_wrap().gap_1().children(
                events.iter().filter(|event| event.id != from).map(|event| {
                    let id = event.id;
                    Button::new(format!("target-{id}"))
                        .small()
                        .label(event.title.clone())
                        .toggled(self.event_target == Some(id))
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.event_target = Some(id);
                            view.relation_note.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                            view.relation_time.update(cx, |input, cx| {
                                input.set_value("", window, cx);
                            });
                            cx.notify();
                        }))
                }),
            ))
            .when(self.event_target.is_some(), |panel| {
                panel
                    .child(field_label("Erläuterung", cx))
                    .child(Input::new(&self.relation_note).aria_label("Erläuterung"))
                    .child(field_label("Relative Zeitangabe", cx))
                    .child(Input::new(&self.relation_time).aria_label("Relative Zeitangabe"))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .children([Relation::Before, Relation::Cause].map(|kind| {
                                Button::new(relation_label(kind))
                                    .label(relation_label(kind))
                                    .small()
                                    .on_click(
                                        cx.listener(move |view, _, _, cx| view.relate(kind, cx)),
                                    )
                            })),
                    )
            })
    }

    fn begin_derived_scene(&mut self, from: Id, window: &mut Window, cx: &mut Context<Self>) {
        let bullets = self.form_body.read(cx).value();
        let title = cx.new(|cx| {
            let mut input = InputState::new(window, cx);
            input.set_value(first_bullet(&bullets).unwrap_or("Neue Szene"), window, cx);
            input
        });
        let bullets = cx.new(|cx| {
            let mut input = TextareaState::new(window, cx);
            input.set_value(bullets, window, cx);
            input
        });
        self.planning.derived = Some(DerivedScene {
            event: from,
            title,
            bullets,
            time: cx.new(|cx| InputState::new(window, cx)),
            after: None,
            before: None,
            characters: BTreeSet::new(),
        });
        cx.notify();
    }

    fn save_derived_scene(&mut self, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let Some((project, form)) = self.project.as_ref().zip(self.planning.derived.as_ref())
        else {
            return;
        };
        let title = form.title.read(cx).value();
        if title.trim().is_empty() {
            self.error = Some("Gib der Szene einen Titel.".into());
            cx.notify();
            return;
        }
        let bullets = form.bullets.read(cx).value();
        let time = optional(form.time.read(cx).value().as_ref());
        let timing = SceneTiming {
            after: form.after,
            before: form.before,
            time,
        };
        let mut outline = Outline::from_project(project);
        let id = outline.derive_scene(form.event, &bullets);
        if outline
            .update_scene(
                id,
                title.trim(),
                &bullets,
                form.characters.iter().copied().collect(),
                timing,
            )
            .is_ok()
            && self.persist_outline(&outline, cx)
        {
            self.planning.derived = None;
            self.planning.recent_scene = Some(id);
            cx.notify();
        }
    }

    fn derived_scene_form(
        &self,
        project: &Project,
        timeline: &Timeline,
        form: &DerivedScene,
        cx: &mut Context<Self>,
    ) -> Div {
        let cancel = Button::new("cancel-derived-scene")
            .ghost()
            .small()
            .label("Abbrechen")
            .on_click(cx.listener(|view, _, _, cx| {
                view.planning.derived = None;
                cx.notify();
            }));
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(inspector_heading(
                "Neue Szene",
                "Szene ableiten",
                cancel,
                cx,
            ))
            .child(field_label("Titel", cx))
            .child(Input::new(&form.title).aria_label("Titel der Szene"))
            .child(field_label("Stichpunkte", cx))
            .child(
                Textarea::new(&form.bullets)
                    .h(rems(4.5))
                    .aria_label("Stichpunkte"),
            )
            .child(field_label("Zeitangabe", cx))
            .child(Input::new(&form.time).aria_label("Zeitangabe"));
        for (before, label, selected) in [
            (false, "Nach Ereignis", form.after),
            (true, "Vor Ereignis", form.before),
        ] {
            panel = panel.child(field_label(label, cx)).child(
                div().flex().flex_wrap().gap_1().children(
                    timeline.ordered_events().into_iter().map(|event| {
                        let id = event.id;
                        Button::new(format!("derive-bound-{before}-{id}"))
                            .small()
                            .label(event.title.clone())
                            .toggled(selected == Some(id))
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if let Some(form) = &mut view.planning.derived {
                                    let bound = if before {
                                        &mut form.before
                                    } else {
                                        &mut form.after
                                    };
                                    *bound = (*bound != Some(id)).then_some(id);
                                    cx.notify();
                                }
                            }))
                    }),
                ),
            );
        }
        panel
            .child(field_label("Figuren", cx))
            .child(
                div().flex().flex_wrap().gap_1().children(
                    project
                        .documents()
                        .filter(|doc| doc.kind() == DocumentKind::Character)
                        .map(|doc| {
                            let id = doc.id();
                            Button::new(format!("derive-character-{id}"))
                                .small()
                                .label(doc.title().to_owned())
                                .toggled(form.characters.contains(&id))
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if let Some(form) = &mut view.planning.derived {
                                        if !form.characters.remove(&id) {
                                            form.characters.insert(id);
                                        }
                                        cx.notify();
                                    }
                                }))
                        }),
                ),
            )
            .child(
                Button::new("confirm-derived-scene")
                    .primary()
                    .label("Szene anlegen")
                    .on_click(cx.listener(|view, _, _, cx| view.save_derived_scene(cx))),
            )
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

    pub(super) fn select_outline_chapter(
        &mut self,
        id: Id,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(chapter) = self
            .project
            .as_ref()
            .and_then(|project| project.document(id))
        else {
            return;
        };
        self.chapter = Some(id);
        self.planning.scene = None;
        self.planning.outline_details = true;
        self.chapter_title.update(cx, |input, cx| {
            input.set_value(chapter.title().to_owned(), window, cx);
        });
        cx.notify();
    }

    fn edit_outline_chapter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.chapter_title.read(cx).value();
        let title = title.trim();
        if title.is_empty() {
            self.error = Some("Gib dem Kapitel einen Titel.".into());
            cx.notify();
            return;
        }
        let selected = self.chapter;
        self.apply_outline(
            |outline| match selected {
                Some(id) => outline.rename_chapter(id, title),
                None => {
                    outline.create_chapter(title);
                    Ok(())
                }
            },
            cx,
        );
        if selected.is_none() && self.error.is_none() {
            self.planning.outline_details = false;
            InspectorOrigin::restore(self.planning.outline_origin.as_ref(), window, cx);
            cx.notify();
        }
    }

    pub(super) fn outline_view(
        &self,
        project: &Project,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let outline = Outline::from_project(project);
        let actions = InspectorOrigin::bind(self.planning.outline_origin.as_ref(), "new-chapter")
            .small()
            .label("+ Kapitel")
            .on_click(cx.listener(|view, _, window, cx| {
                view.planning.outline_origin = InspectorOrigin::capture("new-chapter", window, cx);
                view.chapter = None;
                view.planning.scene = None;
                view.planning.outline_details = true;
                view.chapter_title.update(cx, |input, cx| {
                    input.set_value("", window, cx);
                    input.focus_handle(cx).focus(window, cx);
                });
                cx.notify();
            }));
        let mut list = div().flex().flex_col().gap_2().p_4().children(
            outline
                .issues()
                .iter()
                .map(|issue| div().child(issue.to_string())),
        );
        for (index, chapter) in outline.chapters().iter().enumerate() {
            let id = chapter.id;
            let expanded = !self.planning.collapsed_chapters.contains(&id);
            let row =
                self.outline_chapter_row(index, chapter, outline.chapters().len(), expanded, cx);
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .pb_2()
                    .child(row)
                    .when(expanded, |group| {
                        group.children(chapter.scenes.iter().enumerate().filter_map(
                            |(position, scene)| {
                                Some(self.outline_scene_row(
                                    project,
                                    &outline,
                                    outline.scene(*scene)?,
                                    Some((id, position, chapter.scenes.len())),
                                    cx,
                                ))
                            },
                        ))
                    }),
            );
        }
        let expanded = !self.planning.unplaced_collapsed;
        let unplaced = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id("unplaced-scenes")
                    .flex()
                    .items_center()
                    .gap_1()
                    .drag_over::<DragScene>(|style, _, _, cx| style.bg(cx.theme().muted))
                    .on_drop(cx.listener(|view, source: &DragScene, _, cx| {
                        view.apply_outline(
                            |outline| outline.remove_from_chapter(source.0).map(|_| ()),
                            cx,
                        )
                    }))
                    .child(
                        disclosure_button("unplaced-disclosure", "Szenen ohne Kapitel", expanded)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.planning.unplaced_collapsed =
                                    !view.planning.unplaced_collapsed;
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().font_semibold().child("Ohne Kapitel"))
                    .test_support(),
            )
            .when(expanded, |group| {
                group.children(
                    outline
                        .unplaced_scenes()
                        .map(|scene| self.outline_scene_row(project, &outline, scene, None, cx)),
                )
            });
        let inspector = self.planning.outline_details.then(|| {
            if let Some(scene) = self.planning.scene.and_then(|id| outline.scene(id)) {
                self.outline_scene_inspector(project, &outline, scene, cx)
            } else {
                self.outline_chapter_inspector(&outline, cx)
            }
        });
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(workspace_toolbar("Kapitel", actions, cx))
            .child(workspace_panes(
                "outline",
                list.child(unplaced),
                inspector,
                window,
                cx,
            ))
    }

    fn outline_chapter_row(
        &self,
        index: usize,
        chapter: &Chapter,
        count: usize,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = chapter.id;
        let trigger = format!("chapter-{id}");
        div()
            .id(format!("chapter-row-{id}"))
            .flex()
            .items_center()
            .gap_1()
            .drag_over::<DragChapter>(|style, _, _, cx| style.bg(cx.theme().muted))
            .drag_over::<DragScene>(|style, _, _, cx| style.bg(cx.theme().muted))
            .on_drop(cx.listener(move |view, source: &DragChapter, _, cx| {
                view.apply_outline(|outline| outline.move_chapter_to(source.0, id), cx)
            }))
            .on_drop(cx.listener(move |view, source: &DragScene, _, cx| {
                view.apply_outline(|outline| outline.append_scene(source.0, id), cx)
            }))
            .child(
                disclosure_button(format!("chapter-disclosure-{id}"), &chapter.title, expanded)
                    .on_click(cx.listener(move |view, _, _, cx| {
                        if !view.planning.collapsed_chapters.remove(&id) {
                            view.planning.collapsed_chapters.insert(id);
                        }
                        cx.notify();
                    })),
            )
            .child(drag_handle(
                DragChapter(id),
                format!("drag-chapter-{id}"),
                &chapter.title,
            ))
            .child(
                InspectorOrigin::bind(self.planning.outline_origin.as_ref(), trigger.clone())
                    .small()
                    .ghost()
                    .min_w_0()
                    .truncate()
                    .label(format!("{}. {}", index + 1, chapter.title))
                    .toggled(self.chapter == Some(id) && self.planning.scene.is_none())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.planning.outline_origin =
                            InspectorOrigin::capture(trigger.clone(), window, cx);
                        view.select_outline_chapter(id, window, cx);
                    })),
            )
            .test_support()
            .context_menu(outline_menu(chapter_commands(id, index, count), cx))
    }

    fn outline_scene_row(
        &self,
        project: &Project,
        outline: &Outline,
        scene: &Scene,
        position: Option<(Id, usize, usize)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = scene.id;
        let trigger = format!("scene-{id}");
        let concept = project.scene(id).is_some_and(|document| {
            !SceneText::from_markdown(id, document.body())
                .concepts()
                .is_empty()
        });
        div()
            .id(format!("scene-row-{id}"))
            .flex()
            .items_center()
            .gap_1()
            .pl_8()
            .child(drag_handle(
                DragScene(id),
                format!("drag-scene-{id}"),
                &scene.title,
            ))
            .child(
                InspectorOrigin::bind(self.planning.outline_origin.as_ref(), trigger.clone())
                    .small()
                    .ghost()
                    .min_w_0()
                    .truncate()
                    .label(scene.title.clone())
                    .toggled(self.planning.scene == Some(id))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.planning.outline_origin =
                            InspectorOrigin::capture(trigger.clone(), window, cx);
                        view.planning.scene = Some(id);
                        view.planning.outline_details = true;
                        cx.notify();
                    })),
            )
            .when(concept, |row| {
                row.child(Tag::warning().xsmall().child("Konzept"))
            })
            .when_some(position, |line, (chapter, _, _)| {
                line.drag_over::<DragScene>(|style, _, _, cx| style.bg(cx.theme().muted))
                    .on_drop(cx.listener(move |view, source: &DragScene, _, cx| {
                        view.apply_outline(
                            |outline| outline.place_at_scene(source.0, chapter, id),
                            cx,
                        )
                    }))
            })
            .test_support()
            .context_menu(outline_menu(scene_commands(outline, id, position), cx))
    }

    fn close_outline_inspector(&self, cx: &mut Context<Self>) -> Button {
        Button::new("close-outline-inspector")
            .small()
            .ghost()
            .label("Schließen")
            .on_click(cx.listener(|view, _, window, cx| {
                view.planning.outline_details = false;
                InspectorOrigin::restore(view.planning.outline_origin.as_ref(), window, cx);
                cx.notify();
            }))
    }

    fn outline_chapter_inspector(&self, outline: &Outline, cx: &mut Context<Self>) -> Div {
        let chapter = self.chapter.and_then(|id| {
            outline
                .chapters()
                .iter()
                .enumerate()
                .find(|(_, chapter)| chapter.id == id)
        });
        let close = self.close_outline_inspector(cx);
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(inspector_heading(
                "Kapitel",
                chapter.map_or("Neues Kapitel", |(_, chapter)| &chapter.title),
                close,
                cx,
            ))
            .child(field_label("Titel", cx))
            .child(Input::new(&self.chapter_title).aria_label("Kapiteltitel"))
            .child(
                Button::new(if chapter.is_some() {
                    "chapter-rename"
                } else {
                    "chapter-add"
                })
                .primary()
                .label(if chapter.is_some() {
                    "Umbenennen"
                } else {
                    "Kapitel anlegen"
                })
                .on_click(cx.listener(|view, _, window, cx| view.edit_outline_chapter(window, cx))),
            )
            .when_some(chapter, |panel, (index, chapter)| {
                panel.child(
                    Button::new("chapter-actions")
                        .small()
                        .label("Kapitel verschieben")
                        .dropdown_menu(outline_menu(
                            chapter_move_commands(chapter.id, index, outline.chapters().len()),
                            cx,
                        )),
                )
            })
    }

    fn outline_scene_inspector(
        &self,
        project: &Project,
        outline: &Outline,
        scene: &Scene,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = scene.id;
        let close = self.close_outline_inspector(cx);
        let position =
            outline
                .chapters()
                .iter()
                .enumerate()
                .find_map(|(chapter_number, chapter)| {
                    let index = chapter.scenes.iter().position(|&scene| scene == id)?;
                    Some((chapter_number, chapter, index))
                });
        let placement = position.map_or_else(
            || "Ohne Kapitel".to_owned(),
            |(chapter_number, chapter, index)| {
                format!(
                    "Kapitel {} · {}\nSzene {}",
                    chapter_number + 1,
                    chapter.title,
                    index + 1
                )
            },
        );
        let links = [
            (scene.event, "outline-timeline-event", ""),
            (scene.timing.after, "outline-timeline-after", "Nach "),
            (scene.timing.before, "outline-timeline-before", "Vor "),
        ]
        .into_iter()
        .filter_map(|(event, key, prefix)| {
            let target = project.document(event?)?;
            let event = target.id();
            Some(
                Button::new(format!("{key}-{event}"))
                    .ghost()
                    .small()
                    .label(format!("{prefix}{}", target.title()))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.navigate(Page::Timeline, window, cx);
                        if view.page == Page::Timeline {
                            view.select_event(event, window, cx);
                        }
                    })),
            )
        })
        .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .child(inspector_heading("Szene", scene.title.clone(), close, cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(field_label("In der Erzählung", cx))
                    .child(
                        div()
                            .id("outline-scene-position")
                            .child(placement)
                            .test_support(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_1()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .pt_2()
                    .child(field_label("In der Timeline", cx))
                    .when(links.is_empty() && scene.timing.time.is_none(), |panel| {
                        panel.child("Zeitlich offen")
                    })
                    .children(links)
                    .children(scene.timing.time.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("outline-edit-scene")
                            .label("Szene bearbeiten")
                            .primary()
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.select_scene(id, window, cx)
                            })),
                    )
                    .child(
                        Button::new("outline-scene-actions")
                            .label("Verschieben …")
                            .dropdown_menu(outline_menu(
                                scene_commands(
                                    outline,
                                    id,
                                    position.map(|(_, chapter, index)| {
                                        (chapter.id, index, chapter.scenes.len())
                                    }),
                                ),
                                cx,
                            )),
                    ),
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

fn disclosure_button(id: impl Into<ElementId>, title: &str, expanded: bool) -> Button {
    let action = if expanded { "einklappen" } else { "aufklappen" };
    Button::new(id)
        .small()
        .ghost()
        .icon(if expanded {
            IconName::ChevronDown
        } else {
            IconName::ChevronRight
        })
        .accessibility_label(format!("{title} {action}"))
}

#[derive(Clone, Copy)]
enum OutlineCommand {
    Rename(Id),
    ChapterPosition(Id, usize),
    ScenePosition(Id, Id, usize),
    PlaceInChapter(Id, Id),
    RemoveFromChapter(Id),
}

impl OutlineCommand {
    fn apply(self, outline: &mut Outline) -> Result<(), OutlineError> {
        match self {
            Self::Rename(_) => Ok(()),
            Self::ChapterPosition(id, position) => outline.move_chapter(id, position),
            Self::ScenePosition(id, chapter, position) => outline.place(id, chapter, position),
            Self::PlaceInChapter(id, chapter) => outline.append_scene(id, chapter),
            Self::RemoveFromChapter(id) => outline.remove_from_chapter(id).map(|_| ()),
        }
    }
}

fn chapter_move_commands(id: Id, index: usize, count: usize) -> Vec<(String, OutlineCommand)> {
    let mut commands = Vec::new();
    if index > 0 {
        commands.push((
            "Nach oben".into(),
            OutlineCommand::ChapterPosition(id, index - 1),
        ));
    }
    if index + 1 < count {
        commands.push((
            "Nach unten".into(),
            OutlineCommand::ChapterPosition(id, index + 1),
        ));
    }
    commands
}

fn chapter_commands(id: Id, index: usize, count: usize) -> Vec<(String, OutlineCommand)> {
    let mut commands = vec![("Umbenennen".into(), OutlineCommand::Rename(id))];
    commands.extend(chapter_move_commands(id, index, count));
    commands
}

fn scene_commands(
    outline: &Outline,
    id: Id,
    position: Option<(Id, usize, usize)>,
) -> Vec<(String, OutlineCommand)> {
    let mut commands = Vec::new();
    if let Some((chapter, index, total)) = position {
        if index > 0 {
            commands.push((
                "Nach oben".into(),
                OutlineCommand::ScenePosition(id, chapter, index - 1),
            ));
        }
        if index + 1 < total {
            commands.push((
                "Nach unten".into(),
                OutlineCommand::ScenePosition(id, chapter, index + 1),
            ));
        }
        commands.push((
            "Aus Kapitel nehmen".into(),
            OutlineCommand::RemoveFromChapter(id),
        ));
    }
    commands.extend(
        outline
            .chapters()
            .iter()
            .filter(|chapter| position.is_none_or(|(current, _, _)| current != chapter.id))
            .map(|chapter| {
                (
                    format!("In „{}“ verschieben", chapter.title),
                    OutlineCommand::PlaceInChapter(id, chapter.id),
                )
            }),
    );
    commands
}

fn outline_menu(
    commands: Vec<(String, OutlineCommand)>,
    cx: &Context<ProjectView>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + use<> {
    let view = cx.entity().downgrade();
    move |menu, _, _| {
        commands.iter().fold(menu, |menu, (label, command)| {
            let view = view.clone();
            let command = *command;
            menu.item(
                PopupMenuItem::new(label.clone()).on_click(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| match command {
                        OutlineCommand::Rename(id) => view.select_outline_chapter(id, window, cx),
                        command => view.apply_outline(|outline| command.apply(outline), cx),
                    });
                }),
            )
        })
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
