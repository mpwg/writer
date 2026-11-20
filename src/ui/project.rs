#[path = "assistance.rs"]
mod assistance;
#[path = "manuscript.rs"]
mod manuscript;
#[path = "planning.rs"]
mod planning;
#[path = "world.rs"]
mod world;
use std::{path::PathBuf, time::Duration};

use super::recent_projects::{Location, RecentProjects};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, TextDecoration, Textarea, TextareaState},
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    status_bar::StatusBar,
};
use gpui_kit::{prelude::FluentBuilder, *};
actions!(
    writer_actions,
    [
        NewProject,
        OpenProject,
        SaveProject,
        UndoScene,
        QuitWriter,
        CloseProjectWindow,
        ProjectSettings,
        ProjectExport
    ]
);

use writer::{
    Id, Project,
    passage::SceneText,
    storage::{ConflictResolution, Store},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) enum Page {
    Timeline,
    Outline,
    Scene,
    Manuscript,
    World,
    Codex,
    Consistency,
    Settings,
    Research,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Self::Timeline => "Timeline",
            Self::Outline => "Kapitel",
            Self::Scene => "Schreiben",
            Self::Manuscript => "Manuskript",
            Self::World => "Weltkartei",
            Self::Codex => "Lücke erkunden",
            Self::Consistency => "Konsistenzprüfung",
            Self::Settings => "Projekteinstellungen",
            Self::Research => "Frage zur Geschichte",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SceneInspector {
    Preview,
    Details,
}

const SCENE_INSPECTOR_MIN_WIDTH: Pixels = px(1120.);
const BOOK_SERIF: &str = if cfg!(target_os = "linux") {
    "serif"
} else {
    "Georgia"
};

pub struct ProjectView {
    keyboard_focus: FocusHandle,
    _quit_subscription: Subscription,
    title: Entity<InputState>,
    destination: Entity<InputState>,
    picked_destination: Option<PathBuf>,
    creation_origin: Option<FocusHandle>,
    busy: bool,
    error: Option<String>,
    project: Option<Project>,
    recent_projects: RecentProjects,
    selected_scene: Option<Id>,
    show_issues: bool,
    store: Option<Store>,
    editor: Entity<TextareaState>,
    merge: Entity<TextareaState>,
    merge_path: Option<PathBuf>,
    autosave: Option<Task<()>>,
    last_version: Option<String>,
    has_unversioned_changes: bool,
    version_query: Option<Task<()>>,
    last_export: Option<PathBuf>,
    scene_inspector: Option<SceneInspector>,
    preview_trigger: FocusHandle,
    details_trigger: FocusHandle,
    scene_text: Option<SceneText>,
    scene_baseline: Option<writer::Document>,
    page: Page,
    sidebar_collapsed: bool,
    planning: planning::PlanningView,
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
    pub(crate) fn restore_recent_projects(&mut self) {
        if let Some(path) = RecentProjects::default_path() {
            self.load_recent_projects(path);
        }
    }

    fn load_recent_projects(&mut self, path: PathBuf) {
        self.recent_projects = RecentProjects::read(path);
    }

    fn remember_location(&mut self, window: &mut Window) {
        let Some(project) = &self.project else { return };
        let location = Location {
            page: self.page,
            scene: self.selected_scene,
        };
        window.set_window_title(&format!("{} — {}", project.title(), self.page.title()));
        if let Err(error) = self.recent_projects.remember(project, location) {
            eprintln!("Die letzte Projektansicht konnte nicht gespeichert werden. {error}");
        }
    }

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
            has_unversioned_changes: false,
            version_query: None,
            last_export: None,
            scene_inspector: None,
            preview_trigger: cx.focus_handle(),
            details_trigger: cx.focus_handle(),
            scene_text: None,
            scene_baseline: None,
            page: Page::Timeline,
            sidebar_collapsed: false,
            planning: planning::PlanningView::default(),
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
            creation_origin: None,
            busy: false,
            error: None,
            project: None,
            recent_projects: RecentProjects::default(),
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
        if self.busy || !self.flush(cx) {
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

    fn start_creating(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.flush(cx) {
            return;
        }
        self.creation_origin.get_or_insert_with(|| {
            window
                .focused(cx)
                .unwrap_or_else(|| self.keyboard_focus.clone())
        });
        self.error = None;
        self.title.update(cx, |title, cx| title.focus(window, cx));
        cx.notify();
    }

    fn cancel_creating(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        if let Some(origin) = self.creation_origin.take() {
            window.focus(&origin, cx);
        }
        cx.notify();
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
        if self.busy || !self.flush(cx) {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let load = cx.background_executor().spawn(async move {
            let root = std::path::absolute(&root)
                .map_err(|source| writer::ProjectError::Io { path: root, source })?;
            let project = match title {
                Some(title) => Project::create(&root, &title),
                None => Project::open(&root),
            }?;
            let store = Store::new(project.root())?;
            let latest_version = store.latest_version_time();
            let has_unversioned_changes = store.has_unversioned_changes().unwrap_or_default();
            Ok::<_, writer::ProjectError>((project, store, latest_version, has_unversioned_changes))
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = load.await;
            let _ = view.update_in(cx, |view, window, cx| {
                view.busy = false;
                match result {
                    Ok((project, store, latest_version, has_unversioned_changes)) => {
                        let location = view.recent_projects.location(project.root());
                        view.store = Some(store);
                        view.last_version = latest_version;
                        view.has_unversioned_changes = has_unversioned_changes;
                        view.project = Some(project);
                        view.reset_workspace(window, cx);
                        view.restore_ai(window, cx);
                        view.selected_scene = None;
                        view.show_issues = false;
                        view.creation_origin = None;
                        if let Some(location) = location {
                            if let Some(scene) = location.scene.filter(|&id| {
                                view.project
                                    .as_ref()
                                    .is_some_and(|project| project.scene(id).is_some())
                            }) {
                                view.select_scene(scene, window, cx);
                            }
                            if location.page != Page::Scene {
                                view.navigate(location.page, window, cx);
                            }
                        }
                        view.remember_location(window);
                        window.focus(&view.keyboard_focus, cx);
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
            .when(
                self.creation_origin.is_none() && !self.recent_projects.entries().is_empty(),
                |picker| {
                    picker.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .bg(cx.theme().sidebar)
                                    .px_3()
                                    .py_2()
                                    .text_sm()
                                    .child("Zuletzt geöffnet"),
                            )
                            .children(self.recent_projects.entries().iter().enumerate().map(
                                |(index, entry)| {
                                    let root = entry.root.clone();
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap_2()
                                        .px_3()
                                        .pb_2()
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .child(entry.title.clone())
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(cx.theme().muted_foreground)
                                                        .child(entry.root.display().to_string()),
                                                ),
                                        )
                                        .child(
                                            Button::new(("recent-project", index))
                                                .small()
                                                .label("Öffnen")
                                                .disabled(self.busy)
                                                .on_click(cx.listener(
                                                    move |view, _, window, cx| {
                                                        view.load(root.clone(), None, window, cx);
                                                    },
                                                )),
                                        )
                                },
                            )),
                    )
                },
            )
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
                            .on_click(
                                cx.listener(|view, _, window, cx| view.start_creating(window, cx)),
                            ),
                    ),
            )
            .when(self.creation_origin.is_some(), |content| {
                content.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("Buchtitel")
                        .child(Input::new(&self.title).aria_label("Buchtitel"))
                        .child("Zielordner")
                        .child(
                            div()
                                .flex()
                                .gap_2()
                                .child(
                                    div().flex_1().min_w_0().child(
                                        Input::new(&self.destination).aria_label("Zielordner"),
                                    ),
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
                                        .on_click(cx.listener(|view, _, window, cx| {
                                            view.cancel_creating(window, cx)
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
        self.editor
            .update(cx, |editor, cx| apply_concept_decorations(editor, text, cx));
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
            self.refresh_version_status(cx);
        }
        self.report(result, cx)
    }

    fn refresh_version_status(&mut self, cx: &mut Context<Self>) {
        let Some(project) = &self.project else { return };
        let root = project.root().to_owned();
        self.version_query = Some(cx.spawn(async move |view, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let result =
                        Store::new(&root).and_then(|store| store.has_unversioned_changes());
                    (root, result)
                })
                .await;
            let _ = view.update(cx, |view, cx| {
                if let (root, Ok(changed)) = result
                    && view
                        .project
                        .as_ref()
                        .is_some_and(|project| project.root() == root)
                {
                    view.has_unversioned_changes = changed;
                    cx.notify();
                }
            });
        }));
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
        let text = SceneText::from_markdown(id, scene.body());
        self.editor.update(cx, |editor, cx| {
            editor.set_value(text.text().to_owned(), window, cx);
            apply_concept_decorations(editor, &text, cx);
        });
        self.scene_text = Some(text);
        self.scene_inspector = None;
        self.scene_baseline = Some(scene.clone());
        self.page = Page::Scene;
        self.selected_scene = Some(id);
        self.load_scene_time(id, window, cx);
        self.scene_opened(window, cx);
        self.remember_location(window);
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
                        view.refresh_version_status(cx);
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
            self.refresh_version_status(cx);
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

    fn document_status(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pending = self.store.as_ref().is_some_and(Store::is_pending);
        let conflict = self.store.as_ref().and_then(Store::conflict);
        let state = if conflict.is_some() {
            "Dateikonflikt"
        } else if self.autosave.is_some() {
            "Wird auf Festplatte gesichert …"
        } else if pending {
            "Dateiänderungen ausstehend"
        } else {
            "Auf Festplatte gesichert"
        };
        let mut status = StatusBar::new().left(state);
        if !pending && self.has_unversioned_changes {
            status = status.left(
                div()
                    .id("changes-since-version")
                    .child("Änderungen seit letzter Version")
                    .test_support(),
            );
        }
        if let Some(time) = &self.last_version {
            status = status.left(format!("Letzte lokale Version {time}"));
        }
        if self.page == Page::Scene && self.selected_scene.is_some() {
            status = status.right("Markdown");
        }
        if pending && conflict.is_none() && self.autosave.is_none() {
            status = status.child(
                Button::new("retry")
                    .ghost()
                    .small()
                    .label("Sicherung erneut versuchen")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.flush(cx);
                    })),
            );
        }
        div()
            .id("document-status")
            .flex_shrink_0()
            .child(status)
            .test_support()
    }

    fn conflict_controls(&self, cx: &mut Context<Self>) -> Div {
        let conflict = self.store.as_ref().and_then(Store::conflict);
        div()
            .flex()
            .flex_col()
            .gap_2()
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
        self.version_query = None;
        self.page = Page::Timeline;
        self.planning = planning::PlanningView::default();
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
        self.remember_location(window);
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
        self.scene_inspector = (self.scene_inspector != Some(inspector)).then_some(inspector);
        window.focus(self.scene_inspector_trigger(inspector), cx);
        cx.notify();
    }

    fn scene_inspector_trigger(&self, inspector: SceneInspector) -> &FocusHandle {
        match inspector {
            SceneInspector::Preview => &self.preview_trigger,
            SceneInspector::Details => &self.details_trigger,
        }
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
                        Button::new(id)
                            .small()
                            .ghost()
                            .label(title)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.scene_action(format, concept, window, cx)
                            }))
                    })),
            )
            .child(
                div()
                    .key_context("WriterScene")
                    .on_action(cx.listener(|view, _: &UndoScene, window, cx| {
                        view.scene_action(None, None, window, cx)
                    }))
                    .w_full()
                    .max_w(px(650.))
                    .mx_auto()
                    .child(
                        Textarea::new(&self.editor)
                            .font_family(cx.theme().mono_font_family.clone())
                            .aria_label("Szenentext"),
                    ),
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
                            .border_color(cx.theme().warning)
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
        let outline = writer::outline::Outline::from_project(project);
        let chapter = outline
            .chapters()
            .iter()
            .enumerate()
            .find(|(_, chapter)| chapter.scenes.contains(&scene.id()))
            .map_or_else(
                || "Ohne Kapitel".into(),
                |(index, chapter)| format!("Kapitel {} · {}", index + 1, chapter.title),
            );
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
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(scene.title().to_owned()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(chapter),
                    ),
            )
            .children(
                inspector_actions
                    .into_iter()
                    .map(|(id, open, close, inspector)| {
                        Button::new(id)
                            .small()
                            .ghost()
                            .track_focus(self.scene_inspector_trigger(inspector))
                            .selected(current == Some(inspector))
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
        let editor = (split || current.is_none()).then(|| self.scene_editor(scene.id(), cx));
        let inspector = current.map(|inspector| match inspector {
            Details => self.scene_timing(project, scene.id(), cx),
            Preview => div().child(
                div()
                    .id("scene-preview")
                    .w_full()
                    .font_family(BOOK_SERIF)
                    .children(self.scene_text.as_ref().map(|text| {
                        gpui_kit::component::text::TextView::markdown(
                            "markdown-preview",
                            writer::export::preview_markdown(text),
                        )
                    }))
                    .test_support(),
            ),
        });
        let resizable = self.scene_inspector_is_resizable(project, window);
        let panes = match (editor, inspector) {
            (Some(editor), Some(inspector)) if resizable => div().flex_1().min_h_0().child(
                h_resizable("scene-split")
                    .child(
                        resizable_panel().child(
                            editor
                                .p_4()
                                .overflow_y_scrollbar()
                                .id("scene-editor-scroll"),
                        ),
                    )
                    .child(
                        resizable_panel()
                            .size(px(340.))
                            .size_range(px(235.)..px(520.))
                            .fixed()
                            .child(
                                inspector
                                    .size_full()
                                    .p_4()
                                    .bg(cx.theme().sidebar)
                                    .overflow_y_scrollbar()
                                    .id("scene-inspector-scroll"),
                            ),
                    ),
            ),
            (editor, inspector) => div()
                .flex()
                .items_start()
                .gap_4()
                .children(editor)
                .children(inspector.map(|panel| {
                    panel
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
                })),
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(toolbar.when(resizable, |bar| {
                bar.flex_shrink_0()
                    .px_4()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
            }))
            .child(panes)
    }

    fn scene_inspector_is_resizable(&self, project: &Project, window: &Window) -> bool {
        self.scene_inspector.is_some()
            && window.viewport_size().width >= SCENE_INSPECTOR_MIN_WIDTH
            && project.issues().is_empty()
            && self
                .store
                .as_ref()
                .is_none_or(|store| store.conflict().is_none())
    }

    fn workspace(&self, project: &Project, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let narrow = window.viewport_size().width < px(790.);
        let nav_button = |index: usize, page: Page, label: &'static str, cx: &mut Context<Self>| {
            Button::new(("page", index))
                .ghost()
                .small()
                .label(label)
                .selected(self.page == page)
                .when(!narrow, |button| button.w_full().justify_start())
                .on_click(cx.listener(move |view, _, window, cx| {
                    if page == Page::Settings {
                        view.open_project_settings(window, cx);
                    } else {
                        view.navigate(page, window, cx);
                    }
                }))
        };
        let navigation = div()
            .flex()
            .gap_1()
            .p_2()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .when(narrow, |nav| {
                nav.flex_wrap().border_b_1().border_color(cx.theme().border)
            })
            .when(!narrow, |nav| nav.flex_col().size_full())
            .when(!narrow, |nav| {
                nav.child(
                    div()
                        .px_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Buchprojekt"),
                )
            })
            .children(
                [
                    (0, Page::Timeline, "Timeline"),
                    (1, Page::Outline, "Kapitel"),
                    (2, Page::Scene, "Schreiben"),
                    (3, Page::Manuscript, "Manuskript"),
                    (4, Page::World, "Welt"),
                ]
                .into_iter()
                .map(|(index, page, label)| nav_button(index, page, label, cx)),
            )
            .when(!narrow, |nav| {
                nav.child(
                    div()
                        .mt_3()
                        .px_2()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Projektwerkzeuge"),
                )
            })
            .child(nav_button(6, Page::Consistency, "Konsistenz", cx))
            .child(nav_button(7, Page::Settings, "Einstellungen", cx));
        let has_conflict = self
            .store
            .as_ref()
            .is_some_and(|store| store.conflict().is_some());
        let own_scroll = !has_conflict
            && project.issues().is_empty()
            && (matches!(self.page, Page::Timeline | Page::Outline)
                || self.page == Page::Codex && Self::wide_brainstorm(window)
                || self.page == Page::Scene && self.scene_inspector_is_resizable(project, window));
        let content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .gap_4()
            .when(!own_scroll, |panel| panel.p_4())
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
            .when(
                self.store
                    .as_ref()
                    .is_some_and(|store| store.conflict().is_some()),
                |content| content.child(self.conflict_controls(cx)),
            )
            .child(match self.page {
                Page::Timeline => self.timeline_view(project, window, cx),
                Page::Outline => self.outline_view(project, window, cx),
                Page::Scene => self.scene_view(project, window, cx),
                Page::Manuscript => self.manuscript_view(window, cx),
                Page::World => self.world_view(project, window, cx),
                Page::Codex => self.assistance_view(project, window, cx),
                Page::Consistency => self.consistency_page(project, cx),
                Page::Settings => self.settings_page(project, cx),
                Page::Research => self.research_page(project, cx),
            });
        let content = if own_scroll {
            content.into_any_element()
        } else {
            content
                .overflow_y_scrollbar()
                .id((
                    if has_conflict {
                        "workspace-conflict"
                    } else {
                        "workspace-page"
                    },
                    self.page as usize,
                ))
                .into_any_element()
        };
        let body = div().flex().flex_col().flex_1().min_h_0();
        let body = if self.sidebar_collapsed {
            body.child(content)
        } else if narrow {
            body.child(navigation).child(content)
        } else {
            body.child(
                h_resizable("document-split")
                    .child(
                        resizable_panel()
                            .size(px(200.))
                            .size_range(px(150.)..px(320.))
                            .fixed()
                            .child(navigation),
                    )
                    .child(resizable_panel().child(content)),
            )
        };
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().title_bar)
                    .child(
                        Button::new("toggle-source-list")
                            .small()
                            .ghost()
                            .label(if self.sidebar_collapsed {
                                "Quellliste einblenden"
                            } else {
                                "Quellliste ausblenden"
                            })
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.sidebar_collapsed = !view.sidebar_collapsed;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_wrap()
                            .items_baseline()
                            .gap_2()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(project.title().to_owned()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(self.page.title()),
                            ),
                    )
                    .child(
                        Button::new("checkpoint")
                            .label("Speichern")
                            .small()
                            .primary()
                            .tooltip("Lokale Git-Version erstellen")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|view, _, window, cx| view.checkpoint(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("choose-project")
                            .label("Projektwahl")
                            .small()
                            .ghost()
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| {
                                if view.flush(cx) {
                                    view.scene_left(window, cx);
                                    view.reset_workspace(window, cx);
                                    view.project = None;
                                    view.store = None;
                                    view.error = None;
                                    window.set_window_title("Schreibwerkstatt");
                                    cx.notify();
                                }
                            })),
                    ),
            )
            .child(body)
            .when_some(self.error.as_ref(), |shell, error| {
                shell.child(
                    div()
                        .flex_shrink_0()
                        .p_2()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                )
            })
            .child(self.document_status(cx))
    }
}

