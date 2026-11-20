mod ui;

use gpui_kit::*;
use ui::project::{ProjectView, SaveProject, UndoScene};

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);
        cx.bind_keys(writer_key_bindings());
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
            ..Default::default()
        };
        match open_window(options, cx, |window, cx| {
            cx.new(|cx| ProjectView::new(window, cx))
        }) {
            Ok(_) => cx.activate(true),
            Err(error) => {
                eprintln!("Das Fenster konnte nicht geöffnet werden: {error}");
                cx.quit();
            }
        }
    });
}

fn editor_undo_shortcut() -> &'static str {
    if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    }
}

fn writer_key_bindings() -> [KeyBinding; 2] {
    [
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
    ]
}

#[cfg(test)]
mod tests {
    use super::ui;
    use gpui_kit::{KeyBinding, KeyContext, Keymap, Keystroke};

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
