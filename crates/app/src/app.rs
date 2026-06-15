use std::sync::Arc;
use std::time::Duration;

use gpui::*;
use gpui_component::ActiveTheme;
use sdr_engine::{Engine, EngineConfig, RtlConfig, RtlSdrSource, SpectrumFrame};

use crate::colormap::Colormap;
use crate::components::{title_bar, transport_bar};
use crate::controls::SettingsControls;
use crate::nav::{self, Workspace};
use crate::radio::RadioState;
use crate::settings::{Settings, WindowChoice};
use crate::signal::Waterfall;
use crate::store::Store;
use crate::workspaces;

/// How often the display polls the engine taps and repaints (~30 Hz, the waterfall row rate).
const TICK_INTERVAL: Duration = Duration::from_millis(33);

/// Settings are written this long after the last change, so a slider drag collapses to one write.
const SAVE_DEBOUNCE: Duration = Duration::from_millis(400);

/// Frequency is edited as a fixed-width decimal. Ten digits cover the RTL-SDR V3's range past
/// its 1.766 GHz ceiling; place `p` is the digit worth `10^p` Hz.
pub const FREQ_DIGITS: u32 = 10;
const MAX_FREQ: u64 = 9_999_999_999;

/// A point the cursor is hovering over the signal display, in fractions of the pane (0..1).
#[derive(Debug, Clone, Copy)]
pub struct Hover {
    pub in_waterfall: bool,
    pub x: f32,
    pub y: f32,
}

/// Root view. Owns the persisted settings, the radio connection, and the live display state, and
/// lays out the persistent frame: the integrated title bar, then a body of [nav rail | active
/// workspace], then the transport bar pinned along the bottom.
///
/// `settings` is the single source of truth for everything the user can change; every mutator
/// updates it, drives the matching live engine/display change, and schedules a debounced save.
/// The frame arrangement is described in `docs/UI.md`.
pub struct SdrApp {
    settings: Settings,
    store: Option<Store>,
    save_task: Option<Task<()>>,
    radio: RadioState,
    paused: bool,
    waterfall: Waterfall,
    /// Drives the ~30 Hz poll/repaint while a device is running; dropped to stop it.
    tick_task: Option<Task<()>>,
    /// Focus for the frequency selector, so it receives typed digits.
    freq_focus: FocusHandle,
    /// The frequency digit (place, `10^p` Hz) the cursor is over, for scroll/type editing.
    hovered_digit: Option<u32>,
    /// Cursor position over the spectrum/waterfall, for the hover readout.
    hover: Option<Hover>,
    /// Display-smoothed spectrum bins (EMA over frames) drawn as the spectrum line, and the seq
    /// of the frame last folded in.
    smoothed: Vec<f32>,
    smoothed_seq: u64,
    /// The settings-panel dropdowns, grouped so they stay out of the top-level field list.
    controls: SettingsControls,
}

