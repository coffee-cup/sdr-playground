use gpui::*;
use gpui_component::ActiveTheme;

use crate::app::SdrApp;

/// The live operating view. Stacks the top status bar over a row of
/// [center signal display | inspect panel], with the tabbed working pane beneath.
/// Arrangement and region responsibilities are defined in `docs/UI.md`. All regions
/// are static placeholders; nothing reads the radio yet.
pub fn render(_window: &mut Window, cx: &mut Context<SdrApp>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(top_bar(cx))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .overflow_hidden()
                .child(center(cx))
                .child(inspect(cx)),
        )
        .child(bottom_pane(cx))
}

/// Read-only status line for the active channel, with the command-palette entry on the right.
fn top_bar(cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .w_full()
        .h(px(40.))
        .px_4()
        .border_b_1()
        .border_color(border)
        .child(
            div()
                .flex()
                .flex_row()
                .gap_4()
                .child(div().font_weight(FontWeight::MEDIUM).child("133.700 MHz"))
                .child(div().text_color(muted).child("AM"))
                .child(div().text_color(muted).child("bw 12k"))
                .child(div().text_color(muted).child("gain 28")),
        )
        .child(div().text_color(muted).child("⌘K"))
}

/// Center signal display: the frequency-domain spectrum stacked over the waterfall.
/// The splitter between them (and the canvas rendering) come later.
fn center(cx: &mut Context<SdrApp>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let panel = cx.theme().muted;

    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_px()
        .p_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .h(px(140.))
                .rounded_md()
                .bg(panel)
                .text_color(muted)
                .child("▁▂▃▅▇ spectrum (FFT) ▇▅▃▂▁"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .flex_1()
                .rounded_md()
                .bg(panel)
                .text_color(muted)
                .child("waterfall"),
        )
}

/// Right-hand observability surface: the selected stage's output plus Tune controls.
fn inspect(cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;

    div()
        .flex()
        .flex_col()
        .gap_3()
        .w(px(240.))
        .h_full()
        .p_3()
        .border_l_1()
        .border_color(border)
        .child(div().font_weight(FontWeight::MEDIUM).child("INSPECT"))
        .child(div().text_sm().text_color(muted).child("stage: demod"))
        .child(div().text_sm().text_color(muted).child("∿ waveform"))
        .child(div().text_sm().text_color(muted).child("SNR    18 dB"))
        .child(div().text_sm().text_color(muted).child("bw     12 kHz"))
        .child(div().mt_2().font_weight(FontWeight::MEDIUM).child("TUNE"))
        .child(div().text_sm().text_color(muted).child("freq · mode"))
        .child(
            div()
                .text_sm()
                .text_color(muted)
                .child("bw · gain · squelch"),
        )
}

/// Bottom working pane. Tabs (Decoder | Events | Channels) per `docs/UI.md`; the active
/// tab and its content are static for now.
fn bottom_pane(cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;

    let tabs = ["Decoder", "Events", "Channels"];

    div()
        .flex()
        .flex_col()
        .h(px(160.))
        .border_t_1()
        .border_color(border)
        .child(
            div()
                .flex()
                .flex_row()
                .gap_4()
                .px_4()
                .h(px(34.))
                .items_center()
                .border_b_1()
                .border_color(border)
                .children(tabs.iter().enumerate().map(|(i, label)| {
                    div()
                        .text_sm()
                        .text_color(if i == 0 { foreground } else { muted })
                        .child(*label)
                })),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_3()
                .text_sm()
                .text_color(muted)
                .child("AC123   39000ft   hdg 270        ADS-B")
                .child("pager: \"call ext 4471\"           POCSAG")
                .child("Acurite 0x3f   21.4°C   58%RH    433"),
        )
}
