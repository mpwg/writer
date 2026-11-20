use std::path::PathBuf;

use gpui_kit::component::{
    ActiveTheme, Disableable,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
};
use gpui_kit::{prelude::FluentBuilder, *};
use writer::{Id, Project};

pub struct ProjectView {
    title: Entity<InputState>,
    destination: Entity<InputState>,
    picked_destination: Option<PathBuf>,
    creating: bool,
    busy: bool,
    error: Option<String>,
    project: Option<Project>,
    selected_scene: Option<Id>,
    show_issues: bool,
}

impl ProjectView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
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
            match title {
                Some(title) => Project::create(&root, &title),
                None => Project::open(&root),
            }
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = load.await;
            let _ = view.update(cx, |view, cx| {
                view.busy = false;
                match result {
                    Ok(project) => {
                        view.project = Some(project);
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

    fn workspace(&self, project: &Project, cx: &mut Context<Self>) -> Div {
        let scenes = project.scenes().map(|scene| {
            let id = scene.id();
            Button::new(id.to_string())
                .label(scene.title().to_owned())
                .on_click(cx.listener(move |view, _, _, cx| {
                    view.selected_scene = Some(id);
                    cx.notify();
                }))
        });
        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                Button::new("choose-project")
                    .label("Zur Projektwahl")
                    .on_click(cx.listener(|view, _, _, cx| {
                        view.project = None;
                        view.error = None;
                        cx.notify();
                    })),
            )
            .child(div().text_2xl().child(project.title().to_owned()))
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
            .child(div().text_lg().child("Szenen"))
            .child(div().flex().flex_wrap().gap_2().children(scenes))
            .when(project.scenes().next().is_none(), |content| {
                content.child("Dieses Buch enthält noch keine Szenen.")
            })
            .when_some(
                self.selected_scene.and_then(|id| project.scene(id)),
                |content, scene| {
                    content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(div().text_lg().child(scene.title().to_owned()))
                            .children(
                                scene
                                    .body()
                                    .split('\n')
                                    .map(|line| div().min_h(px(20.)).child(line.to_owned())),
                            ),
                    )
                },
            )
    }
}

impl Render for ProjectView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id("workspace")
            .size_full()
            .overflow_y_scroll()
            .bg(theme.background)
            .text_color(theme.foreground)
            .p_8()
            .child(match &self.project {
                Some(project) => self.workspace(project, cx),
                None => self.welcome(cx),
            })
            .when(self.busy, |content| {
                content.child(div().mt_4().child("Projekt wird vorbereitet …"))
            })
            .when_some(self.error.clone(), |content, error| {
                content.child(div().mt_4().child(error))
            })
    }
}