impl SdrApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = Store::open();
        let mut settings = store.as_ref().and_then(Store::load).unwrap_or_default();
        // Migrate configs written before the tuned marker existed: start it at the center.
        if settings.tuned_freq == 0 {
            settings.tuned_freq = settings.center_freq;
        }
        let mut waterfall = Waterfall::new();
        waterfall.set_colormap(settings.colormap);
        waterfall.set_range_override(settings.manual_db_range());

        let controls = SettingsControls::new(&settings, window, cx);

        Self {
            settings,
            store,
            save_task: None,
            radio: RadioState::Connecting,
            paused: false,
            waterfall,
            tick_task: None,
            freq_focus: cx.focus_handle(),
            hovered_digit: None,
            hover: None,
            smoothed: Vec::new(),
            smoothed_seq: 0,
            controls,
        }
    }

    pub fn controls(&self) -> &SettingsControls {
        &self.controls
    }

    pub fn active(&self) -> Workspace {
        self.settings.active
    }

    pub fn radio(&self) -> &RadioState {
        &self.radio
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn waterfall(&self) -> &Waterfall {
        &self.waterfall
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn freq_focus(&self) -> &FocusHandle {
        &self.freq_focus
    }

    pub fn hovered_digit(&self) -> Option<u32> {
        self.hovered_digit
    }

    pub fn hover(&self) -> Option<Hover> {
        self.hover
    }

    /// The spectrum line to draw: the smoothed bins (empty until the first frame).
    pub fn smoothed_bins(&self) -> &[f32] {
        &self.smoothed
    }

    /// Fold a new spectrum frame into the display-smoothed bins. `averaging` is the EMA weight on
    /// the existing trace; 0 tracks the latest frame exactly.
    fn update_smoothing(&mut self, frame: &SpectrumFrame) {
        if frame.seq == 0 || frame.seq == self.smoothed_seq {
            return;
        }
        self.smoothed_seq = frame.seq;
        let a = self.settings.averaging;
        if a <= 0.0 || self.smoothed.len() != frame.bins_db.len() {
            self.smoothed = frame.bins_db.to_vec();
        } else {
            for (s, &b) in self.smoothed.iter_mut().zip(frame.bins_db.iter()) {
                *s = a * *s + (1.0 - a) * b;
            }
        }
    }

    pub fn activate(&mut self, workspace: Workspace, cx: &mut Context<Self>) {
        if self.settings.active != workspace {
            self.settings.active = workspace;
            self.mark_dirty(cx);
            cx.notify();
        }
    }

    pub fn toggle_pause(&mut self, cx: &mut Context<Self>) {
        if self.radio.engine().is_some() {
            self.paused = !self.paused;
            cx.notify();
        }
    }

    // --- Tuning -----------------------------------------------------------------------------

    /// The frequency the user is tuned to (the marker) — the only frequency shown.
    pub fn tuned_freq(&self) -> u64 {
        self.settings.tuned_freq
    }

    /// The marker's horizontal position as a fraction of the display (0 = left edge, 1 = right):
    /// where `tuned_freq` falls within the captured window `center ± sample_rate/2`.
    pub fn marker_fraction(&self) -> f32 {
        let Some(engine) = self.radio.engine() else {
            return 0.5;
        };
        let s = engine.snapshot();
        if s.sample_rate == 0 {
            return 0.5;
        }
        let frac =
            (self.settings.tuned_freq as f64 - s.center_freq as f64) / s.sample_rate as f64 + 0.5;
        frac.clamp(0.0, 1.0) as f32
    }

    /// The device's tunable range, clamped to the editable digit ceiling; the full editable
    /// range when no device is connected.
    fn device_range(&self) -> (u64, u64) {
        match self.radio.engine() {
            Some(engine) => {
                let s = engine.snapshot();
                (s.tune_min, s.tune_max.min(MAX_FREQ))
            }
            None => (0, MAX_FREQ),
        }
    }

    /// Offset the tuned marker to `hz`, clamped to the captured window so the picture never moves.
    /// Used by click- and drag-to-tune. The device is not retuned; only `tune_center` does that.
    pub fn set_tuned(&mut self, hz: u64, cx: &mut Context<Self>) {
        let Some(engine) = self.radio.engine() else {
            return;
        };
        let s = engine.snapshot();
        let half = s.sample_rate as u64 / 2;
        let hz = hz.clamp(
            s.center_freq.saturating_sub(half),
            s.center_freq.saturating_add(half),
        );
        if hz == self.settings.tuned_freq {
            return;
        }
        self.settings.tuned_freq = hz;
        self.mark_dirty(cx);
        cx.notify();
    }

    /// Tune to the frequency at horizontal fraction `x` (0 = left edge, 1 = right edge) of the
    /// signal display, i.e. `center ± sample_rate/2`. Used by click- and drag-to-tune; `x` is
    /// within the window, so this moves the marker without re-tuning (the picture stays put).
    pub fn tune_to_fraction(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some(engine) = self.radio.engine() else {
            return;
        };
        let s = engine.snapshot();
        let hz = (s.center_freq as f64 + (x.clamp(0.0, 1.0) as f64 - 0.5) * s.sample_rate as f64)
            .max(0.0);
        self.set_tuned(hz as u64, cx);
    }

    /// A full hardware retune to `hz`: recenters the captured window on `hz` with the marker at
    /// center. Used by the frequency selector (absolute entry). Dragging the display is the only
    /// thing that offsets the marker from center.
    pub fn tune_center(&mut self, hz: u64, cx: &mut Context<Self>) {
        let (lo, hi) = self.device_range();
        let hz = hz.clamp(lo, hi);
        if hz == self.settings.center_freq && hz == self.settings.tuned_freq {
            return;
        }
        self.settings.center_freq = hz;
        self.settings.tuned_freq = hz;
        if let Some(engine) = self.radio.engine() {
            engine.tune(hz);
        }
        self.mark_dirty(cx);
        cx.notify();
    }

    /// Set which frequency digit the cursor is over (place = `10^p` Hz), or clear it.
    pub fn set_hovered_digit(&mut self, place: Option<u32>, cx: &mut Context<Self>) {
        if self.hovered_digit != place {
            self.hovered_digit = place;
            cx.notify();
        }
    }

    /// Nudge one decimal place of the frequency by `steps` (scroll), carrying across places. The
    /// selector is absolute tuning, so this fully retunes the device (see `tune_center`).
    pub fn nudge_digit(&mut self, place: u32, steps: i64, cx: &mut Context<Self>) {
        let delta = steps.saturating_mul(10i64.pow(place));
        let hz = (self.settings.tuned_freq as i64 + delta).clamp(0, MAX_FREQ as i64) as u64;
        self.tune_center(hz, cx);
    }

    /// Set the hovered decimal place to digit `d`, then advance to the next place to the right,
    /// so typing successive digits walks down the display (gqrx-style entry).
    pub fn type_digit(&mut self, d: u32, cx: &mut Context<Self>) {
        let Some(place) = self.hovered_digit else {
            return;
        };
        let scale = 10u64.pow(place);
        let current = (self.settings.tuned_freq / scale) % 10;
        let hz = self.settings.tuned_freq - current * scale + (d as u64) * scale;
        self.tune_center(hz, cx);
        if place > 0 {
            self.hovered_digit = Some(place - 1);
        }
    }

    // --- FFT / display settings -------------------------------------------------------------

    pub fn set_fft_size(&mut self, n: usize, cx: &mut Context<Self>) {
        if self.settings.fft_size != n {
            self.settings.fft_size = n;
            self.apply_spectrum(cx);
        }
    }

    pub fn set_window(&mut self, window: WindowChoice, cx: &mut Context<Self>) {
        if self.settings.window != window {
            self.settings.window = window;
            self.apply_spectrum(cx);
        }
    }

    fn apply_spectrum(&mut self, cx: &mut Context<Self>) {
        if let Some(engine) = self.radio.engine() {
            engine.set_spectrum(self.settings.spectrum_config());
        }
        self.mark_dirty(cx);
        cx.notify();
    }

    pub fn set_fps(&mut self, fps: f32, cx: &mut Context<Self>) {
        if self.settings.fps != fps {
            self.settings.fps = fps;
            if let Some(engine) = self.radio.engine() {
                engine.set_fps(fps);
            }
            self.mark_dirty(cx);
            cx.notify();
        }
    }

    pub fn set_colormap(&mut self, colormap: Colormap, cx: &mut Context<Self>) {
        if self.settings.colormap != colormap {
            self.settings.colormap = colormap;
            self.waterfall.set_colormap(colormap);
            self.mark_dirty(cx);
            cx.notify();
        }
    }

    pub fn set_averaging(&mut self, averaging: f32, cx: &mut Context<Self>) {
        let averaging = averaging.clamp(0.0, 0.97);
        if self.settings.averaging != averaging {
            self.settings.averaging = averaging;
            self.mark_dirty(cx);
            cx.notify();
        }
    }

    pub fn set_db_auto(&mut self, auto: bool, cx: &mut Context<Self>) {
        if self.settings.db_auto != auto {
            self.settings.db_auto = auto;
            self.waterfall
                .set_range_override(self.settings.manual_db_range());
            self.mark_dirty(cx);
            cx.notify();
        }
    }

    pub fn set_db_range(&mut self, min: f32, max: f32, cx: &mut Context<Self>) {
        self.settings.db_min = min;
        self.settings.db_max = max;
        self.waterfall
            .set_range_override(self.settings.manual_db_range());
        self.mark_dirty(cx);
        cx.notify();
    }

    pub fn set_bandwidth(&mut self, hz: u32, cx: &mut Context<Self>) {
        if self.settings.bandwidth != hz {
            self.settings.bandwidth = hz;
            self.mark_dirty(cx);
            cx.notify();
        }
    }

    pub fn set_hover(&mut self, hover: Option<Hover>, cx: &mut Context<Self>) {
        self.hover = hover;
        cx.notify();
    }

    // --- Persistence ------------------------------------------------------------------------

    /// Schedule a debounced write of the current settings. Coalesces bursts (slider drags) into a
    /// single save and works whether or not a device is connected.
    fn mark_dirty(&mut self, cx: &mut Context<Self>) {
        if self.save_task.is_some() {
            return;
        }
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            this.update(cx, |app, _| {
                if let Some(store) = &app.store {
                    store.save(&app.settings);
                }
                app.save_task = None;
            })
            .ok();
        }));
    }

    // --- Device lifecycle -------------------------------------------------------------------

    /// Open the first available RTL-SDR and start the engine from the persisted settings.
    /// Enumeration and the blocking USB `open` run off the foreground; the constructed engine is
    /// handed back to the view. Safe to call again to retry; it resets the display and replaces
    /// any running engine.
    pub fn connect(&mut self, cx: &mut Context<Self>) {
        self.radio = RadioState::Connecting;
        self.paused = false;
        self.waterfall = Waterfall::new();
        self.waterfall.set_colormap(self.settings.colormap);
        self.waterfall
            .set_range_override(self.settings.manual_db_range());
        self.tick_task = None;
        cx.notify();

        let rtl = RtlConfig {
            freq_hz: self.settings.center_freq,
            ..RtlConfig::default()
        };
        let engine_config = EngineConfig {
            spectrum: self.settings.spectrum_config(),
            ..EngineConfig::default()
        };
        let fps = self.settings.fps;

        let open = cx.background_executor().spawn(async move {
            let devices = RtlSdrSource::list().map_err(|e| e.to_string())?;
            let Some(device) = devices.into_iter().next() else {
                return Ok::<Option<Engine>, String>(None);
            };
            let source = RtlSdrSource::open(device.index, rtl).map_err(|e| e.to_string())?;
            let engine = Engine::start(Box::new(source), engine_config);
            engine.set_fps(fps);
            Ok(Some(engine))
        });

        cx.spawn(async move |this, cx| {
            let result = open.await;
            this.update(cx, |app, cx| {
                match result {
                    Ok(Some(engine)) => {
                        app.radio = RadioState::Running(Arc::new(engine));
                        app.start_tick_loop(cx);
                    }
                    Ok(None) => app.radio = RadioState::NoDevice,
                    Err(e) => app.radio = RadioState::Failed(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn start_tick_loop(&mut self, cx: &mut Context<Self>) {
        self.tick_task = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(TICK_INTERVAL).await;
            let alive = this
                .update_in(cx, |app, window, cx| {
                    let Some(engine) = app.radio.engine().cloned() else {
                        return;
                    };
                    if app.paused {
                        return;
                    }
                    // Fold the newest spectrum into the waterfall and the smoothed line; release
                    // the old GPU tile.
                    let frame = engine.spectrum();
                    if let Some(old) = app.waterfall.push(&frame) {
                        let _ = window.drop_image(old);
                    }
                    app.update_smoothing(&frame);
                    cx.notify();
                })
                .is_ok();
            if !alive {
                break;
            }
        }));
    }
}

impl Render for SdrApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;

        let content = match self.settings.active {
            Workspace::Listen => workspaces::listen::render(self, cx).into_any_element(),
            Workspace::Library => workspaces::library::render(cx).into_any_element(),
            Workspace::Recordings => workspaces::recordings::render(cx).into_any_element(),
            Workspace::Settings => workspaces::settings::render(cx).into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .text_size(px(13.))
            .bg(background)
            .text_color(foreground)
            .child(title_bar::render(self, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(nav::render(self.settings.active, cx))
                    .child(div().flex().flex_1().overflow_hidden().child(content)),
            )
            .child(transport_bar::render(self, cx))
    }
}
