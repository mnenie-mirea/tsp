#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui_kit::AppContext as _;
use gpui_kit::component::TitleBar;

fn main() {
    let options = gpui_kit::WindowOptions {
        titlebar: Some(TitleBar::title_bar_options()),
        window_bounds: Some(gpui_kit::WindowBounds::Windowed(gpui_kit::Bounds {
            origin: gpui_kit::Point::default(),
            size: gpui_kit::Size {
                width: gpui_kit::px(1100.),
                height: gpui_kit::px(720.),
            },
        })),
        ..Default::default()
    };

    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(move |cx| {
        gpui_kit::init(cx);
        editor::app::bind_keys(cx);
        cx.activate(true);

        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| editor::app::EditorApp::new(window, cx))
        })
        .expect("Failed to open window");
    });
}
