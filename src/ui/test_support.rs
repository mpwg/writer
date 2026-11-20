use gpui_kit::{
    AnyWindowHandle, AppContext, Bounds, Context, ElementInputHandler, Entity, InputHandler,
    TestAppContext, Window, WindowBounds, WindowOptions, init, open_window, point, px, size,
};
use std::path::PathBuf;
use writer::{Id, Project, storage::Store};

use super::{Page, ProjectView};
use std::{fs, path::Path, process::Command};

pub(super) struct TestWorkspace {
    pub window: AnyWindowHandle,
    pub view: Entity<ProjectView>,
    pub root: PathBuf,
    _tmp: tempfile::TempDir,
}

impl TestWorkspace {
    pub(super) fn update<R>(
        &self,
        cx: &mut TestAppContext,
        f: impl FnOnce(&mut ProjectView, &mut Window, &mut Context<ProjectView>) -> R,
    ) -> R {
        cx.update_window(self.window, |_, window, cx| {
            self.view.update(cx, |view, cx| f(view, window, cx))
        })
        .expect("Testfenster")
    }
}

pub(super) fn open(cx: &mut TestAppContext) -> TestWorkspace {
    open_workspace(cx, true)
}

fn open_workspace(cx: &mut TestAppContext, with_project: bool) -> TestWorkspace {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("Imperium");
    copy_project(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/imperium")
            .as_path(),
        &root,
    );
    for args in [
        &["init", "--quiet"][..],
        &["config", "user.name", "Komponententest"],
        &["config", "user.email", "komponententest@example.invalid"],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    cx.update(|cx| {
        init(cx);
        cx.bind_keys(crate::writer_key_bindings());
        ProjectView::install_quit_action(cx);
    });
    let (window, view) = mount_view(cx, with_project.then_some(root.as_path()));
    TestWorkspace {
        window,
        view,
        root,
        _tmp: tmp,
    }
}

fn mount_view(
    cx: &mut TestAppContext,
    project_root: Option<&Path>,
) -> (AnyWindowHandle, Entity<ProjectView>) {
    cx.update(|cx| {
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(0.), px(0.)),
                    size: size(px(1440.), px(1500.)),
                })),
                ..Default::default()
            },
            cx,
            |window, cx| {
                cx.new(|cx| {
                    let mut view = ProjectView::new(window, cx);
                    if let Some(root) = project_root {
                        view.project = Some(Project::open(root).unwrap());
                        view.store = Some(Store::new(root).unwrap());
                    }
                    view
                })
            },
        )
        .unwrap()
    })
}

fn copy_project(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_project(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

#[gpui_kit::test]
fn project_picker_accepts_the_simulated_folder_response(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open_workspace(cx, false);
    cx.update_window(workspace.window, |_, window, cx| window.click("open", cx))
        .unwrap();
    assert!(cx.did_prompt_for_paths());
    let root = workspace.root.clone();
    cx.simulate_path_prompt_response(move |options| {
        assert!(options.directories && !options.files);
        Some(vec![root])
    });
    cx.run_until_parked();
    workspace.update(cx, |view, _, _| {
        assert_eq!(
            view.project.as_ref().unwrap().title(),
            "Das verlorene Imperium"
        );
        assert!(view.page == Page::Timeline);
    });
}

#[gpui_kit::test]
fn native_key_dispatch_undoes_a_long_scene_edit_and_persists_the_previous_text(
    cx: &mut TestAppContext,
) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let path = workspace.root.join("Manuskript/Signal.md");
    let original = fs::read(&path).unwrap();
    let sample = "Über die Antenne hört Rüdiger ein Signal.\n".repeat(900);
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    cx.update_window(workspace.window, |_, window, cx| {
        window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(sample.clone()));
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-v"
            } else {
                "ctrl-v"
            },
            cx,
        );
    })
    .unwrap();
    workspace.update(cx, |view, _, cx| {
        assert!(view.editor.read(cx).value().contains(&sample));
        assert!(view.flush(cx));
    });
    assert!(fs::read_to_string(&path).unwrap().contains(&sample));

    cx.simulate_keystrokes(workspace.window, crate::editor_undo_shortcut());
    workspace.update(cx, |view, _, cx| {
        assert!(!view.editor.read(cx).value().contains(&sample));
        assert!(view.flush(cx));
    });
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[gpui_kit::test]
fn scene_scroll_keeps_project_navigation_and_save_in_place(cx: &mut TestAppContext) {
    assert_scene_scroll_keeps_project_controls(cx, px(1080.));
}

