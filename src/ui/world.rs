use super::*;
use gpui_kit::base::StyledExt;
use gpui_kit::component::{Icon, IconName, Selectable, WindowExt};
use writer::{
    Document, DocumentKind,
    passage::Anchor,
    sources::{Resolution, Sources},
};

const KINDS: [(DocumentKind, &str, &str, &str); 4] = [
    (DocumentKind::Character, "Figur", "Figuren", "character"),
    (DocumentKind::Place, "Ort", "Orte", "place"),
    (DocumentKind::Concept, "Konzept", "Konzepte", "concept"),
    (DocumentKind::Idea, "Idee", "Ideen", "idea"),
];

pub(super) struct WorldView {
    selected: Option<Id>,
    creating: bool,
    collapsed: [bool; KINDS.len()],
    directory_search: Entity<InputState>,
    first_event: Option<Id>,
    title: Entity<InputState>,
    body: Entity<TextareaState>,
    claim: Entity<InputState>,
    source_search: Entity<InputState>,
    source_query: String,
}

impl WorldView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ProjectView>) -> Self {
        let directory_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Einträge suchen"));
        cx.subscribe(&directory_search, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        })
        .detach();
        Self {
            selected: None,
            creating: false,
            collapsed: [false; KINDS.len()],
            directory_search,
            first_event: None,
            title: cx.new(|cx| InputState::new(window, cx)),
            body: cx.new(|cx| TextareaState::new(window, cx).rows(9)),
            claim: cx.new(|cx| InputState::new(window, cx)),
            source_search: cx
                .new(|cx| InputState::new(window, cx).placeholder("Textstelle suchen")),
            source_query: String::new(),
        }
    }

    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<ProjectView>) {
        self.selected = None;
        self.creating = false;
        self.collapsed = [false; KINDS.len()];
        self.source_query.clear();
        self.directory_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.source_search
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.clear_form(window, cx);
    }

    fn clear_form(&mut self, window: &mut Window, cx: &mut Context<ProjectView>) {
        self.first_event = None;
        self.title
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.body
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.claim
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn enter(&mut self, document: &Document, window: &mut Window, cx: &mut Context<ProjectView>) {
        self.selected = Some(document.id());
        self.creating = false;
        self.first_event = None;
        self.title.update(cx, |input, cx| {
            input.set_value(document.title().to_owned(), window, cx)
        });
        self.body.update(cx, |input, cx| {
            input.set_value(document.body().to_owned(), window, cx)
        });
        self.claim
            .update(cx, |input, cx| input.set_value("", window, cx));
    }
}

impl ProjectView {
    fn open_world(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self
            .project
            .as_ref()
            .and_then(|project| project.document(id))
        else {
            return;
        };
        self.world.enter(document, window, cx);
        cx.notify();
    }

    fn begin_world_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.world.creating = true;
        self.world.clear_form(window, cx);
        self.world
            .title
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn cancel_world_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.world.creating = false;
        if let Some(id) = self.world.selected {
            self.open_world(id, window, cx);
        }
        self.world
            .directory_search
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    fn save_world(
        &mut self,
        kind: &str,
        existing: Option<Id>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.flush(cx) {
            return;
        }
        let title = self.world.title.read(cx).value();
        if title.trim().is_empty() {
            self.error = Some("Gib dem Karteieintrag einen Titel.".into());
            cx.notify();
            return;
        }
        let body = self.world.body.read(cx).value();
        let Some((project, store)) = self.projects_mut() else {
            return;
        };
        let previous = existing.and_then(|id| project.document(id));
        let id = existing.unwrap_or_else(Id::new_v4);
        let path = previous
            .map(|document| store.relative_path(document))
            .unwrap_or_else(|| Ok(PathBuf::from("Welt").join(format!("{id}.md"))));
        let result = path.and_then(|path| {
            store.stage_document(
                &path,
                previous,
                serde_json::json!({
                    "id": id, "kind": kind, "title": title.trim(),
                }),
                &body,
            )
        });
        if self.report(result, cx) && self.flush(cx) {
            self.open_world(id, window, cx);
        }
    }

