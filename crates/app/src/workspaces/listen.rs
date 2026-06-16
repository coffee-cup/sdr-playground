use std::cell::Cell;
use std::rc::Rc;

use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_component::{ActiveTheme, Sizable};

use crate::app::{Hover, SdrApp};
use crate::components::{frequency, level_meter};
use crate::radio::RadioState;
use crate::signal;
use crate::ui::{
    device_header, dropdown, field_row, inset, knob, kv_row, palette, section_label, segmented,
    segmented_meter, tab_strip, tokens, MeterDir,
};

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
    let line = palette(cx).data;
    let grid = palette(cx).line_hi.opacity(0.25);
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
    let center = cx.theme().primary;
    let band = cx.theme().primary.opacity(0.12);
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

/// Right-hand observability surface, styled as a device: a header with a power dot, the live
/// waveform, signal readouts, the FFT/display settings, and a channel-style level meter down the
/// edge.
fn inspect(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let running = app.radio().engine().is_some();

    let mut panel = div()
        .flex()
        .flex_col()
        .size_full()
        .border_l_1()
        .border_color(border)
        .child(device_header("Inspect", Some("demod"), running, cx));

    if let Some(engine) = app.radio().engine() {
        let snap = engine.snapshot();
        let spec = engine.spectrum();
        let wave = palette(cx).data;
        let frac = (snap.peak_dbfs.clamp(-100.0, 0.0) + 100.0) / 100.0;

        let content = div()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .flex_1()
            .overflow_hidden()
            .child(section_label("Waveform", cx))
            .child(
                inset(cx)
                    .h(px(72.))
                    .child(signal::waveform(engine.clone(), wave)),
            )
            .child(section_label("Signal", cx))
            .child(kv_row("Freq", &freq(app.tuned_freq()), cx))
            .child(kv_row("Rate", &rate(snap.sample_rate), cx))
            .child(kv_row("Power", &format!("{} dBFS", db(snap.mean_dbfs)), cx))
            .child(kv_row(
                "Peak",
                &peak(&spec.bins_db, spec.center_freq, spec.sample_rate),
                cx,
            ))
            .child(section_label("FFT", cx))
            .child(fft_settings(app, cx));

        // A mixer-style level meter down the device edge.
        let vmeter = div()
            .flex()
            .w(px(14.))
            .py_2()
            .px(px(2.))
            .border_l_1()
            .border_color(border)
            .child(segmented_meter(frac, 16, MeterDir::Vertical, cx));

        panel = panel.child(
            div()
                .flex()
                .flex_row()
                .flex_1()
                .overflow_hidden()
                .child(content)
                .child(vmeter),
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
        .child(field_row(
            "FFT Size",
            dropdown(&c.fft).into_any_element(),
            cx,
        ))
        .child(field_row(
            "Window",
            dropdown(&c.window).into_any_element(),
            cx,
        ))
        .child(field_row(
            "Colormap",
            dropdown(&c.colormap).into_any_element(),
            cx,
        ))
        .child(field_row("Rate", dropdown(&c.fps).into_any_element(), cx))
        .child(field_row(
            "Bandwidth",
            dropdown(&c.bandwidth).into_any_element(),
            cx,
        ))
        .child(db_scale(app, cx))
        .child(averaging_knob(app, cx))
}

/// The display-smoothing control as an arc knob; scroll over it to adjust (0..0.97).
fn averaging_knob(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let avg = app.settings().averaging;
    let accent = cx.theme().primary;
    div()
        .id("avg-knob")
        .flex()
        .justify_center()
        .pt_2()
        .child(knob(
            "Smoothing",
            avg / 0.97,
            format!("{avg:.2}"),
            accent,
            cx,
        ))
        .on_scroll_wheel(cx.listener(|app, ev: &ScrollWheelEvent, _window, cx| {
            let dy = match ev.delta {
                ScrollDelta::Pixels(p) => f32::from(p.y),
                ScrollDelta::Lines(l) => l.y,
            };
            if dy != 0.0 {
                let step = if dy > 0.0 { 0.05 } else { -0.05 };
                let v = (app.settings().averaging + step).clamp(0.0, 0.97);
                app.set_averaging(v, cx);
            }
        }))
}

/// The dB-window control: an Auto toggle, and −/+ steppers for min/max when manual.
fn db_scale(app: &SdrApp, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let s = app.settings();
    let auto = s.db_auto;

    let toggle = field_row(
        "dB Scale",
        segmented(
            "db-scale",
            &[("Auto", true), ("Manual", false)],
            auto,
            cx,
            |app, v, _, cx| app.set_db_auto(v, cx),
        )
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

    field_row(
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
        .child(tab_strip(&tabs, 0, cx))
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