#[gpui_kit::test]
fn scene_scroll_keeps_project_controls_in_place_in_a_narrow_window(cx: &mut TestAppContext) {
    assert_scene_scroll_keeps_project_controls(cx, px(680.));
}

fn assert_scene_scroll_keeps_project_controls(cx: &mut TestAppContext, width: gpui_kit::Pixels) {
    use gpui_kit::{ScrollDelta, test::TestWindowExt};

    let workspace = open(cx);
    cx.simulate_window_resize(workspace.window, size(width, px(520.)));
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));

    cx.update_window(workspace.window, |_, window, cx| {
        window.click("scene-details", cx);
    })
    .unwrap();
    cx.update_window(workspace.window, |_, window, cx| {
        window.render_frame(cx);
        let save = window.find("checkpoint").bounds();
        let chapters = window.find(("page", 1_usize)).bounds();
        let scene_details = window.find("scene-details").bounds();
        window.scroll(
            "scene-details",
            ScrollDelta::Pixels(point(px(0.), px(-220.))),
            cx,
        );
        assert!(window.find("scene-details").bounds().origin.y < scene_details.origin.y);
        assert_eq!(window.find("checkpoint").bounds(), save);
        assert_eq!(window.find(("page", 1_usize)).bounds(), chapters);
    })
    .unwrap();
}

#[gpui_kit::test]
fn scene_composition_saves_only_after_the_input_method_commits(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let path = workspace.root.join("Manuskript/Signal.md");
    let original = fs::read(&path).unwrap();
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    let mut handler = cx
        .update_window(workspace.window, |_, window, cx| {
            window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
            let editor = workspace.view.read(cx).editor.clone();
            ElementInputHandler::new(window.find(("input", editor.entity_id())).bounds(), editor)
        })
        .unwrap();
    cx.update_window(workspace.window, |_, window, cx| {
        handler.replace_and_mark_text_in_range(Some(0..0), "に", None, window, cx);
        window.render_frame(cx);
        assert!(
            window
                .find(("input", workspace.view.read(cx).editor.entity_id()))
                .value()
                .unwrap()
                .starts_with("に")
        );
    })
    .unwrap();
    workspace.update(cx, |view, _, cx| assert!(view.flush(cx)));
    assert_eq!(fs::read(&path).unwrap(), original);

    cx.update_window(workspace.window, |_, window, cx| {
        handler.replace_text_in_range(None, "日本語", window, cx);
    })
    .unwrap();
    workspace.update(cx, |view, _, cx| assert!(view.flush(cx)));
    assert!(fs::read_to_string(path).unwrap().contains("日本語"));
}

#[gpui_kit::test]
fn empty_merge_keeps_both_conflict_versions(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let path = workspace.root.join("Manuskript/Signal.md");
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    let external = format!("{}\nExterne Fassung.", fs::read_to_string(&path).unwrap());
    fs::write(&path, &external).unwrap();
    cx.update_window(workspace.window, |_, window, cx| {
        window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
        window.input("Interne Fassung.", cx);
    })
    .unwrap();
    workspace.update(cx, |view, _, cx| assert!(!view.flush(cx)));

    cx.update_window(workspace.window, |_, window, cx| {
        window.render_frame(cx);
        let field = ("input", workspace.view.read(cx).merge.entity_id());
        assert!(
            window
                .find(field)
                .value()
                .unwrap()
                .contains("Interne Fassung.")
        );
    })
    .unwrap();
    workspace.update(cx, |view, window, cx| {
        view.merge
            .update(cx, |input, cx| input.set_value("", window, cx));
    });
    cx.update_window(workspace.window, |_, window, cx| {
        window.click("merge-apply", cx)
    })
    .unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), external);
    workspace.update(cx, |view, _, _| {
        let conflict = view.store.as_ref().unwrap().conflict().unwrap();
        assert!(String::from_utf8_lossy(&conflict.in_app).contains("Interne Fassung."));
    });

    workspace.update(cx, |view, window, cx| {
        let merged = format!("{external}\nZusammengeführte Fassung.");
        view.merge
            .update(cx, |input, cx| input.set_value(merged, window, cx));
    });
    cx.update_window(workspace.window, |_, window, cx| {
        window.click("merge-apply", cx)
    })
    .unwrap();
    let reopened = Project::open(&workspace.root).unwrap();
    assert!(
        reopened
            .scene(id)
            .unwrap()
            .body()
            .contains("Zusammengeführte Fassung.")
    );
}

