use super::*;
use std::collections::BTreeMap;
use writer::{
    export::{ExportFormat, export},
    manuscript::Manuscript,
    passage::Anchor,
};

pub(super) struct ManuscriptView {
    book: Manuscript,
    editors: BTreeMap<Id, Entity<TextareaState>>,
    baselines: BTreeMap<Id, writer::Document>,
    start: Option<Anchor>,
    end: Option<Anchor>,
    replacement: Entity<TextareaState>,
}

impl ManuscriptView {
    pub(super) fn new(
        project: &Project,
        window: &mut Window,
        cx: &mut Context<ProjectView>,
    ) -> Self {
        let book = Manuscript::from_project(project);
        let ids = book
            .chapters()
            .iter()
            .flat_map(|chapter| chapter.scenes.iter().copied());
        let editors = ids
            .filter_map(|id| Some((id, book.scene(id)?)))
            .map(|(id, scene)| {
                let editor = cx.new(|cx| {
                    let mut state = TextareaState::new(window, cx).rows(8);
                    state.set_value(scene.text.text().to_owned(), window, cx);
                    state
                });
                cx.subscribe_in(
                    &editor,
                    window,
                    move |view, editor, event: &InputEvent, window, cx| {
                        if matches!(event, InputEvent::Change) {
                            let text = editor.read(cx).value();
                            view.edit_manuscript(id, &text, window, cx);
                        }
                    },
                )
                .detach();
                (id, editor)
            })
            .collect();
        let baselines = project
            .scenes()
            .map(|document| (document.id(), document.clone()))
            .collect();
        Self {
            baselines,
            book,
            editors,
            start: None,
            end: None,
            replacement: cx.new(|cx| TextareaState::new(window, cx).rows(3)),
        }
    }
}

