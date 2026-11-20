mod ui;

use gpui_kit::*;
use ui::project::{
    NewProject, OpenProject, ProjectExport, ProjectSettings, ProjectView, QuitWriter, SaveProject,
    UndoScene,
};

fn main() {
    let app = application().with_assets(assets::Assets);
    app.on_reopen(open_workspace_window);
    app.run(|cx| {
        init(cx);
        cx.bind_keys(writer_key_bindings());
        ProjectView::install_quit_action(cx);
        #[cfg(target_os = "macos")]
        cx.set_menus([
            Menu::new("Schreibwerkstatt").items([
                MenuItem::action("Projekteinstellungen …", ProjectSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Dienste", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Schreibwerkstatt beenden", QuitWriter),
            ]),
            Menu::new("Ablage").items([
                MenuItem::action("Neues Buchprojekt …", NewProject),
                MenuItem::action("Buchprojekt öffnen …", OpenProject),
                MenuItem::separator(),
                MenuItem::action("Speichern", SaveProject),
                MenuItem::action("Manuskript exportieren …", ProjectExport),
            ]),
        ]);
        open_workspace_window(cx);
    });
}

fn open_workspace_window(cx: &mut App) {
    if !cx.windows().is_empty() {
        return;
    }
    let options = WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some("Schreibwerkstatt".into()),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(920.), px(680.)),
            cx,
        ))),
        window_min_size: Some(size(px(640.), px(480.))),
        ..Default::default()
    };
    match open_window(options, cx, |window, cx| {
        cx.new(|cx| {
            let mut view = ProjectView::new(window, cx);
            view.restore_recent_projects();
            view
        })
    }) {
        Ok(_) => cx.activate(true),
        Err(error) => {
            eprintln!("Das Fenster konnte nicht geöffnet werden: {error}");
            cx.quit();
        }
    }
}

fn editor_undo_shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    }
}

fn writer_key_bindings() -> [KeyBinding; 6] {
    [
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-,"
            } else {
                "ctrl-,"
            },
            ProjectSettings,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-n"
            } else {
                "ctrl-n"
            },
            NewProject,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-o"
            } else {
                "ctrl-o"
            },
            OpenProject,
            None,
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-s"
            } else {
                "ctrl-s"
            },
            SaveProject,
            None,
        ),
        KeyBinding::new(
            editor_undo_shortcut(),
            UndoScene,
            Some("WriterScene > Input"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-q"
            } else {
                "ctrl-q"
            },
            QuitWriter,
            None,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::ui;
    use gpui_kit::{AppContext, KeyBinding, KeyContext, Keymap, Keystroke};

    #[gpui_kit::test]
    fn reopen_creates_a_project_picker_only_when_no_window_exists(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::test::TestWindowExt;

        cx.update(|cx| {
            gpui_kit::init(cx);
            cx.set_quit_mode(gpui_kit::QuitMode::Explicit);
            super::open_workspace_window(cx);
        });
        let original = cx.windows()[0];

        cx.update(super::open_workspace_window);
        assert_eq!(cx.windows().len(), 1);
        cx.update_window(original, |_, window, _| window.remove_window())
            .unwrap();
        assert!(cx.windows().is_empty());

        cx.update(super::open_workspace_window);
        let [reopened]: [_; 1] = cx.windows().try_into().unwrap();
        assert_ne!(reopened.window_id(), original.window_id());
        cx.update_window(reopened, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("open").visible());
        })
        .unwrap();
    }

    #[test]
    fn scene_undo_takes_precedence_only_in_the_scene_editor() {
        let mut keymap = Keymap::default();
        keymap.add_bindings([KeyBinding::new(
            super::editor_undo_shortcut(),
            ui::project::SaveProject,
            Some("Input"),
        )]);
        keymap.add_bindings(super::writer_key_bindings());
        let undo = Keystroke::parse(super::editor_undo_shortcut()).unwrap();
        let input = KeyContext::parse("Input").unwrap();
        let scene = KeyContext::parse("WriterScene").unwrap();
        let (scene_actions, _) =
            keymap.bindings_for_input(std::slice::from_ref(&undo), &[scene, input.clone()]);
        assert!(
            scene_actions[0]
                .action()
                .partial_eq(&ui::project::UndoScene)
        );
        let (other_actions, _) = keymap.bindings_for_input(&[undo], &[input]);
        assert!(
            other_actions[0]
                .action()
                .partial_eq(&ui::project::SaveProject)
        );
    }
}
