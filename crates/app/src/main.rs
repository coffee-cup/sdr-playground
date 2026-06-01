use gpui::*;
use gpui_component::Root;
use gpui_component_assets::Assets;

mod app;
mod components;
mod nav;
mod workspaces;

use app::SdrApp;

fn main() {
    let application = gpui_platform::application().with_assets(Assets);

    application.run(move |cx| {
        // Required before any gpui-component features (theme, components) are used.
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| SdrApp::new(window, cx));
                // The top-level view on a window must be a `Root`: it owns the overlay
                // layers (dialogs, sheets, notifications) that gpui-component renders into.
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}
