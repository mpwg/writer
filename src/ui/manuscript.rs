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
    export_formats: bool,
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
                    apply_concept_decorations(&mut state, &scene.text, cx);
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
            export_formats: false,
        }
    }
}

impl ProjectView {
    fn edit_manuscript(&mut self, id: Id, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = &mut self.export_view else {
            return;
        };
        match view.book.edit_scene(id, text) {
            Ok(()) => {
                if let Some((editor, scene)) = view.editors.get(&id).zip(view.book.scene(id)) {
                    editor.update(cx, |editor, cx| {
                        apply_concept_decorations(editor, &scene.text, cx)
                    });
                }
                self.stage_manuscript(window, cx);
            }
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
                        editor.set_value(text.to_owned(), window, cx);
                        apply_concept_decorations(editor, &scene.text, cx);
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

    pub(super) fn show_export_formats(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Manuscript, window, cx);
        if self.page == Page::Manuscript
            && let Some(view) = &mut self.export_view
        {
            view.export_formats = true;
            cx.notify();
        }
    }

    pub(super) fn manuscript_view(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let Some(view) = &self.export_view else {
            return div();
        };
        let narrow = window.viewport_size().width < px(1000.);
        let toolbar = div()
            .flex()
            .flex_wrap()
            .justify_between()
            .items_center()
            .gap_2()
            .pb_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_lg()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Gesamtmanuskript"),
            )
            .child(
                Button::new("manuscript-export")
                    .small()
                    .primary()
                    .label("Export …")
                    .selected(view.export_formats)
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, _, _, cx| {
                        if let Some(view) = &mut view.export_view {
                            view.export_formats = !view.export_formats;
                            cx.notify();
                        }
                    })),
            );
        let formats = div()
            .flex()
            .items_center()
            .flex_wrap()
            .gap_2()
            .child("Exportformat")
            .children(
                [
                    ("export-pdf", "Formatiertes PDF", ExportFormat::Pdf),
                    ("export-md", "Markdown", ExportFormat::Markdown),
                ]
                .into_iter()
                .map(|(id, label, format)| {
                    Button::new(id)
                        .small()
                        .label(label)
                        .disabled(self.busy)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.choose_export(format, window, cx);
                        }))
                }),
            );
        let index = div()
            .flex()
            .gap_1()
            .p_3()
            .bg(cx.theme().sidebar)
            .when(narrow, |index| index.flex_wrap().items_center())
            .when(!narrow, |index| {
                index.flex_shrink_0().flex_col().w(px(192.))
            })
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Inhaltsverzeichnis"),
            )
            .children(
                view.book
                    .chapters()
                    .iter()
                    .enumerate()
                    .map(|(index, chapter)| {
                        let first = chapter.scenes.first().copied();
                        Button::new(("chapter-jump", index))
                            .ghost()
                            .small()
                            .label(chapter.title.clone())
                            .when(!narrow, |item| item.w_full().justify_start())
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if let Some(editor) =
                                    first.and_then(|id| view.export_view.as_ref()?.editors.get(&id))
                                {
                                    editor.read(cx).focus_handle(cx).focus(window, cx);
                                }
                            }))
                    }),
            );
        let pages = div()
            .flex()
            .flex_col()
            .gap_6()
            .w_full()
            .max_w(px(760.))
            .mx_auto()
            .p_5()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child("Szenenübergreifend ersetzen")
                    .child(
                        Textarea::new(&view.replacement)
                            .aria_label("Szenenübergreifender Ersatztext"),
                    )
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
                    .child(
                        div()
                            .text_xl()
                            .font_family(BOOK_SERIF)
                            .child(chapter.title.clone()),
                    )
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
                                .child(
                                    Textarea::new(editor)
                                        .appearance(false)
                                        .font_family(BOOK_SERIF)
                                        .aria_label(scene.title.clone()),
                                )
                                .children(scene.text.concepts().iter().map(|concept| {
                                    div()
                                        .border_l_2()
                                        .border_color(cx.theme().warning)
                                        .pl_2()
                                        .child(format!(
                                            "Konzept · {}",
                                            &scene.text.text()[concept.range.clone()]
                                        ))
                                })),
                        )
                    }))
            }));
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(toolbar)
            .when(view.export_formats, |pane| pane.child(formats))
            .when_some(self.last_export.as_ref(), |pane, path| {
                let file = path.clone();
                pane.child(
                    Button::new("show-export")
                        .ghost()
                        .label(format!(
                            "Datei öffnen · {}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ))
                        .on_click(move |_, _, cx| cx.open_with_system(&file)),
                )
            })
            .child(
                div()
                    .flex()
                    .min_w_0()
                    .bg(cx.theme().muted)
                    .when(narrow, |layout| layout.flex_col())
                    .child(index)
                    .child(div().flex_1().min_w_0().p_4().child(pages)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support;
    use gpui_kit::{Focusable, TestAppContext, test::TestWindowExt};
    use std::time::Duration;
    use writer::{Id, Project, outline::Outline};

    const START: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000006);
    const SIGNAL: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    const ERSTKONTAKT: Id = Id::from_u128(0x00000000_0000_4000_8000_000000000001);

    #[gpui_kit::test]
    fn manuscript_places_contents_next_to_the_page_and_discloses_export_formats(
        cx: &mut TestAppContext,
    ) {
        let workspace = test_support::open(cx);
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(("page", 3usize), cx);
                let editor = workspace
                    .view
                    .read(cx)
                    .export_view
                    .as_ref()
                    .unwrap()
                    .editors[&START]
                    .entity_id();
                assert!(
                    window.find(("chapter-jump", 0_usize)).bounds().right()
                        <= window.find(("input", editor)).bounds().left()
                );
                assert!(window.try_find("export-pdf").is_none());
                window.click("manuscript-export", cx);
                assert!(window.find("export-pdf").visible());
                assert!(window.find("export-md").visible());
            })
            .unwrap();
    }

    #[gpui_kit::test]
    fn manuscript_replacement_and_undo_update_both_scene_files(cx: &mut TestAppContext) {
        let workspace = test_support::open(cx);
        let original = Project::open(&workspace.root).unwrap();
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(("page", 3usize), cx);
                window.render_frame(cx);
                assert_eq!(window.find("replace-manuscript").disabled(), Some(true));
                window.click(("chapter-jump", 1usize), cx);
            })
            .unwrap();
        workspace.update(cx, |view, window, cx| {
            let view = view.export_view.as_ref().unwrap();
            assert!(
                view.editors[&SIGNAL]
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window)
            );
            view.editors[&START].update(cx, |editor, cx| editor.set_selected_range(4..9, cx));
            view.editors[&SIGNAL].update(cx, |editor, cx| {
                editor.set_selected_range(0.."Eine Tür öffnet sich.".len(), cx);
            });
            view.replacement.update(cx, |editor, cx| {
                editor.set_value("Fähre", window, cx);
            });
        });
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(format!("replace-start-{START}"), cx);
                window.click(format!("replace-end-{SIGNAL}"), cx);
                window.click("replace-manuscript", cx);
            })
            .unwrap();
        cx.executor().advance_clock(Duration::from_millis(400));
        cx.run_until_parked();

        let edited = Project::open(&workspace.root).unwrap();
        assert_eq!(edited.scene(START).unwrap().body().trim(), "Die Fähre");
        assert_eq!(
            edited.scene(SIGNAL).unwrap().body(),
            original
                .scene(SIGNAL)
                .unwrap()
                .body()
                .replace("Eine Tür öffnet sich.", "")
        );
        assert_eq!(
            Outline::from_project(&edited).scene(SIGNAL).unwrap().event,
            Some(ERSTKONTAKT)
        );
        workspace.update(cx, |view, _, cx| {
            let editors = &view.export_view.as_ref().unwrap().editors;
            assert_eq!(editors[&START].read(cx).value().as_ref(), "Die Fähre");
            assert_eq!(
                editors[&SIGNAL].read(cx).value().trim(),
                "Danach verstummt die Antenne."
            );
        });

        workspace
            .window
            .update(cx, |_, window, cx| window.click("undo-manuscript", cx))
            .unwrap();
        cx.executor().advance_clock(Duration::from_millis(400));
        cx.run_until_parked();
        let restored = Project::open(&workspace.root).unwrap();
        for id in [START, SIGNAL] {
            assert_eq!(
                restored.scene(id).unwrap().body(),
                original.scene(id).unwrap().body()
            );
        }
    }

    #[gpui_kit::test]
    fn manuscript_exports_include_a_pending_editor_change(cx: &mut TestAppContext) {
        let workspace = test_support::open(cx);
        workspace
            .window
            .update(cx, |_, window, cx| {
                window.click(("page", 3usize), cx);
                window.click(("chapter-jump", 0usize), cx);
            })
            .unwrap();
        workspace.update(cx, |view, _, cx| {
            let editor = &view.export_view.as_ref().unwrap().editors[&START];
            editor.update(cx, |editor, cx| editor.set_selected_range(4..9, cx));
        });
        workspace
            .window
            .update(cx, |_, window, cx| window.input("Fähre", cx))
            .unwrap();
        assert_eq!(
            Project::open(&workspace.root)
                .unwrap()
                .scene(START)
                .unwrap()
                .body(),
            "Die Sonde startet.\n"
        );

        workspace
            .window
            .update(cx, |_, window, cx| window.click("manuscript-export", cx))
            .unwrap();
        for (button, name) in [("export-md", "Buch.md"), ("export-pdf", "Buch.pdf")] {
            workspace
                .window
                .update(cx, |_, window, cx| window.click(button, cx))
                .unwrap();
            assert!(cx.did_prompt_for_new_path());
            let output = workspace.root.join(name);
            cx.simulate_new_path_selection(|directory| {
                assert_eq!(directory, workspace.root);
                Some(output.clone())
            });
            cx.run_until_parked();
            workspace
                .window
                .update(cx, |_, window, cx| {
                    window.render_frame(cx);
                    let expected = format!("Datei öffnen · {name}");
                    assert_eq!(window.find("show-export").label(), Some(expected.as_str()));
                })
                .unwrap();
            let content = if name.ends_with(".md") {
                std::fs::read_to_string(output).unwrap()
            } else {
                pdf_extract::extract_text(output).unwrap()
            };
            assert!(content.contains("Die Fähre startet."), "{content}");
            assert!(
                content.contains("Danach verstummt die Antenne."),
                "{content}"
            );
        }
    }
}