    pub(super) fn jump_to(&mut self, anchor: &Anchor, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        let result = self.refresh_project();
        if !self.report(result, cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let sources = Sources::build(project);
        if let Resolution::Unique { document, range } = sources.resolve(anchor) {
            match project.document(document).map(Document::kind) {
                Some(DocumentKind::Scene) => {
                    self.select_scene(document, window, cx);
                    self.editor.update(cx, |editor, cx| {
                        editor.set_selected_range(range, cx);
                        editor.focus(window, cx);
                    });
                }
                Some(DocumentKind::Event) => {
                    self.navigate(Page::Timeline, window, cx);
                    self.select_event(document, window, cx);
                    self.form_body.update(cx, |editor, cx| {
                        editor.set_selected_range(range, cx);
                        editor.focus(window, cx);
                    });
                }
                Some(kind) if KINDS.iter().any(|(candidate, _, _, _)| *candidate == kind) => {
                    self.navigate(Page::World, window, cx);
                    self.open_world(document, window, cx);
                    self.world.body.update(cx, |editor, cx| {
                        editor.set_selected_range(range, cx);
                        editor.focus(window, cx);
                    });
                }
                Some(DocumentKind::Brainstorm) => {
                    self.navigate(Page::Codex, window, cx);
                    let message = self
                        .project
                        .as_ref()
                        .and_then(|project| project.document(document))
                        .and_then(|doc| {
                            writer::ai::brainstorm::Brainstorm::marked_idea_ranges(doc)
                                .ok()?
                                .into_iter()
                                .find(|idea| {
                                    idea.range.start <= range.start && idea.range.end >= range.end
                                })
                                .map(|idea| idea.message)
                        });
                    self.jump_brainstorm(document, message, window, cx);
                }
                _ => {}
            }
            return;
        }
        let candidates = sources.occurrences(anchor);
        if candidates.len() > 1 {
            self.choose_source_occurrence(anchor, candidates, window, cx);
            return;
        }
        self.error =
            Some("Die zitierte Textstelle ist in der Quelle nicht mehr auffindbar.".into());
        cx.notify();
    }

    fn choose_source_occurrence(
        &self,
        anchor: &Anchor,
        candidates: Vec<(Anchor, usize, usize)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let quote = SharedString::from(anchor.quote());
        let view = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title("Fundstelle auswählen")
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(div().child(quote.clone()))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .max_h(rems(20.))
                                .children(candidates.iter().map(|(anchor, line, column)| {
                                    let anchor = anchor.clone();
                                    let view = view.clone();
                                    Button::new(format!("source-occurrence-{line}-{column}"))
                                        .w_full()
                                        .label(format!("Zeile {line}, Spalte {column}"))
                                        .on_click(move |_, window, cx| {
                                            window.close_dialog(cx);
                                            view.update(cx, |view, cx| {
                                                view.jump_to(&anchor, window, cx)
                                            })
                                            .ok();
                                        })
                                }))
                                .overflow_y_scrollbar()
                                .id("source-occurrences"),
                        ),
                )
                .footer(
                    div().flex().justify_end().child(
                        Button::new("cancel-source-occurrence")
                            .ghost()
                            .label("Abbrechen")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
                )
        });
    }

    pub(super) fn world_view(
        &self,
        project: &Project,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let width = window.viewport_size().width;
        let side_by_side = width >= window.rem_size() * 55.;
        let split_inspector = width >= window.rem_size() * 80.;
        let border = cx.theme().border;
        let selected = self.world.selected.and_then(|id| project.document(id));
        let directory = div()
            .min_w_0()
            .when(side_by_side, |list| {
                list.w(rems(14.))
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(border)
                    .pr_3()
            })
            .child(self.world_directory(project, cx));
        let card = self.world_card(project, selected, cx);
        let inspector = div()
            .min_w_0()
            .when(split_inspector, |inspector| {
                inspector
                    .w(rems(21.))
                    .flex_shrink_0()
                    .border_l_1()
                    .border_color(border)
                    .pl_4()
            })
            .when(!split_inspector, |inspector| {
                inspector.border_t_1().border_color(border).pt_4()
            })
            .child(self.world_sources(project, cx))
            .child(
                div()
                    .mt_4()
                    .pt_4()
                    .border_t_1()
                    .border_color(border)
                    .child(self.research_page(project, cx)),
            );
        let mut detail = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap_4()
            .child(card);
        let adjacent_inspector = if split_inspector {
            Some(inspector)
        } else {
            detail = detail.child(inspector);
            None
        };
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_xl().font_semibold().child("Weltkartei"))
            .child(
                div()
                    .flex()
                    .gap_4()
                    .when(!side_by_side, |panes| panes.flex_col())
                    .child(directory)
                    .child(detail)
                    .children(adjacent_inspector),
            )
    }

    fn world_directory(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let query = self
            .world
            .directory_search
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let mut matches = 0;
        let groups: Vec<_> = KINDS
            .into_iter()
            .enumerate()
            .filter_map(|(index, (kind, _, group, _))| {
                let mut entries: Vec<_> = project
                    .documents()
                    .filter(|document| {
                        document.kind() == kind
                            && (query.is_empty()
                                || document.title().to_lowercase().contains(&query)
                                || document.body().to_lowercase().contains(&query))
                    })
                    .collect();
                if entries.is_empty() && !query.is_empty() {
                    return None;
                }
                entries.sort_by_cached_key(|document| document.title().to_lowercase());
                matches += entries.len();
                let expanded =
                    !entries.is_empty() && (!query.is_empty() || !self.world.collapsed[index]);
                Some(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            Button::new(("world-group", index))
                                .ghost()
                                .w_full()
                                .icon(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .label(group)
                                .toggled(expanded)
                                .disabled(entries.is_empty() || !query.is_empty())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.world.collapsed[index] = !view.world.collapsed[index];
                                    cx.notify();
                                })),
                        )
                        .when(expanded, |group| {
                            group.child(div().flex().flex_col().pl_3().children(
                                entries.into_iter().map(|document| {
                                    let id = document.id();
                                    Button::new(format!("world-{id}"))
                                        .ghost()
                                        .w_full()
                                        .label(document.title().to_owned())
                                        .tooltip(document.title().to_owned())
                                        .selected(
                                            !self.world.creating && self.world.selected == Some(id),
                                        )
                                        .on_click(cx.listener(move |view, _, window, cx| {
                                            view.open_world(id, window, cx)
                                        }))
                                }),
                            ))
                        }),
                )
            })
            .collect();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Input::new(&self.world.directory_search)
                    .id("world-entry-search")
                    .aria_label("Karteieinträge suchen")
                    .prefix(Icon::new(IconName::Search)),
            )
            .children(groups)
            .when(matches == 0, |list| {
                list.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if query.is_empty() {
                            "Noch keine Einträge"
                        } else {
                            "Keine passenden Einträge"
                        }),
                )
            })
            .child(
                Button::new("clear-world")
                    .icon(IconName::Plus)
                    .label("Eintrag")
                    .disabled(self.world.creating)
                    .on_click(cx.listener(|view, _, window, cx| view.begin_world_card(window, cx))),
            )
    }

    fn world_card(
        &self,
        project: &Project,
        selected: Option<&Document>,
        cx: &mut Context<Self>,
    ) -> Div {
        let muted = cx.theme().muted_foreground;
        let mut card = div().flex().flex_col().gap_3();
        if !self.world.creating && selected.is_none() {
            return card.child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child("Wähle einen Karteieintrag aus oder lege einen neuen an."),
            );
        }
        let selected = selected
            .filter(|_| !self.world.creating)
            .and_then(|document| {
                KINDS
                    .iter()
                    .find(|(kind, _, _, _)| *kind == document.kind())
                    .map(|(_, _, _, wire)| (document, *wire))
            });
        card = card
            .child(
                div().text_lg().font_semibold().child(
                    selected
                        .map_or("Neuer Eintrag", |(document, _)| document.title())
                        .to_owned(),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child("Titel")
                    .child(Input::new(&self.world.title).aria_label("Titel des Karteieintrags")),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        if selected.is_some_and(|(doc, _)| doc.kind() == DocumentKind::Character) {
                            "Figurennotiz"
                        } else {
                            "Notiz"
                        },
                    )
                    .child(Textarea::new(&self.world.body).aria_label("Notiz zum Karteieintrag")),
            );
        if let Some((document, kind)) = selected {
            let id = document.id();
            card.child(
                div().flex().child(
                    Button::new("update-world")
                        .label("Änderung sichern")
                        .primary()
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.save_world(kind, Some(id), window, cx)
                        })),
                ),
            )
            .when(document.kind() == DocumentKind::Character, |card| {
                card.child(
                    div()
                        .pt_4()
                        .border_t_1()
                        .border_color(cx.theme().border)
                        .child(self.knowledge_view(project, id, cx)),
                )
            })
        } else {
            card.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(KINDS.into_iter().map(|(_, label, _, kind)| {
                        Button::new(kind)
                            .label(format!("{label} anlegen"))
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.save_world(kind, None, window, cx)
                            }))
                    }))
                    .child(
                        Button::new("cancel-world")
                            .ghost()
                            .label("Abbrechen")
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.cancel_world_card(window, cx)
                            })),
                    ),
            )
        }
    }

    fn world_sources(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let mut inspector = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(div().text_lg().font_semibold().child("Quellen"))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(div().min_w_0().flex_1().child(
                        Input::new(&self.world.source_search).aria_label("Textstelle suchen"),
                    ))
                    .child(
                        Button::new("source-search")
                            .label("Suchen")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.world.source_query = view
                                    .world
                                    .source_search
                                    .read(cx)
                                    .value()
                                    .trim()
                                    .to_lowercase();
                                let result = view.refresh_project();
                                view.report(result, cx);
                            })),
                    ),
            );
        if !self.world.source_query.is_empty() {
            let sources = writer::sources::Sources::build(project);
            let matches: Vec<_> = sources
                .entries()
                .iter()
                .enumerate()
                .filter(|(_, source)| {
                    source
                        .anchor
                        .quote()
                        .to_lowercase()
                        .contains(&self.world.source_query)
                })
                .collect();
            inspector = inspector
                .when(matches.is_empty(), |inspector| {
                    inspector.child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Keine passenden Textstellen"),
                    )
                })
                .children(matches.into_iter().map(|(index, source)| {
                    self.source_button(index, &source.title, &source.anchor, cx)
                        .w_full()
                }));
        }
        inspector
    }

    pub(super) fn source_button(
        &self,
        index: usize,
        title: &str,
        anchor: &writer::passage::Anchor,
        cx: &mut Context<Self>,
    ) -> Button {
        let anchor = anchor.clone();
        Button::new(("source", index))
            .label(format!(
                "{title} · {}",
                anchor.quote().chars().take(96).collect::<String>()
            ))
            .tooltip(format!("{title} · {}", anchor.quote()))
            .on_click(cx.listener(move |view, _, window, cx| view.jump_to(&anchor, window, cx)))
    }
}

