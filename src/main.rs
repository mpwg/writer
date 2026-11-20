mod ui;

use gpui_kit::*;
use ui::project::ProjectView;

fn main() {
    application().with_assets(assets::Assets).run(|cx| {
        init(cx);
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
