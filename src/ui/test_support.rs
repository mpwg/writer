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
    let (window, view) = cx.update(|cx| {
        init(cx);
        cx.bind_keys(crate::writer_key_bindings());
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
                    if with_project {
                        view.project = Some(Project::open(&root).unwrap());
                        view.store = Some(Store::new(&root).unwrap());
                    }
                    view
                })
            },
        )
        .unwrap()
    });
    TestWorkspace {
        window,
        view,
        root,
        _tmp: tmp,
    }
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
fn native_key_dispatch_undoes_a_scene_edit_and_persists_the_previous_text(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
    let path = workspace.root.join("Manuskript/Signal.md");
    let original = fs::read(&path).unwrap();
    workspace.update(cx, |view, window, cx| view.select_scene(id, window, cx));
    cx.update_window(workspace.window, |_, window, cx| {
        window.click(("input", workspace.view.read(cx).editor.entity_id()), cx);
        window.input("Überprüfung", cx);
    })
    .unwrap();
    workspace.update(cx, |view, _, cx| {
        assert!(view.editor.read(cx).value().contains("Überprüfung"));
        assert!(view.flush(cx));
    });
    assert_ne!(fs::read(&path).unwrap(), original);

    cx.simulate_keystrokes(workspace.window, crate::editor_undo_shortcut());
    workspace.update(cx, |view, _, cx| {
        assert!(!view.editor.read(cx).value().contains("Überprüfung"));
        assert!(view.flush(cx));
    });
    assert_eq!(fs::read(&path).unwrap(), original);
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
fn conflict_panel_loads_the_externally_edited_scene_on_confirmation(cx: &mut TestAppContext) {
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
fn save_shortcut_flushes_the_editor_and_creates_one_local_version(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;

    let workspace = open(cx);
    let id = Id::from_u128(0x00000000_0000_4000_8000_000000000007);
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
    assert_eq!(git(&["rev-list", "--count", "HEAD"]).trim(), "1");
    let saved = git(&["show", "HEAD:Manuskript/Signal.md"]);
    assert!(saved.contains("Version über Tastatur."));
    assert_eq!(
        saved,
        fs::read_to_string(workspace.root.join("Manuskript/Signal.md")).unwrap()
    );
    cx.simulate_keystrokes(workspace.window, shortcut);
    cx.run_until_parked();
    assert_eq!(git(&["rev-list", "--count", "HEAD"]).trim(), "1");
}