impl ProjectView {
    fn save_claim(
        &mut self,
        character: Id,
        state: writer::knowledge::ClaimState,
        cx: &mut Context<Self>,
    ) {
        use writer::knowledge::{Claim, ClaimHolder, Knowledge, KnowledgeKind};
        let text = self.world.claim.read(cx).value();
        if text.trim().is_empty() || !self.flush(cx) {
            return;
        }
        let event = self.world.first_event;
        let Some((project, store)) = self.projects_mut() else {
            return;
        };
        let mut knowledge = Knowledge::from_project(project);
        let holder = project
            .document(character)
            .map_or("Figur", |doc| doc.title());
        knowledge.create(
            KnowledgeKind::Concept,
            &format!("{holder} · {}", text.chars().take(36).collect::<String>()),
            &text,
            Some(Claim {
                holder: ClaimHolder::Character(character),
                state,
                from: event,
            }),
        );
        let result = knowledge.stage_changes(project, store);
        if self.report(result, cx) {
            self.flush(cx);
        }
    }

    fn knowledge_view(&self, project: &Project, character: Id, cx: &mut Context<Self>) -> Div {
        use writer::knowledge::{ClaimHolder, ClaimState, Knowledge};
        let knowledge = Knowledge::from_project(project);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_lg()
                    .font_semibold()
                    .child("Wissen und Überzeugungen"),
            )
            .children(knowledge.entries().filter_map(|entry| {
                let claim = entry
                    .claim
                    .as_ref()
                    .filter(|claim| claim.holder == ClaimHolder::Character(character))?;
                let event = claim
                    .from
                    .and_then(|id| project.document(id))
                    .map_or("Zeitlich offen", |doc| doc.title());
                let id = entry.id;
                Some(
                    Button::new(format!("claim-{id}"))
                        .label(format!(
                            "{event} · {} · {}",
                            match claim.state {
                                ClaimState::Knows => "Weiß",
                                ClaimState::Believes => "Glaubt",
                            },
                            entry.body
                        ))
                        .on_click(
                            cx.listener(move |view, _, window, cx| view.open_world(id, window, cx)),
                        ),
                )
            }))
            .child(Input::new(&self.world.claim).aria_label("Wissen oder Überzeugung"))
            .child(
                div().flex().flex_wrap().gap_2().children(
                    project
                        .documents()
                        .filter(|doc| doc.kind() == DocumentKind::Event)
                        .map(|doc| {
                            let id = doc.id();
                            Button::new(format!("claim-event-{id}"))
                                .label(doc.title().to_owned())
                                .when(self.world.first_event == Some(id), |button| {
                                    button.primary()
                                })
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    view.world.first_event =
                                        (view.world.first_event != Some(id)).then_some(id);
                                    cx.notify();
                                }))
                        }),
                ),
            )
            .child(
                div().flex().gap_2().children(
                    [
                        ("Weiß", ClaimState::Knows),
                        ("Glaubt", ClaimState::Believes),
                    ]
                    .into_iter()
                    .map(|(label, state)| {
                        Button::new(label).label(label).on_click(
                            cx.listener(move |view, _, _, cx| {
                                view.save_claim(character, state, cx)
                            }),
                        )
                    }),
                ),
            )
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::{Page, test_support};
    use gpui_kit::{
        Focusable, TestAppContext,
        test::{TestQueryExt, TestWindowExt},
    };
    use writer::{
        DocumentKind, Id, Project,
        knowledge::{Claim, ClaimHolder, ClaimState, Knowledge},
    };

    #[gpui_kit::test]
    fn world_directory_filters_by_text_and_opens_the_matching_card(cx: &mut TestAppContext) {
        let workspace = test_support::open(cx);
        let world = workspace.root.join("Welt");
        fs::create_dir(&world).unwrap();
        fs::write(
            world.join("Astrid.md"),
            "---\nid: 00000000-0000-4000-8000-000000000051\nkind: character\ntitle: Astrid\n---\nAstrid trägt den Türschlüssel.\n",
        )
        .unwrap();
        fs::write(
            world.join("Station.md"),
            "---\nid: 00000000-0000-4000-8000-000000000052\nkind: place\ntitle: Station\n---\nUmkreist einen Mond.\n",
        )
        .unwrap();
        workspace.update(cx, |view, _, _| view.refresh_project().unwrap());

        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(("page", 4usize), cx);
                assert!(window.try_find_by_label("Station").is_some());
                window.click("world-entry-search", cx);
                window.input("TÜRSCHLÜSSEL", cx);
                window.render_frame(cx);
                assert!(window.try_find_by_label("Station").is_none());
                window.click("world-00000000-0000-4000-8000-000000000051", cx);
            })
            .unwrap();
        workspace.update(cx, |view, _, cx| {
            assert_eq!(view.world.title.read(cx).value(), "Astrid");
            assert_eq!(
                view.world.body.read(cx).value().trim(),
                "Astrid trägt den Türschlüssel."
            );
        });
    }

    #[gpui_kit::test]
    fn world_card_and_timed_belief_are_saved_to_the_project(cx: &mut TestAppContext) {
        let workspace = test_support::open(cx);
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(("page", 4usize), cx);
                window.click("clear-world", cx);
            })
            .unwrap();
        workspace.update(cx, |view, window, cx| {
            view.world.title.update(cx, |input, cx| {
                input.set_value("  Astrid  ", window, cx);
            });
            view.world.body.update(cx, |input, cx| {
                input.set_value("Astrid hört das Funksignal.\n", window, cx);
            });
        });
        workspace
            .window
            .update(cx, |_, window, cx| window.click("character", cx))
            .unwrap();

        let project = Project::open(&workspace.root).unwrap();
        let character = project
            .documents()
            .find(|document| document.title() == "Astrid")
            .expect("die neue Figur muss auf der Platte liegen");
        assert_eq!(character.kind(), DocumentKind::Character);
        assert_eq!(character.body(), "Astrid hört das Funksignal.\n");
        let character = character.id();
        let event = "00000000-0000-4000-8000-000000000001"
            .parse::<Id>()
            .unwrap();

        workspace.update(cx, |view, window, cx| {
            view.world.claim.update(cx, |input, cx| {
                input.set_value("Das Signal ist eine Warnung.", window, cx);
            });
        });
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(format!("claim-event-{event}"), cx);
                window.click("Glaubt", cx);
            })
            .unwrap();

        let project = Project::open(&workspace.root).unwrap();
        let knowledge = Knowledge::from_project(&project);
        let belief = knowledge
            .entries()
            .find(|entry| entry.body.trim() == "Das Signal ist eine Warnung.")
            .expect("die Aussage muss auf der Platte liegen");
        assert_eq!(
            belief.claim,
            Some(Claim {
                holder: ClaimHolder::Character(character),
                state: ClaimState::Believes,
                from: Some(event),
            })
        );
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.render_frame(cx);
                let claim = window.find(format!("claim-{}", belief.id));
                assert_eq!(
                    claim.label().unwrap().trim(),
                    "Erstkontakt · Glaubt · Das Signal ist eine Warnung."
                );
                window.click(format!("claim-{}", belief.id), cx);
            })
            .unwrap();
        workspace.update(cx, |view, _, cx| {
            assert_eq!(
                view.world.body.read(cx).value().trim(),
                "Das Signal ist eine Warnung."
            );
        });
    }

    #[gpui_kit::test]
    fn world_source_search_opens_and_selects_the_matching_scene_passage(cx: &mut TestAppContext) {
        let workspace = test_support::open(cx);
        workspace
            .window
            .update(cx, |_, window, cx| window.click(("page", 4usize), cx))
            .unwrap();
        workspace.update(cx, |view, window, cx| {
            view.world.source_search.update(cx, |input, cx| {
                input.set_value("  TÜR ÖFFNET  ", window, cx);
            });
        });
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click("source-search", cx);
                window.render_frame(cx);
                let result = window.find_by_label("Das Signal · Eine Tür öffnet sich.");
                assert!(
                    window
                        .try_find_by_label("Der Start · Die Sonde startet.")
                        .is_none()
                );
                window.click(result.path().last().unwrap().clone(), cx);
            })
            .unwrap();

        workspace.update(cx, |view, window, cx| {
            let scene = "00000000-0000-4000-8000-000000000007"
                .parse::<Id>()
                .unwrap();
            assert!(view.page == Page::Scene);
            assert_eq!(view.selected_scene, Some(scene));
            let editor = view.editor.read(cx);
            assert_eq!(
                &editor.value()[editor.selected_range()],
                "Eine Tür öffnet sich."
            );
            assert!(editor.focus_handle(cx).is_focused(window));
        });
    }

    #[gpui_kit::test]
    fn world_source_follows_a_passage_moved_after_the_search(cx: &mut TestAppContext) {
        let workspace = test_support::open(cx);
        let path = workspace.root.join("Manuskript/Signal.md");
        let original = fs::read_to_string(&path).unwrap();
        let (metadata, body) = original.split_once("\n---\n").unwrap();
        let prefix = "Ein neuer Absatz.\n\n";
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(("page", 4usize), cx);
            })
            .unwrap();
        workspace.update(cx, |view, window, cx| {
            view.world.source_search.update(cx, |input, cx| {
                input.set_value("Tür öffnet", window, cx);
            });
        });
        workspace
            .window
            .update(cx, |_, window, cx| window.click("source-search", cx))
            .unwrap();
        fs::write(&path, format!("{metadata}\n---\n{prefix}{body}")).unwrap();

        workspace.update(cx, |view, window, cx| {
            view.world
                .source_search
                .update(cx, |input, cx| input.focus(window, cx));
        });
        workspace
            .window
            .update(cx, |_, window, cx| {
                let result = window.find_by_label("Das Signal · Eine Tür öffnet sich.");
                window.press("tab", cx);
                window.press("tab", cx);
                assert_eq!(
                    window.find(result.path().last().unwrap().clone()).focused(),
                    Some(true)
                );
                window.press("enter", cx);
            })
            .unwrap();
        workspace.update(cx, |view, window, cx| {
            assert!(view.page == Page::Scene);
            let editor = view.editor.read(cx);
            assert!(editor.value().starts_with(prefix));
            let range = editor.selected_range();
            assert_eq!(range.start, prefix.len());
            assert_eq!(&editor.value()[range], "Eine Tür öffnet sich.");
            assert!(editor.focus_handle(cx).is_focused(window));
        });
    }

    #[gpui_kit::test]
    fn ambiguous_source_lets_the_user_choose_the_exact_occurrence(cx: &mut TestAppContext) {
        let workspace = test_support::open(cx);
        let path = workspace.root.join("Manuskript/Signal.md");
        let original = fs::read_to_string(&path).unwrap();
        let (metadata, _) = original.split_once("\n---\n").unwrap();
        let quote = "Eine Tür öffnet sich.";
        let before = "Ein längerer Satz steht vor der Textstelle.\n\n";
        let after = "\n\nEin längerer Satz folgt der Textstelle.\n";
        let passage = format!("{before}{quote}{after}");
        fs::write(&path, format!("{metadata}\n---\n{passage}")).unwrap();
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(("page", 4usize), cx);
            })
            .unwrap();
        workspace.update(cx, |view, window, cx| {
            view.world.source_search.update(cx, |input, cx| {
                input.set_value("Tür öffnet", window, cx);
            });
        });
        workspace
            .window
            .update(cx, |_, window, cx| window.click("source-search", cx))
            .unwrap();
        let prefix = format!("{passage}\n{before}");
        fs::write(&path, format!("{metadata}\n---\n{passage}\n{passage}")).unwrap();

        workspace
            .window
            .update(cx, |_, window, cx| {
                let source = window.find_by_label("Das Signal · Eine Tür öffnet sich.");
                window.click(source.path().last().unwrap().clone(), cx);
                window.render_frame(cx);
                assert!(window.try_find_by_label("Zeile 3, Spalte 1").is_some());
                let second = window.find_by_label("Zeile 9, Spalte 1");
                window.click(second.path().last().unwrap().clone(), cx);
            })
            .unwrap();
        workspace.update(cx, |view, window, cx| {
            assert!(view.page == Page::Scene);
            let editor = view.editor.read(cx);
            let range = editor.selected_range();
            assert_eq!(range.start, prefix.len());
            assert_eq!(&editor.value()[range], quote);
            assert!(editor.focus_handle(cx).is_focused(window));
        });
    }
}
