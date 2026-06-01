use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::ActiveTheme;

use crate::app::SdrApp;

/// The live operating view, laid out with resizable splitters (see `docs/UI.md`):
/// a top row of [center signal display | inspect panel] over a bottom working pane;
/// the center itself splits spectrum over waterfall. All regions are static
/// placeholders; nothing reads the radio yet.
pub fn render(cx: &mut Context<SdrApp>) -> impl IntoElement {
    v_resizable("sdr.listen.rows")
        .child(
            resizable_panel().child(
                h_resizable("sdr.listen.cols")
                    .child(resizable_panel().child(center(cx)))
                    .child(resizable_panel().size(px(300.)).child(inspect(cx))),
            ),
        )
        .child(resizable_panel().size(px(190.)).child(bottom_pane(cx)))
}

/// Center signal display: the frequency-domain spectrum over the waterfall.
fn center(cx: &mut Context<SdrApp>) -> impl IntoElement {
    v_resizable("sdr.listen.center")
        .child(
            resizable_panel()
                .size(px(200.))
                .child(surface("Spectrum", cx)),
        )
        .child(resizable_panel().child(surface("Waterfall", cx)))
}

/// A flat panel with a small section header and an empty body where the signal render
/// will live.
fn surface(label: &'static str, cx: &mut Context<SdrApp>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(panel_header(label, cx))
        .child(div().flex_1())
}

/// Right-hand observability surface: the selected stage's readouts plus Tune controls.
fn inspect(cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;

    div()
        .flex()
        .flex_col()
        .size_full()
        .border_l_1()
        .border_color(border)
        .child(panel_header("Inspect", cx))
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .child(kv("Stage", "demod", cx))
                .child(kv("SNR", "18 dB", cx))
                .child(kv("BW", "12 kHz", cx))
                .child(section("Tune", cx))
                .child(kv("Freq", "133.700 MHz", cx))
                .child(kv("Mode", "AM", cx))
                .child(kv("Gain", "28", cx)),
        )
}

/// Bottom working pane. Tabs (Decoder | Events | Channels) per `docs/UI.md`; the active
/// tab and its content are static for now.
fn bottom_pane(cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let muted = cx.theme().muted_foreground;
    let mono = cx.theme().mono_font_family.clone();

    let tabs = ["Decoder", "Events", "Channels"];

    div()
        .flex()
        .flex_col()
        .size_full()
        .border_t_1()
        .border_color(border)
        .child(
            div()
                .flex()
                .flex_row()
                .h(px(28.))
                .items_center()
                .border_b_1()
                .border_color(border)
                .children(
                    tabs.into_iter()
                        .enumerate()
                        .map(|(i, label)| tab(label, i == 0, cx)),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .p_3()
                .font_family(mono)
                .text_xs()
                .text_color(muted)
                .child("AC123      39000 ft   hdg 270        ADS-B")
                .child("\"call ext 4471\"                      POCSAG")
                .child("Acurite 0x3f   21.4 C   58 %RH        433"),
        )
}

/// A bottom-pane tab. Active tab is foreground with a raised fill and an accent underline.
fn tab(label: &'static str, active: bool, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let raised = cx.theme().secondary;
    let accent = cx.theme().primary;

    div()
        .flex()
        .items_center()
        .h_full()
        .px_3()
        .text_xs()
        .border_b_2()
        .border_color(if active {
            accent
        } else {
            gpui::transparent_black()
        })
        .when(active, |this| this.bg(raised))
        .text_color(if active { foreground } else { muted })
        .child(label)
}

/// A small section header used as the top strip of a panel.
fn panel_header(label: &'static str, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let border = cx.theme().border;

    div()
        .flex()
        .items_center()
        .h(px(24.))
        .px_2()
        .border_b_1()
        .border_color(border)
        .text_xs()
        .text_color(muted)
        .child(label)
}

/// An inline section divider/label inside the inspect panel.
fn section(label: &'static str, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;

    div().pt_2().text_xs().text_color(muted).child(label)
}

/// A label/value row: muted key on the left, foreground value on the right.
fn kv(key: &'static str, value: &'static str, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let mono = cx.theme().mono_font_family.clone();

    div()
        .flex()
        .flex_row()
        .justify_between()
        .text_xs()
        .child(div().text_color(muted).child(key))
        .child(div().font_family(mono).text_color(foreground).child(value))
}
