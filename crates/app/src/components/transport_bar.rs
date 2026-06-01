use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName};

use crate::app::SdrApp;

/// Full-width strip for record and replay, present in every workspace so recording is
/// always one action away: transport controls, the DVR buffer scrubber, buffer depth,
/// record state, and sample rate (see `docs/UI.md`). Play/pause freezes the live display;
/// the rest is a static placeholder for now.
pub fn render(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let background = cx.theme().background;
    let border = cx.theme().border;
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let track = cx.theme().secondary;
    let record = cx.theme().danger;
    let mono = cx.theme().mono_font_family.clone();

    let running = app.radio().engine().is_some();
    // Paused shows Play (resume); live shows Pause (freeze).
    let play_icon = if app.paused() {
        IconName::Play
    } else {
        IconName::Pause
    };
    let play_color = if running { foreground } else { muted };

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
            // Transport: play / stop / record.
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(
                    div()
                        .id("transport-play")
                        .cursor_pointer()
                        .child(Icon::new(play_icon).size_4().text_color(play_color))
                        .on_click(cx.listener(|app, _, _, cx| app.toggle_pause(cx))),
                )
                .child(div().size(px(9.)).bg(muted))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .child(div().size(px(8.)).rounded_full().bg(record))
                        .child("REC"),
                ),
        )
        .child(
            // DVR buffer scrubber.
            div()
                .flex_1()
                .h(px(4.))
                .bg(track)
                .child(div().h_full().w(px(180.)).bg(muted)),
        )
        .child("-12s")
        .child("44.1 kHz")
}