#[gpui_kit::test]
fn conflict_resolution_survives_reopening_the_book_in_a_new_view(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let path = workspace.root.join("Manuskript/Signal.md");
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    let external = format!("{}\nExterne Fassung.", fs::read_to_string(&path).unwrap());
    fs::write(&path, &external).unwrap();

    cx.update_window(workspace.window, |_, window, cx| {
        window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
        window.input("Interne Fassung.", cx);
    })
    .unwrap();
    workspace.update(cx, |view, _, cx| assert!(!view.flush(cx)));
    assert_eq!(fs::read_to_string(&path).unwrap(), external);

    cx.update_window(workspace.window, |_, window, cx| {
        window.click("keep-disk", cx);
    })
    .unwrap();
    workspace.update(cx, |view, _, cx| {
        assert!(view.store.as_ref().unwrap().conflict().is_none());
        assert!(view.editor.read(cx).value().contains("Externe Fassung."));
        assert!(!view.editor.read(cx).value().contains("Interne Fassung."));
    });
    assert_eq!(fs::read_to_string(&path).unwrap(), external);

    let (fresh_window, fresh_view) = mount_view(cx, None);
    cx.update_window(workspace.window, |_, window, _| window.remove_window())
        .unwrap();
    cx.update_window(fresh_window, |_, window, cx| window.click("open", cx))
        .unwrap();
    let root = workspace.root.clone();
    cx.simulate_path_prompt_response(move |_| Some(vec![root]));
    cx.run_until_parked();

    cx.update_window(fresh_window, |_, window, cx| {
        window.click(("page", 1_usize), cx);
        window.click(format!("scene-{id}"), cx);
        let editor = fresh_view.read(cx).editor.entity_id();
        let observed = window.find(("input", editor));
        let text = observed.value().unwrap();
        assert!(text.contains("Externe Fassung."));
        assert!(!text.contains("Interne Fassung."));
    })
    .unwrap();
}

#[gpui_kit::test]
fn application_shutdown_flushes_pending_scene_text(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let path = workspace.root.join("Manuskript/Signal.md");
    let original = fs::read(&path).unwrap();
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    cx.update_window(workspace.window, |_, window, cx| {
        window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
        window.input("Sofort beendet. ", cx);
    })
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), original);

    cx.update(gpui_kit::App::shutdown);

    assert!(
        fs::read_to_string(path)
            .unwrap()
            .contains("Sofort beendet.")
    );
}

#[gpui_kit::test]
fn quit_shortcut_keeps_the_conflicting_scene_open(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let (workspace, path, external) = edit_scene_with_external_changes(cx);
    cx.update_window(workspace.window, |_, window, cx| {
        window.press(
            if cfg!(target_os = "macos") {
                "cmd-q"
            } else {
                "ctrl-q"
            },
            cx,
        );
    })
    .unwrap();
    cx.update_window(workspace.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find("keep-app")
                .is_some_and(|button| button.visible())
        );
        assert!(
            window
                .try_find("keep-disk")
                .is_some_and(|button| button.visible())
        );
    })
    .unwrap();

    assert_eq!(fs::read_to_string(path).unwrap(), external);
    workspace.update(cx, |view, _, cx| {
        assert!(
            view.editor
                .read(cx)
                .value()
                .contains("In der App bearbeitet.")
        );
    });
}

#[gpui_kit::test]
fn app_quit_command_checks_for_conflicts_without_editor_focus(cx: &mut TestAppContext) {
    let (workspace, path, external) = edit_scene_with_external_changes(cx);
    cx.update_window(workspace.window, |_, window, cx| window.blur(cx))
        .unwrap();

    cx.update(|cx| cx.dispatch_action(&super::QuitWriter));

    assert_eq!(fs::read_to_string(path).unwrap(), external);
    workspace.update(cx, |view, _, _| {
        let conflict = view.store.as_ref().unwrap().conflict();
        assert!(
            conflict.is_some(),
            "Die App muss den Konflikt beim Beenden anzeigen"
        );
        assert!(
            String::from_utf8_lossy(&conflict.unwrap().in_app).contains("In der App bearbeitet.")
        );
    });
}

