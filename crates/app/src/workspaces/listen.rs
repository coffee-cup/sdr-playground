use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::{ActiveTheme, Sizable};

use crate::app::{Hover, SdrApp};
use crate::components::{frequency, level_meter};
use crate::radio::RadioState;
use crate::signal;
use crate::ui::{dropdown, tokens};

/// Drag payload for scrubbing the tuned marker across the signal display. GPUI's `on_drag` /
/// `on_drag_move` are used (not `on_mouse_move` + `dragging()`, which is gated by hover and stops
/// firing mid-drag). The drag preview is invisible; the marker is the only feedback.
#[derive(Clone)]
struct DragTune;

/// The invisible drag preview attached to a `DragTune` drag.
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

/// The live operating view, laid out with resizable splitters (see `docs/UI.md`): a top row of
/// [center signal display | inspect panel] over a bottom working pane; the center is a tuning
/// header over a spectrum/waterfall stack. The center and inspect show the live signal when a
/// device is running, and a connection state (searching / no device / error + retry) otherwise.
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

/// Center column: the tuning header (frequency selector + level meter) over the live display.
fn center(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let dbfs = app
        .radio()
        .engine()
        .map(|e| e.snapshot().peak_dbfs)
        .unwrap_or(f32::NEG_INFINITY);

    let body = if app.radio().engine().is_some() {
        spectrum_over_waterfall(app, cx).into_any_element()
    } else {
        connection_state(app.radio(), cx)
    };

    div()
        .flex()
        .flex_col()
        .size_full()
        .child(header(app, dbfs, cx))
        .child(div().flex_1().overflow_hidden().child(body))
}

/// The tuning header: the big frequency selector on the left, the signal-level meter on the right.
fn header(app: &SdrApp, dbfs: f32, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px_4()
        .py_2()
        .gap_4()
        .border_b_1()
        .border_color(border)
        .child(frequency::render(app, cx))
        .child(level_meter::render(dbfs, cx))
}

/// Spectrum (with dB axis + frequency scale) over the scrolling waterfall, split by a draggable
/// handle. The red tuned-marker line and channel-bandwidth band overlay the spectrum.
fn spectrum_over_waterfall(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let Some(engine) = app.radio().engine() else {
        return div().into_any_element();
    };
    let snap = engine.snapshot();
    let line = cx.theme().foreground;
    let grid = cx.theme().border.opacity(0.4);
    let axis = cx.theme().muted_foreground;
    let range = app.waterfall().range();
    let bins = app.smoothed_bins().to_vec();
    let fticks = signal::freq_ticks(snap.center_freq, snap.sample_rate);
    let dticks = signal::db_ticks(range);
    let vlines: Vec<f32> = fticks.iter().map(|(f, _)| *f).collect();
    let hlines: Vec<f32> = dticks.iter().map(|(f, _)| *f).collect();
    // The tuned-frequency marker, shown on the spectrum only.
    let mx = app.marker_fraction();

    let spectrum_plot = display_layer(
        false,
        signal::spectrum(bins, range, line, grid, vlines, hlines).into_any_element(),
        marker(mx, app.settings().bandwidth, snap.sample_rate, cx),
        hover_tip(app, &snap, false),
        cx,
    );

    let spectrum_pane = div()
        .flex()
        .flex_col()
        .size_full()
        .child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .overflow_hidden()
                .child(signal::db_axis(&dticks, axis))
                .child(div().flex_1().child(spectrum_plot)),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .child(div().w(tokens::DB_AXIS_WIDTH))
                .child(div().flex_1().child(signal::freq_scale(&fticks, axis))),
        );

    // Inset the waterfall by the same dB-axis gutter as the spectrum plot, so both panes occupy
    // the identical x-range and a given frequency lands at the same column in both.
    let waterfall_pane = div()
        .flex()
        .flex_row()
        .size_full()
        .child(div().w(tokens::DB_AXIS_WIDTH))
        .child(div().flex_1().child(display_layer(
            true,
            signal::waterfall(app.waterfall().image()).into_any_element(),
            div().into_any_element(),
            hover_tip(app, &snap, true),
            cx,
        )));

    v_resizable("sdr.listen.center")
        .child(resizable_panel().size(px(220.)).child(spectrum_pane))
        .child(resizable_panel().child(waterfall_pane))
        .into_any_element()
}

