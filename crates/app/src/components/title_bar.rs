use gpui::*;
use gpui_component::{ActiveTheme, TitleBar};

use crate::app::SdrApp;

/// The integrated title bar. `TitleBar` reserves the macOS traffic-light inset itself, so
/// content here starts after it. Holds the wordmark, the active workspace, the live
/// current-channel readout, and the command-palette hint.
pub fn render(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let mono = cx.theme().mono_font_family.clone();

    let readout = match app.radio().engine() {
        Some(engine) => {
            let s = engine.snapshot();
            format!(
                "{:.3} MHz   {:.3} MS/s",
                s.center_freq as f64 / 1e6,
                s.sample_rate as f64 / 1e6,
            )
        }
        None => "— no device".to_string(),
    };

    let separator = || div().w(px(1.)).h(px(12.)).bg(border);

    TitleBar::new().child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .size_full()
            .pr_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(foreground)
                            .child("SDR"),
                    )
                    .child(separator())
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(app.active().title()),
                    )
                    .child(separator())
                    .child(
                        div()
                            .font_family(mono)
                            .text_xs()
                            .text_color(muted)
                            .child(readout),
                    ),
            )
            .child(div().text_xs().text_color(muted).child("⌘K")),
    )
}
