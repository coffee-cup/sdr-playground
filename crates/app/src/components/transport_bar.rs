use gpui::*;
use gpui_component::ActiveTheme;

use crate::app::SdrApp;

/// Full-width strip for record and replay, present in every workspace so recording is
/// always one action away: transport controls, the DVR buffer scrubber, buffer depth,
/// record toggle, and sample rate (see `docs/UI.md`). Static placeholder for now.
pub fn render(cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_4()
        .w_full()
        .h(px(44.))
        .px_4()
        .border_t_1()
        .border_color(border)
        .text_color(muted)
        .text_sm()
        .child("◀◀  ▮▮  ▶▶")
        .child(div().flex_1().child("[════ DVR buffer ════○─────────]"))
        .child("-12s")
        .child("● REC")
        .child("44.1k")
}
