use gpui::*;
use gpui_component::{ActiveTheme, TitleBar};

use crate::app::SdrApp;
use crate::settings::bandwidth_label;
use crate::ui::value_box;

/// The integrated title bar. `TitleBar` reserves the macOS traffic-light inset itself, so
/// content here starts after it. Holds the wordmark, the active workspace, the tuned frequency,
/// flat status chips (sample rate, bandwidth), and the command-palette hint.
pub fn render(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let mono = cx.theme().mono_font_family.clone();

    let freq = format!("{:.3} MHz", app.tuned_freq() as f64 / 1e6);
    let separator = || div().w(px(1.)).h(px(12.)).bg(border);

    let mut left = div()
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
                .text_color(foreground)
                .child(freq),
        );

    match app.radio().engine() {
        Some(engine) => {
            let rate = format!("{:.3} MS/s", engine.snapshot().sample_rate as f64 / 1e6);
            left = left.child(value_box("Rate", rate, cx)).child(value_box(
                "BW",
                bandwidth_label(app.settings().bandwidth),
                cx,
            ));
        }
        None => {
            left = left.child(div().text_xs().text_color(muted).child("no device"));
        }
    }

    TitleBar::new().child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .size_full()
            .pr_3()
            .child(left)
            .child(div().text_xs().text_color(muted).child("⌘K")),
    )
}