impl Render for ProjectView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.prepare_merge(window, cx);
        let choosing_project = self.project.is_none() || self.creation_origin.is_some();
        div()
            .id("workspace")
            .track_focus(&self.keyboard_focus)
            .on_action(cx.listener(|view, _: &CloseProjectWindow, window, cx| {
                if view.flush(cx) {
                    window.remove_window();
                }
            }))
            .when(!self.busy, |root| {
                root.on_action(cx.listener(|view, _: &OpenProject, window, cx| {
                    view.choose_directory(false, window, cx)
                }))
                .on_action(
                    cx.listener(|view, _: &NewProject, window, cx| view.start_creating(window, cx)),
                )
            })
            .when(
                self.project.is_some() && self.creation_origin.is_none() && !self.busy,
                |root| {
                    root.on_action(
                        cx.listener(|view, _: &SaveProject, window, cx| {
                            view.checkpoint(window, cx)
                        }),
                    )
                    .on_action(cx.listener(|view, _: &ProjectSettings, window, cx| {
                        view.open_project_settings(window, cx)
                    }))
                    .on_action(cx.listener(
                        |view, _: &ProjectExport, window, cx| view.show_export_formats(window, cx),
                    ))
                },
            )
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(match &self.project {
                Some(project) if self.creation_origin.is_none() => {
                    self.workspace(project, window, cx).into_any_element()
                }
                _ => div()
                    .flex_1()
                    .min_h_0()
                    .p_5()
                    .child(self.welcome(cx))
                    .overflow_y_scrollbar()
                    .into_any_element(),
            })
            .when(choosing_project && self.busy, |content| {
                content.child(
                    div()
                        .flex_shrink_0()
                        .mt_2()
                        .child("Projekt wird vorbereitet …"),
                )
            })
            .when_some(
                self.error.clone().filter(|_| choosing_project),
                |content, error| content.child(div().flex_shrink_0().mt_2().child(error)),
            )
    }
}

fn apply_concept_decorations(
    editor: &mut TextareaState,
    scene: &SceneText,
    cx: &mut Context<TextareaState>,
) {
    let color = cx.theme().warning;
    let style = HighlightStyle {
        background_color: Some(color.opacity(0.16)),
        underline: Some(UnderlineStyle {
            color: Some(color),
            thickness: px(1.),
            wavy: false,
        }),
        ..Default::default()
    };
    let decorations = scene
        .concepts()
        .iter()
        .filter(|concept| !concept.range.is_empty())
        .map(|concept| TextDecoration::new(concept.range.clone(), style))
        .collect();
    editor.set_decorations(decorations, cx);
}

#[cfg(test)]
#[path = "test_support.rs"]
pub(super) mod test_support;
