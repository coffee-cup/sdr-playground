use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::ActiveTheme;

use crate::app::SdrApp;
use crate::radio::RadioState;
use crate::signal;

/// The live operating view, laid out with resizable splitters (see `docs/UI.md`): a top row of
/// [center signal display | inspect panel] over a bottom working pane; the center itself splits
/// spectrum over waterfall. The center and inspect show the live signal when a device is
/// running, and a connection state (searching / no device / error + retry) otherwise.
pub fn render(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    v_resizable("sdr.listen.rows")
        .child(
            resizable_panel().child(
                h_resizable("sdr.listen.cols")
                    .child(resizable_panel().child(center(app, cx)))
                    .child(resizable_panel().size(px(300.)).child(inspect(app, cx))),
            ),
        )
        .child(resizable_panel().size(px(190.)).child(bottom_pane(cx)))
}

/// Center signal display: spectrum over waterfall when running, else a connection state.
fn center(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let Some(engine) = app.radio().engine() else {
        return surface("Spectrum", connection_state(app.radio(), cx), cx).into_any_element();
    };

    let line = cx.theme().foreground;
    let grid = cx.theme().border.opacity(0.4);
    let range = app.waterfall().range();

    v_resizable("sdr.listen.center")
        .child(resizable_panel().size(px(200.)).child(surface(
            "Spectrum",
            signal::spectrum(engine.clone(), range, line, grid).into_any_element(),
            cx,
        )))
        .child(resizable_panel().child(surface(
            "Waterfall",
            signal::waterfall(app.waterfall().image()).into_any_element(),
            cx,
        )))
        .into_any_element()
}

/// Right-hand observability surface: the live waveform and readouts, plus the Tune section.
fn inspect(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let mut panel = div()
        .flex()
        .flex_col()
        .size_full()
        .border_l_1()
        .border_color(border)
        .child(panel_header("Inspect", cx));

    if let Some(engine) = app.radio().engine() {
        let snap = engine.snapshot();
        let spec = engine.spectrum();
        let wave = cx.theme().chart_2;

        panel = panel.child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .child(section("Waveform", cx))
                .child(
                    div()
                        .h(px(72.))
                        .border_1()
                        .border_color(border)
                        .child(signal::waveform(engine.clone(), wave)),
                )
                .child(section("Signal", cx))
                .child(kv("Freq", &freq(snap.center_freq), cx))
                .child(kv("Rate", &rate(snap.sample_rate), cx))
                .child(kv("Power", &format!("{} dBFS", db(snap.mean_dbfs)), cx))
                .child(kv(
                    "Peak",
                    &peak(&spec.bins_db, spec.center_freq, spec.sample_rate),
                    cx,
                )),
        );
    }

    panel
}

/// A centered connection state: searching, no device, or an error with a Retry button.
fn connection_state(radio: &RadioState, cx: &mut Context<SdrApp>) -> AnyElement {
    let muted = cx.theme().muted_foreground;

    let body = match radio {
        RadioState::Connecting => div()
            .text_color(muted)
            .child("Searching for device…")
            .into_any_element(),
        RadioState::NoDevice => {
            state_message("No RTL-SDR found", "Connect a device and retry.", cx)
        }
        RadioState::Failed(err) => state_message("Connection failed", err, cx),
        RadioState::Running(_) => div().into_any_element(),
    };

    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_4()
        .size_full()
        .child(body)
        .into_any_element()
}

fn state_message(title: &str, detail: &str, cx: &mut Context<SdrApp>) -> AnyElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .child(div().text_color(foreground).child(title.to_string()))
        .child(div().text_xs().text_color(muted).child(detail.to_string()))
        .child(
            Button::new("retry")
                .primary()
                .label("Retry")
                .on_click(cx.listener(|app, _, _, cx| app.connect(cx))),
        )
        .into_any_element()
}

/// Bottom working pane. Tabs (Decoder | Events | Channels) per `docs/UI.md`; the active tab
/// and its content are static for now.
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

/// A flat panel: a small section header over a body that fills the remaining space.
fn surface(label: &'static str, body: AnyElement, cx: &mut Context<SdrApp>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .size_full()
        .child(panel_header(label, cx))
        .child(div().flex_1().overflow_hidden().child(body))
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

/// A label/value row: muted key on the left, foreground mono value on the right.
fn kv(key: &'static str, value: &str, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let mono = cx.theme().mono_font_family.clone();

    div()
        .flex()
        .flex_row()
        .justify_between()
        .text_xs()
        .child(div().text_color(muted).child(key))
        .child(
            div()
                .font_family(mono)
                .text_color(foreground)
                .child(value.to_string()),
        )
}

fn freq(hz: u64) -> String {
    format!("{:.3} MHz", hz as f64 / 1e6)
}

fn rate(sps: u32) -> String {
    format!("{:.3} MS/s", sps as f64 / 1e6)
}

fn db(v: f32) -> String {
    if v.is_finite() {
        format!("{v:.1}")
    } else {
        "-inf".to_string()
    }
}

/// The strongest spectral bin as an absolute frequency and dB.
fn peak(bins: &[f32], center_freq: u64, sample_rate: u32) -> String {
    let n = bins.len();
    let Some((bin, &peak_db)) = bins
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
    else {
        return "—".to_string();
    };
    let offset = (bin as i64 - n as i64 / 2) * sample_rate as i64 / n as i64;
    let abs = (center_freq as i64 + offset).max(0) as u64;
    format!("{} ({} dBFS)", freq(abs), db(peak_db))
}