fn edit_scene_with_external_changes(cx: &mut TestAppContext) -> (TestWorkspace, PathBuf, String) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let path = workspace.root.join("Manuskript/Signal.md");
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    let external = format!(
        "{}\nAußerhalb bearbeitet.",
        fs::read_to_string(&path).unwrap()
    );
    fs::write(&path, &external).unwrap();
    cx.update_window(workspace.window, |_, window, cx| {
        window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
        window.input("In der App bearbeitet. ", cx);
    })
    .unwrap();
    (workspace, path, external)
}

#[gpui_kit::test]
fn project_picker_can_create_a_book_without_using_a_native_dialog(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open_workspace(cx, false);
    let target = workspace.root.with_file_name("Neues Buch");
    cx.update_window(workspace.window, |_, window, cx| {
        window.click("new", cx);
        let view = workspace.view.read(cx);
        let title = view.title.entity_id();
        let destination = view.destination.entity_id();
        window.click(("input", title), cx);
        window.input("Ein Buch für Rüdiger", cx);
        window.click(("input", destination), cx);
        window.input(target.to_str().unwrap(), cx);
        window.click("create", cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        Project::open(&target).unwrap().title(),
        "Ein Buch für Rüdiger"
    );
    assert!(target.join(".git").is_dir());
    workspace.update(cx, |view, _, _| assert!(view.page == Page::Timeline));
}

#[gpui_kit::test]
fn project_picker_tab_reaches_the_first_action_on_start(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open_workspace(cx, false);
    cx.update_window(workspace.window, |_, window, cx| window.press("tab", cx))
        .unwrap();
    cx.update_window(workspace.window, |_, window, _| {
        assert_eq!(window.find("open").focused(), Some(true));
    })
    .unwrap();
}

#[gpui_kit::test]
fn save_shortcut_creates_one_local_version_after_retrying_a_locked_index(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&workspace.root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let initial = Store::new(&workspace.root)
        .unwrap()
        .checkpoint("Ausgangsfassung")
        .unwrap();
    fs::write(workspace.root.join("fremd.txt"), "Fremd").unwrap();
    git(&["add", "fremd.txt"]);
    let index_lock = workspace.root.join(".git/index.lock");
    fs::write(&index_lock, []).unwrap();
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    cx.update_window(workspace.window, |_, window, cx| {
        window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
        window.input("Version über Tastatur. ", cx);
    })
    .unwrap();
    let shortcut = if cfg!(target_os = "macos") {
        "cmd-s"
    } else {
        "ctrl-s"
    };
    cx.simulate_keystrokes(workspace.window, shortcut);
    cx.run_until_parked();
    workspace.update(cx, |view, _, _| {
        assert!(
            view.error
                .as_deref()
                .is_some_and(|error| error.contains("Lokale Version"))
        );
    });
    let scene_path = "Manuskript/Signal.md";
    let disk_text = fs::read_to_string(workspace.root.join(scene_path)).unwrap();
    assert!(disk_text.contains("Version über Tastatur."));
    assert_eq!(git(&["rev-parse", "HEAD"]).trim(), initial.id);
    assert!(!git(&["show", "HEAD:Manuskript/Signal.md"]).contains("Version über Tastatur."));

    fs::remove_file(index_lock).unwrap();
    cx.simulate_keystrokes(workspace.window, shortcut);
    cx.run_until_parked();
    assert_eq!(git(&["show", "HEAD:Manuskript/Signal.md"]), disk_text);
    cx.simulate_keystrokes(workspace.window, shortcut);
    cx.run_until_parked();
    assert_eq!(git(&["rev-list", "--count", "HEAD"]).trim(), "2");
    assert_eq!(
        git(&["diff", "--cached", "--name-only"]).trim(),
        "fremd.txt"
    );
    assert!(!git(&["ls-tree", "--name-only", "HEAD"]).contains("fremd.txt"));
}