impl ProjectView {
    fn edit_manuscript(&mut self, id: Id, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = &mut self.export_view else {
            return;
        };
        match view.book.edit_scene(id, text) {
            Ok(()) => self.stage_manuscript(window, cx),
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn stage_manuscript(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = &self.export_view else {
            return;
        };
        let Some(store) = self.store.as_mut() else {
            return;
        };
        let result = store.stage_batch(|store| {
            for id in view.book.changed_scene_ids() {
                if let Some((document, scene)) = view.baselines.get(&id).zip(view.book.scene(id)) {
                    let path = store.relative_path(document)?;
                    store.stage_body(&path, document, &scene.text.markdown())?;
                }
            }
            Ok(())
        });
        if self.report(result, cx) {
            self.schedule_flush(window, cx);
        }
    }

    pub(super) fn manuscript_saved(&mut self) {
        if let Some((view, project)) = self.export_view.as_mut().zip(self.project.as_ref()) {
            let ids: Vec<_> = view.book.changed_scene_ids().collect();
            for id in ids {
                if let Some((document, scene)) = project.scene(id).zip(view.book.scene(id))
                    && document.body() == scene.text.markdown()
                {
                    view.baselines.insert(id, document.clone());
                    view.book.acknowledge_saved(id);
                }
            }
        }
    }

    fn mark_manuscript(&mut self, id: Id, start: bool, cx: &mut Context<Self>) {
        let Some(view) = &mut self.export_view else {
            return;
        };
        let Some((scene, editor)) = view.book.scene(id).zip(view.editors.get(&id)) else {
            return;
        };
        match scene.text.anchor(editor.read(cx).selected_range()) {
            Ok(anchor) => {
                if start {
                    view.start = Some(anchor);
                } else {
                    view.end = Some(anchor);
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        cx.notify();
    }

    fn change_manuscript(&mut self, undo: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = &mut self.export_view else {
            return;
        };
        if undo {
            view.book.undo();
        } else {
            let Some((start, end)) = view.start.take().zip(view.end.take()) else {
                return;
            };
            let replacement = view.replacement.read(cx).value();
            if let Err(error) = view.book.replace(&start, &end, &replacement) {
                self.error = Some(error.to_string());
                cx.notify();
                return;
            }
        }
        for (id, editor) in &view.editors {
            if let Some(scene) = view.book.scene(*id) {
                let text = scene.text.text();
                if editor.read(cx).value().as_ref() != text {
                    editor.update(cx, |editor, cx| {
                        editor.set_value(text.to_owned(), window, cx)
                    });
                }
            }
        }
        self.stage_manuscript(window, cx);
    }

    fn choose_export(&mut self, format: ExportFormat, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.flush(cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let book = Manuscript::from_project(project);
        let name = match format {
            ExportFormat::Markdown => "Manuskript.md",
            ExportFormat::Pdf => "Manuskript.pdf",
        };
        let selection = cx.prompt_for_new_path(project.root(), Some(name));
        self.busy = true;
        cx.notify();
        cx.spawn_in(window, async move |view, cx| {
            let result = match selection.await {
                Ok(Ok(Some(path))) => cx
                    .background_executor()
                    .spawn(async move { export(&book, format, &path).map(|()| path) })
                    .await
                    .map(Some)
                    .map_err(|error| error.to_string()),
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(Some(path)) => view.last_export = Some(path),
                    Ok(None) => {}
                    Err(error) => view.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn manuscript_view(&self, cx: &mut Context<Self>) -> Div {
        let Some(view) = &self.export_view else {
            return div();
        };
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("export-md")
                            .label("Markdown exportieren")
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.choose_export(ExportFormat::Markdown, window, cx)
                            })),
                    )
                    .child(
                        Button::new("export-pdf")
                            .label("PDF exportieren")
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.choose_export(ExportFormat::Pdf, window, cx)
                            })),
                    ),
            )
            .when_some(self.last_export.as_ref(), |pane, path| {
                let file = path.clone();
                pane.child(
                    Button::new("show-export")
                        .label(format!(
                            "Datei öffnen · {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ))
                        .on_click(move |_, _, cx| cx.open_with_system(&file)),
                )
            })
            .child(
                div().flex().flex_wrap().gap_2().children(
                    view.book
                        .chapters()
                        .iter()
                        .enumerate()
                        .map(|(index, chapter)| {
                            let first = chapter.scenes.first().copied();
                            Button::new(("chapter-jump", index))
                                .label(chapter.title.clone())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if let Some(editor) = first
                                        .and_then(|id| view.export_view.as_ref()?.editors.get(&id))
                                    {
                                        editor.read(cx).focus_handle(cx).focus(window, cx);
                                    }
                                }))
                        }),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Szenenübergreifend ersetzen")
                    .child(Textarea::new(&view.replacement))
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(
                                Button::new("replace-manuscript")
                                    .label("Markierten Bereich ersetzen")
                                    .disabled(view.start.is_none() || view.end.is_none())
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.change_manuscript(false, window, cx)
                                    })),
                            )
                            .child(Button::new("undo-manuscript").label("Rückgängig").on_click(
                                cx.listener(|view, _, window, cx| {
                                    view.change_manuscript(true, window, cx)
                                }),
                            )),
                    ),
            )
            .children(view.book.chapters().iter().map(|chapter| {
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_xl().child(chapter.title.clone()))
                    .children(chapter.scenes.iter().filter_map(|id| {
                        let id = *id;
                        let (scene, editor) = view.book.scene(id).zip(view.editors.get(&id))?;
                        Some(
                            div()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap_2()
                                        .child(
                                            Button::new(format!("single-{id}"))
                                                .label(scene.title.clone())
                                                .on_click(cx.listener(
                                                    move |view, _, window, cx| {
                                                        view.select_scene(id, window, cx)
                                                    },
                                                )),
                                        )
                                        .child(
                                            Button::new(format!("replace-start-{id}"))
                                                .label("Auswahl als Beginn")
                                                .on_click(cx.listener(move |view, _, _, cx| {
                                                    view.mark_manuscript(id, true, cx)
                                                })),
                                        )
                                        .child(
                                            Button::new(format!("replace-end-{id}"))
                                                .label("Auswahl als Ende")
                                                .on_click(cx.listener(move |view, _, _, cx| {
                                                    view.mark_manuscript(id, false, cx)
                                                })),
                                        ),
                                )
                                .child(Textarea::new(editor))
                                .children(scene.text.concepts().iter().map(|concept| {
                                    div().border_l_2().border_color(rgb(0xb98a37)).pl_2().child(
                                        format!(
                                            "Konzept · {}",
                                            &scene.text.text()[concept.range.clone()]
                                        ),
                                    )
                                })),
                        )
                    }))
            }))
    }
}