/// A signal-display pane that tracks the cursor: it records its own bounds so a mouse move can be
/// turned into a frequency (and, for the waterfall, an age), and overlays the marker and the hover
/// tooltip. `tip` is precomputed by the caller from the current hover position.
fn display_layer(
    in_waterfall: bool,
    content: AnyElement,
    overlay: AnyElement,
    tip: Option<(f32, f32, Vec<String>)>,
    cx: &mut Context<SdrApp>,
) -> AnyElement {
    let bounds: Rc<Cell<Option<Bounds<Pixels>>>> = Rc::new(Cell::new(None));
    let recorder = bounds.clone();
    let hit = bounds.clone();
    let clicker = bounds.clone();

    let mut layer = div()
        .id(if in_waterfall {
            "wf-layer"
        } else {
            "spec-layer"
        })
        .relative()
        .size_full()
        .overflow_hidden()
        .child(content)
        .child(overlay)
        .child(
            canvas(
                move |b, _, _| recorder.set(Some(b)),
                |_, _, _: &mut Window, _: &mut App| {},
            )
            .absolute()
            .size_full(),
        )
        .on_mouse_move(cx.listener(move |app, ev: &MouseMoveEvent, _window, cx| {
            let Some(b) = hit.get() else { return };
            let w = f32::from(b.size.width).max(1.0);
            let h = f32::from(b.size.height).max(1.0);
            let x = (f32::from(ev.position.x) - f32::from(b.origin.x)) / w;
            let y = (f32::from(ev.position.y) - f32::from(b.origin.y)) / h;
            if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                return;
            }
            app.set_hover(Some(Hover { in_waterfall, x, y }), cx);
        }))
        .on_hover(cx.listener(move |app, hovered: &bool, _window, cx| {
            if !*hovered {
                app.set_hover(None, cx);
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, ev: &MouseDownEvent, _window, cx| {
                let Some(b) = clicker.get() else { return };
                let w = f32::from(b.size.width).max(1.0);
                let x = (f32::from(ev.position.x) - f32::from(b.origin.x)) / w;
                app.tune_to_fraction(x, cx);
            }),
        )
        // Drag-to-scrub: `on_drag` starts the drag (invisible preview); `on_drag_move` fires for
        // every move until release, even outside the pane, so the marker tracks the cursor.
        .on_drag(DragTune, |_, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| DragGhost)
        })
        .on_drag_move(
            cx.listener(move |app, ev: &DragMoveEvent<DragTune>, _window, cx| {
                let b = ev.bounds;
                let w = f32::from(b.size.width).max(1.0);
                let x = (f32::from(ev.event.position.x) - f32::from(b.origin.x)) / w;
                app.tune_to_fraction(x.clamp(0.0, 1.0), cx);
            }),
        );

    if let Some((x, y, lines)) = tip {
        layer = layer.child(tooltip(x, y, lines, cx));
    }
    layer.into_any_element()
}

/// The tuned-frequency marker overlay at horizontal fraction `x`: the red line plus the
/// translucent channel-bandwidth band. Shown on the spectrum only (the waterfall has none).
fn marker(x: f32, bandwidth: u32, sample_rate: u32, cx: &mut Context<SdrApp>) -> AnyElement {
    let center = cx.theme().danger;
    let band = cx.theme().foreground.opacity(0.08);
    let bw = (bandwidth as f32 / sample_rate.max(1) as f32).clamp(0.0, 1.0);

    div()
        .absolute()
        .inset_0()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative((x - bw / 2.0).clamp(0.0, 1.0)))
                .w(relative(bw))
                .bg(band),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(relative(x))
                .w(px(1.))
                .bg(center),
        )
        .into_any_element()
}

/// Build the hover-readout lines for `pane`, if the cursor is currently over it.
fn hover_tip(
    app: &SdrApp,
    snap: &sdr_engine::Snapshot,
    in_waterfall: bool,
) -> Option<(f32, f32, Vec<String>)> {
    let h = app.hover().filter(|h| h.in_waterfall == in_waterfall)?;
    let hz = snap.center_freq as f64 + (h.x as f64 - 0.5) * snap.sample_rate as f64;
    let delta_khz = (hz - snap.center_freq as f64) / 1e3;
    let mut lines = vec![
        format!("{:.4} MHz", hz / 1e6),
        format!("Δ {delta_khz:+.1} kHz"),
    ];
    if in_waterfall {
        if let Some(age) = app.waterfall().row_age(h.y) {
            lines.insert(0, format!("−{:.1} s", age.as_secs_f32()));
        }
    }
    Some((h.x, h.y, lines))
}

/// A small floating readout positioned at the cursor (fractions of the pane).
fn tooltip(x: f32, y: f32, lines: Vec<String>, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let bg = cx.theme().background;
    let fg = cx.theme().foreground;
    let border = cx.theme().border;
    let mono = cx.theme().mono_font_family.clone();

    div()
        .absolute()
        .left(relative(x))
        .top(relative(y))
        .ml(px(8.))
        .mt(px(8.))
        .px(px(6.))
        .py(px(3.))
        .bg(bg)
        .text_color(fg)
        .border_1()
        .border_color(border)
        .rounded(px(3.))
        .text_size(px(10.))
        .font_family(mono)
        .flex()
        .flex_col()
        .children(lines.into_iter().map(|l| div().child(l)))
}

