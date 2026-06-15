//! A read-only signal-strength meter: a horizontal −100..0 dBFS bar with a tick scale and a
//! filled level, plus the numeric readout. Reflects the live signal power; it is not a control.

use gpui::*;
use gpui_component::ActiveTheme;

use crate::app::SdrApp;
use crate::ui::tokens;

const TICKS: [&str; 6] = ["-100", "-80", "-60", "-40", "-20", "0"];

/// Render the meter for a dBFS level.
pub fn render(dbfs: f32, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let track = cx.theme().secondary;
    let fill = cx.theme().success;
    let mono = cx.theme().mono_font_family.clone();

    let level = if dbfs.is_finite() {
        dbfs.clamp(-100.0, 0.0)
    } else {
        -100.0
    };
    let frac = (level + 100.0) / 100.0;
    let readout = if dbfs.is_finite() {
        format!("{dbfs:.1} dBFS")
    } else {
        "— dBFS".to_string()
    };

    div()
        .flex()
        .flex_col()
        .gap(px(2.))
        .w(px(240.))
        .font_family(mono)
        .text_color(muted)
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_size(tokens::TEXT_SM)
                .children(TICKS.into_iter().map(|t| div().child(t))),
        )
        .child(
            div()
                .h(px(7.))
                .w_full()
                .rounded(px(2.))
                .bg(track)
                .child(div().h_full().rounded(px(2.)).bg(fill).w(relative(frac))),
        )
        .child(div().text_size(tokens::TEXT_AXIS).child(readout))
}
