#[path = "assistance.rs"]
mod assistance;
#[path = "manuscript.rs"]
mod manuscript;
#[path = "planning.rs"]
mod planning;
#[path = "world.rs"]
mod world;
use std::{path::PathBuf, time::Duration};

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    ActiveTheme, Disableable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Textarea, TextareaState},
    scroll::ScrollableElement,
};
use gpui_kit::{prelude::FluentBuilder, *};
actions!(writer_actions, [SaveProject, UndoScene, QuitWriter]);

use writer::{
    Id, Project,
    storage::{ConflictResolution, Store},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Timeline,
    Outline,
    Scene,
    Manuscript,
    World,
    Codex,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SceneInspector {
    Preview,
    Details,
}

const SCENE_INSPECTOR_MIN_WIDTH: Pixels = px(1120.);

pub struct ProjectView {
    keyboard_focus: FocusHandle,
    _quit_subscription: Subscription,
    title: Entity<InputState>,
    destination: Entity<InputState>,
    picked_destination: Option<PathBuf>,
    creating: bool,
    busy: bool,
    error: Option<String>,
    project: Option<Project>,
    selected_scene: Option<Id>,
    show_issues: bool,
    store: Option<Store>,
    editor: Entity<TextareaState>,
    merge: Entity<TextareaState>,
    merge_path: Option<PathBuf>,
    autosave: Option<Task<()>>,
    last_version: Option<String>,
    last_export: Option<PathBuf>,
    scene_inspector: Option<SceneInspector>,
    scene_text: Option<writer::passage::SceneText>,
    scene_baseline: Option<writer::Document>,
    page: Page,
    event: Option<Id>,
    chapter: Option<Id>,
    event_target: Option<Id>,
    event_list: bool,
    event_title: Entity<InputState>,
    event_time: Entity<InputState>,
    chapter_title: Entity<InputState>,
    scene_time: Entity<InputState>,
    relation_note: Entity<InputState>,
    relation_time: Entity<InputState>,
    form_body: Entity<TextareaState>,
    export_view: Option<manuscript::ManuscriptView>,
    world: world::WorldView,
    ai: assistance::Assistance,
}

impl ProjectView {
    pub(crate) fn install_quit_action(cx: &mut App) {
        cx.on_action(|_: &QuitWriter, cx| {
            // Tastaturaktionen laufen in einem Fenster-Update. Erst danach sind alle Fenster zugänglich.
            cx.defer(|cx| {
                let saved = cx
                    .windows()
                    .into_iter()
                    .filter_map(|handle| handle.downcast::<base::Root>())
                    .all(|handle| {
                        handle
                            .update(cx, |root, window, cx| {
                                let Ok(view) = root.view().clone().downcast::<Self>() else {
                                    return true;
                                };
                                let saved = view.update(cx, |view, cx| view.flush(cx));
                                if !saved {
                                    window.activate_window();
                                }
                                saved
                            })
                            .unwrap_or(false)
                    });
                if saved {
                    cx.quit();
                }
            });
        });
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| TextareaState::new(window, cx).rows(16));
        cx.subscribe_in(
            &editor,
            window,
            |view, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Change) {
                    view.stage_editor(window, cx);
                }
            },
        )
        .detach();
        let merge = cx.new(|cx| TextareaState::new(window, cx).rows(8));
        cx.observe(&merge, |_, _, cx| cx.notify()).detach();
        let keyboard_focus = cx.focus_handle().tab_stop(false);
        window.focus(&keyboard_focus, cx);
        let view = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            view.update(cx, |view, cx| view.flush(cx)).unwrap_or(true)
        });
        let quit_subscription = cx.on_app_quit(|view, cx| {
            if !view.flush(cx)
                && let Some(error) = &view.error
            {
                eprintln!("Ausstehende Buchänderungen konnten nicht gesichert werden. {error}");
            }
            std::future::ready(())
        });
        Self {
            keyboard_focus,
            _quit_subscription: quit_subscription,
            store: None,
            editor,
            merge,
            merge_path: None,
            autosave: None,
            last_version: None,
            last_export: None,
            scene_inspector: None,
            scene_text: None,
            scene_baseline: None,
            page: Page::Timeline,
            event: None,
            chapter: None,
            event_target: None,
            event_list: false,
            event_title: cx.new(|cx| InputState::new(window, cx)),
            event_time: cx.new(|cx| InputState::new(window, cx)),
            chapter_title: cx.new(|cx| InputState::new(window, cx)),
            scene_time: cx.new(|cx| InputState::new(window, cx)),
            relation_note: cx.new(|cx| InputState::new(window, cx)),
            relation_time: cx.new(|cx| InputState::new(window, cx)),
            form_body: cx.new(|cx| TextareaState::new(window, cx).rows(5)),
            export_view: None,
            world: world::WorldView::new(window, cx),
            ai: assistance::Assistance::new(window, cx),

            title: cx.new(|cx| InputState::new(window, cx)),
            destination: cx.new(|cx| InputState::new(window, cx)),
            picked_destination: None,
            creating: false,
            busy: false,
            error: None,
            project: None,
            selected_scene: None,
            show_issues: false,
        }
    }

    fn choose_directory(
        &mut self,
        for_creation: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Ordner wählen".into()),
        });
        cx.spawn_in(window, async move |view, cx| {
            let selection = selection
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match selection {
                    Ok(Some(paths)) => {
                        if let Some(path) = paths.into_iter().next() {
                            if for_creation {
                                view.destination.update(cx, |input, cx| {
                                    input.set_value(
                                        path.to_string_lossy().into_owned(),
                                        window,
                                        cx,
                                    );
                                });
                                view.picked_destination = Some(path);
                            } else {
                                view.load(path, None, window, cx);
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(error) => view.error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = self.title.read(cx).value();
        let path = self.destination.read(cx).value();
        if path.trim().is_empty() {
            self.error = Some("Gib den Pfad zum Zielordner ein.".into());
            cx.notify();
            return;
        }
        let root = self
            .picked_destination
            .as_ref()
            .filter(|picked| picked.to_string_lossy() == path.as_ref())
            .cloned()
            .unwrap_or_else(|| PathBuf::from(path.as_ref()));
        self.load(root, Some(title.to_string()), window, cx);
    }

    fn load(
        &mut self,
        root: PathBuf,
        title: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let load = cx.background_executor().spawn(async move {
            let project = match title {
                Some(title) => Project::create(&root, &title),
                None => Project::open(&root),
            }?;
            let store = Store::new(project.root())?;
            Ok::<_, writer::ProjectError>((project, store))
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = load.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok((project, store)) => {
                        view.store = Some(store);
                        view.last_version = None;
                        view.project = Some(project);
                        view.reset_workspace(window, cx);
                        view.restore_ai(window, cx);
                        view.selected_scene = None;
                        view.show_issues = false;
                        view.creating = false;
                    }
                    Err(error) => view.error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn welcome(&self, cx: &mut Context<Self>) -> Div {
        div()
            .max_w(px(640.))
            .w_full()
            .mx_auto()
            .flex()
            .flex_col()
            .gap_4()
            .child(div().text_2xl().child("Buchprojekt öffnen"))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new("open")
                            .primary()
                            .label("Ordner öffnen …")
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| {
                                view.choose_directory(false, window, cx)
                            })),
                    )
                    .child(
                        Button::new("new")
                            .label("Neues Buchprojekt …")
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.creating = true;
                                view.error = None;
                                cx.notify();
                            })),
                    ),
            )
            .when(self.creating, |content| {
                content.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("Buchtitel")
                        .child(Input::new(&self.title))
                        .child("Zielordner")
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(Input::new(&self.destination)),
                                )
                                .child(
                                    Button::new("choose")
                                        .label("Ordner wählen …")
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.choose_directory(true, window, cx)
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    Button::new("create")
                                        .primary()
                                        .label("Buch anlegen")
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.create(window, cx)
                                        })),
                                )
                                .child(
                                    Button::new("cancel")
                                        .label("Abbrechen")
                                        .disabled(self.busy)
                                        .on_click(cx.listener(|view, _, _, cx| {
                                            view.creating = false;
                                            view.error = None;
                                            cx.notify();
                                        })),
                                ),
                        ),
                )
            })
    }

    fn stage_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((store, document)) = self.store.as_mut().zip(self.scene_baseline.as_ref()) else {
            return;
        };
        let editor_body = self.editor.read(cx).value();
        let Some(text) = &mut self.scene_text else {
            return;
        };
        if let Err(error) = text.edit(&editor_body) {
            self.error = Some(error.to_string());
            cx.notify();
            return;
        }
        let body = text.markdown();
        let staged = store
            .relative_path(document)
            .and_then(|path| store.stage_body(&path, document, &body));
        if let Err(error) = staged {
            self.error = Some(error.to_string());
            cx.notify();
            return;
        }
        self.schedule_flush(window, cx);
    }

    fn schedule_flush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.autosave = Some(cx.spawn_in(window, async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = view.update(cx, |view, cx| {
                view.flush(cx);
            });
        }));
        cx.notify();
    }

    fn refresh_project(&mut self) -> writer::Result<()> {
        if let Some(project) = &self.project {
            self.project = Some(Project::open(project.root())?);
        }
        Ok(())
    }

    fn flush(&mut self, cx: &mut Context<Self>) -> bool {
        self.autosave = None;
        let Some(store) = self.store.as_mut().filter(|store| store.is_pending()) else {
            return true;
        };
        let result = store.flush().and_then(|()| self.refresh_project());
        if result.is_ok() {
            if let Some((document, text)) = self
                .selected_scene
                .and_then(|id| self.project.as_ref()?.scene(id))
                .zip(self.scene_text.as_ref())
                && document.body() == text.markdown()
            {
                self.scene_baseline = Some(document.clone());
            }
            self.manuscript_saved();
        }
        self.report(result, cx)
    }

    fn report(&mut self, result: writer::Result<()>, cx: &mut Context<Self>) -> bool {
        self.error = result.as_ref().err().map(ToString::to_string);
        cx.notify();
        result.is_ok()
    }

    fn select_scene(&mut self, id: Id, window: &mut Window, cx: &mut Context<Self>) {
        if !self.flush(cx) {
            return;
        }
        if self.page == Page::Scene && self.selected_scene != Some(id) {
            self.scene_left(window, cx);
        }
        let Some(scene) = self.project.as_ref().and_then(|project| project.scene(id)) else {
            return;
        };
        let text = writer::passage::SceneText::from_markdown(id, scene.body());
        self.editor.update(cx, |editor, cx| {
            editor.set_value(text.text().to_owned(), window, cx)
        });
        self.scene_text = Some(text);
        self.scene_inspector = None;
        self.scene_baseline = Some(scene.clone());
        self.page = Page::Scene;
        self.selected_scene = Some(id);
        self.load_scene_time(id, window, cx);
        self.scene_opened(window, cx);
        cx.notify();
    }

    fn checkpoint(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.flush(cx) {
            return;
        }
        let Some(project) = &self.project else { return };
        let root = project.root().to_owned();
        self.busy = true;
        let commit = cx
            .background_executor()
            .spawn(async move { Store::new(&root)?.checkpoint("Buchprojekt speichern") });
        cx.spawn_in(window, async move |view, cx| {
            let result = commit.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(checkpoint) => {
                        view.last_version = Some(checkpoint.committed_at);
                        view.error = None;
                    }
                    Err(error) => {
                        view.error = Some(format!(
                            "Lokale Version konnte nicht erstellt werden. {error}"
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn resolve_conflict(
        &mut self,
        resolution: ConflictResolution,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(store) = &mut self.store else { return };
        let result = store
            .resolve(resolution)
            .and_then(|()| self.refresh_project());
        if self.report(result, cx) {
            if self.page == Page::Manuscript
                && let Some(project) = &self.project
            {
                self.export_view = Some(manuscript::ManuscriptView::new(project, window, cx));
            } else if self.page == Page::Scene
                && let Some(id) = self.selected_scene.take()
            {
                self.select_scene(id, window, cx);
            }
            self.merge_path = None;
        }
    }

    fn prepare_merge(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(conflict) = self.store.as_ref().and_then(Store::conflict) else {
            self.merge_path = None;
            return;
        };
        if self.merge_path.as_ref() != Some(&conflict.path) {
            self.merge_path = Some(conflict.path.clone());
            self.merge.update(cx, |input, cx| {
                input.set_value(
                    String::from_utf8_lossy(&conflict.in_app).into_owned(),
                    window,
                    cx,
                );
            });
        }
    }

    fn save_controls(&self, cx: &mut Context<Self>) -> Div {
        let pending = self.store.as_ref().is_some_and(Store::is_pending);
        let conflict = self.store.as_ref().and_then(Store::conflict);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(if pending {
                        "Dateiänderungen ausstehend"
                    } else {
                        "Auf Festplatte gesichert"
                    })
                    .when_some(self.last_version.clone(), |row, time| {
                        row.child(format!("Letzte Version {time}"))
                    })
                    .when(pending && conflict.is_none(), |row| {
                        row.child(
                            Button::new("retry")
                                .label("Sicherung erneut versuchen")
                                .on_click(cx.listener(|view, _, _, cx| {
                                    view.flush(cx);
                                })),
                        )
                    }),
            )
            .when_some(conflict, |pane, conflict| {
                let merge = self.merge.read(cx).value();
                let validation = conflict.validate_merge(merge.as_bytes());
                pane.child(format!("Dateikonflikt · {}", conflict.path.display()))
                    .child("In der App")
                    .child(String::from_utf8_lossy(&conflict.in_app).into_owned())
                    .child("Auf Festplatte")
                    .child(conflict.on_disk.as_ref().map_or_else(
                        || "Datei fehlt".into(),
                        |bytes| String::from_utf8_lossy(bytes).into_owned(),
                    ))
                    .child("Zusammengeführte Datei")
                    .child(Textarea::new(&self.merge).aria_label("Zusammengeführte Datei"))
                    .when_some(validation.as_ref().err(), |pane, error| {
                        pane.child(error.to_string())
                    })
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(
                                Button::new("keep-app")
                                    .label("In der App übernehmen")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.resolve_conflict(ConflictResolution::InApp, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("keep-disk")
                                    .label("Auf Festplatte übernehmen")
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.resolve_conflict(
                                            ConflictResolution::OnDisk,
                                            window,
                                            cx,
                                        )
                                    })),
                            )
                            .child(
                                Button::new("merge-apply")
                                    .label("Zusammenführung übernehmen")
                                    .disabled(validation.is_err())
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        let bytes = view.merge.read(cx).value().as_bytes().to_vec();
                                        view.resolve_conflict(
                                            ConflictResolution::Merged(bytes),
                                            window,
                                            cx,
                                        );
                                    })),
                            ),
                    )
            })
    }

    fn reset_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.page = Page::Timeline;
        self.event = None;
        self.chapter = None;
        self.event_target = None;
        self.export_view = None;
        self.last_export = None;
        self.scene_text = None;
        self.scene_baseline = None;
        self.scene_inspector = None;
        self.event_title
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.form_body
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.event_time
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.chapter_title
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.scene_time
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.world.reset(window, cx);
        self.ai.reset(window, cx);
    }

    fn navigate(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        if self.page == page || !self.flush(cx) {
            return;
        }
        if self.page == Page::Scene {
            self.scene_left(window, cx);
        }
        self.page = page;
        if page == Page::Manuscript
            && let Some(project) = &self.project
        {
            self.export_view = Some(manuscript::ManuscriptView::new(project, window, cx));
        }
        cx.notify();
    }

    fn scene_action(
        &mut self,
        action: Option<writer::passage::TextFormat>,
        concept: Option<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(text) = &mut self.scene_text else {
            return;
        };
        let selection = self.editor.read(cx).selected_range();
        let result = if action.is_none() && concept.is_none() {
            text.undo();
            Ok(())
        } else {
            text.anchor(selection).and_then(|anchor| {
                if let Some(format) = action {
                    text.format(&anchor, format)
                } else {
                    text.set_concept(&anchor, concept.unwrap_or_default())
                }
            })
        };
        match result {
            Ok(()) => {
                let value = text.text().to_owned();
                self.editor
                    .update(cx, |editor, cx| editor.replace_all(value, window, cx));
                self.stage_editor(window, cx);
            }
            Err(error) => {
                self.error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn toggle_scene_inspector(
        &mut self,
        inspector: SceneInspector,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.scene_inspector == Some(inspector) {
            self.scene_inspector = None;
            if window.viewport_size().width < SCENE_INSPECTOR_MIN_WIDTH {
                self.editor
                    .update(cx, |editor, cx| editor.focus(window, cx));
            }
        } else {
            self.scene_inspector = Some(inspector);
        }
        cx.notify();
    }

    fn scene_editor(&self, scene: Id, cx: &mut Context<Self>) -> Div {
        use writer::passage::TextFormat;
        let actions = [
            ("fett", "Fett", Some(TextFormat::Bold), None),
            ("kursiv", "Kursiv", Some(TextFormat::Italic), None),
            ("heading", "Überschrift", Some(TextFormat::Heading), None),
            ("liste", "Liste", Some(TextFormat::List), None),
            ("concept", "Als Konzept markieren", None, Some(true)),
            (
                "unconcept",
                "Konzeptkennzeichnung entfernen",
                None,
                Some(false),
            ),
            ("undo", "Letzte Änderung rückgängig", None, None),
        ];
        let layout = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(actions.into_iter().map(|(id, title, format, concept)| {
                        Button::new(id).label(title).on_click(cx.listener(
                            move |view, _, window, cx| {
                                view.scene_action(format, concept, window, cx)
                            },
                        ))
                    })),
            )
            .child(
                div()
                    .key_context("WriterScene")
                    .on_action(cx.listener(|view, _: &UndoScene, window, cx| {
                        view.scene_action(None, None, window, cx)
                    }))
                    .child(Textarea::new(&self.editor).aria_label("Szenentext")),
            )
            .child(self.scene_assistance(scene, cx));
        let Some(text) = &self.scene_text else {
            return layout;
        };
        layout.when(!text.concepts().is_empty(), |layout| {
            layout.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .children(text.concepts().iter().map(|concept| {
                        div()
                            .border_l_2()
                            .border_color(rgb(0xb98a37))
                            .pl_2()
                            .child(format!("Konzept · {}", &text.text()[concept.range.clone()]))
                    })),
            )
        })
    }

    fn scene_view(&self, project: &Project, window: &Window, cx: &mut Context<Self>) -> Div {
        use SceneInspector::{Details, Preview};
        let Some(scene) = self.selected_scene.and_then(|id| project.scene(id)) else {
            return div().child("Wähle eine Szene in der Kapitelansicht.");
        };
        let split = window.viewport_size().width >= SCENE_INSPECTOR_MIN_WIDTH;
        let current = self.scene_inspector;
        let inspector_actions = [
            (
                "preview",
                "Markdown-Vorschau",
                "Vorschau schließen",
                Preview,
            ),
            (
                "scene-details",
                "Szenendetails",
                "Szenendetails schließen",
                Details,
            ),
        ];
        let toolbar = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(div().flex_1().text_lg().child(scene.title().to_owned()))
            .children(
                inspector_actions
                    .into_iter()
                    .map(|(id, open, close, inspector)| {
                        Button::new(id)
                            .label(if current == Some(inspector) {
                                close
                            } else {
                                open
                            })
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.toggle_scene_inspector(inspector, window, cx);
                            }))
                    }),
            );
        let panes = div()
            .flex()
            .gap_4()
            .items_start()
            .when(split || current.is_none(), |panes| {
                panes.child(self.scene_editor(scene.id(), cx))
            })
            .when_some(current, |panes, inspector| {
                let content = match inspector {
                    Details => self
                        .scene_timing(project, scene.id(), cx)
                        .into_any_element(),
                    Preview => div()
                        .id("scene-preview")
                        .children(self.scene_text.as_ref().map(|text| {
                            gpui_kit::component::text::TextView::markdown(
                                "markdown-preview",
                                writer::export::preview_markdown(text),
                            )
                        }))
                        .test_support()
                        .into_any_element(),
                };
                panes.child(
                    div()
                        .min_w_0()
                        .flex_shrink_0()
                        .when(split, |panel| {
                            panel
                                .w(px(340.))
                                .border_l_1()
                                .border_color(cx.theme().border)
                                .pl_4()
                        })
                        .when(!split, |panel| panel.w_full())
                        .child(content),
                )
            });
        div().flex().flex_col().gap_3().child(toolbar).child(panes)
    }

    fn workspace(&self, project: &Project, window: &Window, cx: &mut Context<Self>) -> Div {
        let pages = [
            (Page::Timeline, "Timeline"),
            (Page::Outline, "Kapitel"),
            (Page::Scene, "Szene"),
            (Page::Manuscript, "Manuskript"),
            (Page::World, "Welt"),
            (Page::Codex, "Codex"),
        ];
        let narrow = window.viewport_size().width < px(790.);
        let rail = if cx.theme().is_dark() {
            rgb(0x302c29)
        } else {
            rgb(0xf0eae0)
        };
        let selection = if cx.theme().is_dark() {
            rgb(0x77aea3)
        } else {
            rgb(0x235a54)
        };
        let navigation = div()
            .flex()
            .flex_shrink_0()
            .gap_2()
            .bg(rail)
            .p_3()
            .when(narrow, |rail| rail.flex_wrap())
            .when(!narrow, |rail| rail.flex_col().w(px(184.)))
            .children(pages.into_iter().enumerate().map(|(index, (page, label))| {
                Button::new(("page", index))
                    .label(label)
                    .when(self.page == page, |button| button.primary())
                    .on_click(
                        cx.listener(move |view, _, window, cx| view.navigate(page, window, cx)),
                    )
            }));
        let content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_4()
            .p_4()
            .when(!project.issues().is_empty(), |content| {
                content
                    .child(
                        Button::new("issues")
                            .label(if self.show_issues {
                                "Projekthinweise ausblenden"
                            } else {
                                "Projekthinweise anzeigen"
                            })
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.show_issues = !view.show_issues;
                                cx.notify();
                            })),
                    )
                    .when(self.show_issues, |content| {
                        content.child(
                            div().flex().flex_col().gap_1().children(
                                project
                                    .issues()
                                    .iter()
                                    .map(|issue| div().child(issue.to_string())),
                            ),
                        )
                    })
            })
            .child(self.save_controls(cx))
            .child(match self.page {
                Page::Timeline => self.timeline_view(project, cx),
                Page::Outline => self.outline_view(project, cx),
                Page::Scene => self.scene_view(project, window, cx),
                Page::Manuscript => self.manuscript_view(cx),
                Page::World => self.world_view(project, cx),
                Page::Codex => self.assistance_view(project, cx),
            })
            .overflow_y_scrollbar()
            .id(("workspace-page", self.page as usize));
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    .pb_3()
                    .border_b_1()
                    .border_color(selection)
                    .child(div().flex_1().text_2xl().child(project.title().to_owned()))
                    .child(
                        Button::new("checkpoint")
                            .label("Speichern")
                            .primary()
                            .tooltip("Lokale Git-Version erstellen")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|view, _, window, cx| view.checkpoint(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("choose-project")
                            .label("Zur Projektwahl")
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| {
                                if view.flush(cx) {
                                    view.scene_left(window, cx);
                                    view.reset_workspace(window, cx);
                                    view.project = None;
                                    view.store = None;
                                    view.error = None;
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .gap_2()
                    .when(narrow, |layout| layout.flex_col())
                    .child(navigation)
                    .child(content),
            )
    }
}

impl Render for ProjectView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_merge(window, cx);
        let dark = cx.theme().is_dark();
        div()
            .id("workspace")
            .track_focus(&self.keyboard_focus)
            .on_action(cx.listener(|view, _: &SaveProject, window, cx| view.checkpoint(window, cx)))
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(if dark { rgb(0x252220) } else { rgb(0xfbf8f2) })
            .text_color(if dark { rgb(0xf0eae0) } else { rgb(0x27231e) })
            .p_5()
            .child(match &self.project {
                Some(project) => self.workspace(project, window, cx).into_any_element(),
                None => div()
                    .flex_1()
                    .min_h_0()
                    .child(self.welcome(cx))
                    .overflow_y_scrollbar()
                    .into_any_element(),
            })
            .when(self.busy, |content| {
                content.child(
                    div()
                        .flex_shrink_0()
                        .mt_2()
                        .child("Projekt wird vorbereitet …"),
                )
            })
            .when_some(self.error.clone(), |content, error| {
                content.child(div().flex_shrink_0().mt_2().child(error))
            })
    }
}

#[cfg(test)]
#[path = "test_support.rs"]
pub(super) mod test_support;