/// Right-hand observability surface: the live waveform, signal readouts, and FFT/display settings.
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
                .overflow_hidden()
                .child(section("Waveform", cx))
                .child(
                    div()
                        .h(px(72.))
                        .border_1()
                        .border_color(border)
                        .child(signal::waveform(engine.clone(), wave)),
                )
                .child(section("Signal", cx))
                .child(kv("Freq", &freq(app.tuned_freq()), cx))
                .child(kv("Rate", &rate(snap.sample_rate), cx))
                .child(kv("Power", &format!("{} dBFS", db(snap.mean_dbfs)), cx))
                .child(kv(
                    "Peak",
                    &peak(&spec.bins_db, spec.center_freq, spec.sample_rate),
                    cx,
                ))
                .child(section("FFT", cx))
                .child(fft_settings(app, cx)),
        );
    }

    panel
}

/// The curated FFT/display settings: size, window, colormap, frame rate, averaging, bandwidth,
/// dB scale. Each picker is a themed dropdown whose state lives in `app.controls()`.
fn fft_settings(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let c = app.controls();
    div()
        .flex()
        .flex_col()
        .gap_2()
        .child(setting_row(
            "FFT Size",
            dropdown(&c.fft).into_any_element(),
            cx,
        ))
        .child(setting_row(
            "Window",
            dropdown(&c.window).into_any_element(),
            cx,
        ))
        .child(setting_row(
            "Colormap",
            dropdown(&c.colormap).into_any_element(),
            cx,
        ))
        .child(setting_row("Rate", dropdown(&c.fps).into_any_element(), cx))
        .child(setting_row(
            "Averaging",
            dropdown(&c.averaging).into_any_element(),
            cx,
        ))
        .child(setting_row(
            "Bandwidth",
            dropdown(&c.bandwidth).into_any_element(),
            cx,
        ))
        .child(db_scale(app, cx))
}

/// The dB-window control: an Auto toggle, and −/+ steppers for min/max when manual.
fn db_scale(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let s = app.settings();
    let auto = s.db_auto;

    let toggle = setting_row(
        "dB Scale",
        Button::new("set-db-auto")
            .outline()
            .small()
            .label(if auto { "Auto" } else { "Manual" })
            .on_click(cx.listener(move |app, _, _, cx| {
                let auto = app.settings().db_auto;
                app.set_db_auto(!auto, cx);
            }))
            .into_any_element(),
        cx,
    );

    let mut col = div().flex().flex_col().gap_1().child(toggle);
    if !auto {
        col = col
            .child(stepper(0, "Min", s.db_min, cx, |a| (a.0, a.1)))
            .child(stepper(1, "Max", s.db_max, cx, |a| (a.2, a.3)));
    }
    col
}

/// A label with −/+ steppers around a dB value. `pick` selects which `(min, max)` of the stepped
/// bounds to apply; the button supplies the step direction.
fn stepper(
    idx: usize,
    label: &'static str,
    value: f32,
    cx: &mut Context<SdrApp>,
    pick: impl Fn(DbBounds) -> (f32, f32) + Copy + 'static,
) -> impl IntoElement {
    let mono = cx.theme().mono_font_family.clone();
    let foreground = cx.theme().foreground;

    setting_row(
        label,
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .child(
                Button::new(("db-minus", idx))
                    .outline()
                    .small()
                    .label("−")
                    .on_click(cx.listener(move |app, _, _, cx| {
                        let (min, max) = pick(DbBounds::step(app, -5.0));
                        app.set_db_range(min, max, cx);
                    })),
            )
            .child(
                div()
                    .w(px(44.))
                    .font_family(mono)
                    .text_color(foreground)
                    .text_xs()
                    .text_center()
                    .child(format!("{value:.0}")),
            )
            .child(
                Button::new(("db-plus", idx))
                    .outline()
                    .small()
                    .label("+")
                    .on_click(cx.listener(move |app, _, _, cx| {
                        let (min, max) = pick(DbBounds::step(app, 5.0));
                        app.set_db_range(min, max, cx);
                    })),
            )
            .into_any_element(),
        cx,
    )
}

/// Candidate `(min−, min+, max−, max+)` dB bounds after stepping one edge by `delta`, clamped so
/// min stays below max. The stepper picks which pair to apply.
struct DbBounds(f32, f32, f32, f32);

impl DbBounds {
    fn step(app: &SdrApp, delta: f32) -> DbBounds {
        let s = app.settings();
        let lo = (s.db_min + delta).clamp(-160.0, s.db_max - 5.0);
        let hi = (s.db_max + delta).clamp(s.db_min + 5.0, 0.0);
        DbBounds(lo, s.db_max, s.db_min, hi)
    }
}

/// A label/control row in the settings panel: muted label left, control right.
fn setting_row(label: &str, control: AnyElement, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .h(px(28.))
        .child(div().text_xs().text_color(muted).child(label.to_string()))
        .child(control)
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

/// Bottom working pane. Tabs (Decoder | Events | Channels) per `docs/UI.md`; the active tab and
/// its content are static for now.
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
