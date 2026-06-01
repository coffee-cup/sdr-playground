use gpui::*;
use gpui_component::{ActiveTheme, TitleBar};

use crate::app::SdrApp;
use crate::nav::Workspace;

/// The integrated title bar. `TitleBar` reserves the macOS traffic-light inset itself,
/// so content here starts after it. Holds the wordmark, the active workspace, the
/// current-channel readout, and the command-palette hint.
pub fn render(active: Workspace, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let mono = cx.theme().mono_font_family.clone();

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
                    .child(div().text_xs().text_color(muted).child(active.title()))
                    .child(separator())
                    .child(
                        div()
                            .font_family(mono)
                            .text_xs()
                            .text_color(muted)
                            .child("133.700 MHz   AM   12k   g28"),
                    ),
            )
            .child(div().text_xs().text_color(muted).child("⌘K")),
    )
}
