use gpui::*;
use gpui_component::{Root, Theme, ThemeMode, TitleBar};
use gpui_component_assets::Assets;

mod app;
mod colormap;
mod components;
mod controls;
mod nav;
mod radio;
mod settings;
mod signal;
mod store;
mod ui;
mod workspaces;

use app::SdrApp;

actions!(sdr, [Quit]);

fn main() {
    let application = gpui_platform::application().with_assets(Assets);

    application.run(move |cx| {
        // Required before any gpui-component features (theme, components) are used.
        gpui_component::init(cx);

        // Cmd+Q quits. The keybinding dispatches the action; the menu item gives macOS
        // its standard Quit entry (and the same shortcut).
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.set_menus(vec![Menu {
            name: "SDR".into(),
            items: vec![MenuItem::action("Quit", Quit)],
            disabled: false,
        }]);

        // Dark theme. Labels and reading text use the default proportional UI font;
        // monospace is reserved for values/data (frequencies, readouts) and applied
        // per-element via the theme's mono family.
        Theme::change(ThemeMode::Dark, None, cx);
        Theme::global_mut(cx).mono_font_family = "JetBrains Mono".into();
        // Recolor to the Ableton-dark palette (must run after Theme::change, which resets colors).
        ui::theme::apply(cx);

        cx.spawn(async move |cx| {
            let options = WindowOptions {
                // Integrated title bar: the native macOS bar goes transparent and we
                // paint our own chrome into the same strip (traffic lights remain).
                titlebar: Some(TitleBar::title_bar_options()),
                window_min_size: Some(Size {
                    width: px(820.),
                    height: px(560.),
                }),
                ..Default::default()
            };

            cx.open_window(options, |window, cx| {
                let view = cx.new(|cx| SdrApp::new(window, cx));
                // Open the device and start streaming as soon as the window exists.
                view.update(cx, |app, cx| app.connect(cx));
                // The top-level view on a window must be a `Root`: it owns the overlay
                // layers (dialogs, sheets, notifications) that gpui-component renders into.
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("failed to open window");
        })
        .detach();
    });
}
