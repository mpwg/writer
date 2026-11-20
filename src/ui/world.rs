use super::*;
use writer::{Document, DocumentKind};

const KINDS: [(DocumentKind, &str, &str); 4] = [
    (DocumentKind::Character, "Figur", "character"),
    (DocumentKind::Place, "Ort", "place"),
    (DocumentKind::Concept, "Konzept", "concept"),
    (DocumentKind::Idea, "Idee", "idea"),
];

pub(super) struct WorldView {
    selected: Option<Id>,
    first_event: Option<Id>,
    title: Entity<InputState>,
    body: Entity<TextareaState>,
    claim: Entity<InputState>,
    source_search: Entity<InputState>,
    source_query: String,
}

impl WorldView {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ProjectView>) -> Self {
        Self {
            selected: None,
            first_event: None,
            title: cx.new(|cx| InputState::new(window, cx)),
            body: cx.new(|cx| TextareaState::new(window, cx).rows(9)),
            claim: cx.new(|cx| InputState::new(window, cx)),
            source_search: cx.new(|cx| InputState::new(window, cx)),
            source_query: String::new(),
        }
    }

    pub(super) fn reset(&mut self, window: &mut Window, cx: &mut Context<ProjectView>) {
        self.selected = None;
        self.first_event = None;
        self.source_query.clear();
        self.title
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.body
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.claim
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.source_search
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    fn enter(&mut self, document: &Document, window: &mut Window, cx: &mut Context<ProjectView>) {
        self.selected = Some(document.id());
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

    pub(super) fn jump_to(
        &mut self,
        anchor: &writer::passage::Anchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use writer::sources::{Resolution, Sources};
        if !self.flush(cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        if let Resolution::Unique { document, range } = Sources::build(project).resolve(anchor) {
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
                Some(kind) if KINDS.iter().any(|(candidate, _, _)| *candidate == kind) => {
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
        } else {
            self.error = Some("Die Quelle wurde geändert oder kommt mehrfach vor.".into());
            cx.notify();
        }
    }

    pub(super) fn world_view(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let selected = self.world.selected.and_then(|id| project.document(id));
        let mut view = div().flex().flex_col().gap_3();
        for (kind, label, _) in KINDS {
            view = view.child(div().text_lg().child(label)).child(
                div().flex().flex_wrap().gap_2().children(
                    project
                        .documents()
                        .filter(move |document| document.kind() == kind)
                        .map(|document| {
                            let id = document.id();
                            Button::new(format!("world-{id}"))
                                .label(document.title().to_owned())
                                .when(self.world.selected == Some(id), |button| button.primary())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    view.open_world(id, window, cx)
                                }))
                        }),
                ),
            );
        }
        view =
            view.child("Titel")
                .child(Input::new(&self.world.title))
                .child("Notiz")
                .child(Textarea::new(&self.world.body))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(Button::new("clear-world").label("Neuer Eintrag").on_click(
                            cx.listener(|view, _, window, cx| {
                                view.world.reset(window, cx);
                                cx.notify();
                            }),
                        ))
                        .when_some(selected, |row, document| {
                            let id = document.id();
                            let kind = KINDS
                                .iter()
                                .find(|(kind, _, _)| *kind == document.kind())
                                .map(|(_, _, wire)| *wire);
                            row.when_some(kind, |row, kind| {
                                row.child(
                                    Button::new("update-world")
                                        .primary()
                                        .label("Änderung sichern")
                                        .on_click(cx.listener(move |view, _, window, cx| {
                                            view.save_world(kind, Some(id), window, cx)
                                        })),
                                )
                            })
                        })
                        .when(selected.is_none(), |row| {
                            row.children(KINDS.into_iter().map(|(_, label, kind)| {
                                Button::new(kind)
                                    .label(format!("{label} anlegen"))
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        view.save_world(kind, None, window, cx)
                                    }))
                            }))
                        }),
                )
                .child("Quellen finden")
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Input::new(&self.world.source_search))
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
            view = view.children(
                sources
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
                    .map(|(index, source)| {
                        self.source_button(index, &source.title, &source.anchor, cx)
                    }),
            );
        }
        if let Some(character) =
            selected.filter(|document| document.kind() == DocumentKind::Character)
        {
            view = view.child(self.knowledge_view(project, character.id(), cx));
        }
        view
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
            .child(div().text_lg().child("Wissen und Glauben"))
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
            .child(Input::new(&self.world.claim))
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
