use gpui::*;
use gpui_component::{ActiveTheme, IconName};

use crate::app::SdrApp;
use crate::ui::{icon_button, inset};

/// Full-width strip for record and replay, present in every workspace so recording is
/// always one action away: transport controls, the DVR buffer scrubber, buffer depth,
/// record state, and sample rate (see `docs/UI.md`). Play/pause freezes the live display;
/// the rest is a static placeholder for now.
pub fn render(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let background = cx.theme().background;
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let record = cx.theme().danger;
    let fill = cx.theme().primary;
    let mono = cx.theme().mono_font_family.clone();

    let running = app.radio().engine().is_some();
    // Paused shows Play (resume); live shows Pause (freeze).
    let play_icon = if app.paused() {
        IconName::Play
    } else {
        IconName::Pause
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .w_full()
        .h(px(38.))
        .px_3()
        .border_t_1()
        .border_color(border)
        .bg(background)
        .font_family(mono)
        .text_xs()
        .text_color(muted)
        .child(
            // Transport: play/pause and the record indicator.
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(icon_button(
                    "transport-play",
                    play_icon,
                    running,
                    cx,
                    |app, _, cx| app.toggle_pause(cx),
                ))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(div().size(px(9.)).rounded_full().bg(record))
                        .child("REC"),
                ),
        )
        .child(
            // DVR buffer scrubber: a near-black well with an orange filled portion.
            inset(cx)
                .flex_1()
                .h(px(8.))
                .child(div().h_full().w(px(180.)).bg(fill)),
        )
        .child("-12s")
        .child("44.1 kHz")
}
