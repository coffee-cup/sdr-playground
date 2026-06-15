//! Live signal rendering: the spectrum line, the scrolling waterfall, and the time-domain
//! waveform. The waterfall is the only stateful piece — it retains a ring of recent rows that
//! it reassembles into a texture each frame (see `docs/UI.md`, "primary signal display").
//!
//! dB scaling is shared between the spectrum and the waterfall and auto-adapts: the floor
//! tracks a slow average of the per-frame median (≈ the noise floor), so the display stays
//! readable across gain settings without a manual min/max control.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, point, px, relative, Bounds, Corners, Hsla, IntoElement, ParentElement, Path,
    PathBuilder, Pixels, RenderImage, Styled,
};
use image::{Frame, ImageBuffer, Rgba};
use sdr_engine::{Engine, SpectrumFrame};

use crate::colormap::Colormap;
use crate::ui::tokens;

/// Waterfall texture width (frequency bins) and height (history rows). The texture is scaled
/// to the panel; sized near typical panel pixels so the image is not upscaled (which blurs both
/// axes and is what made the waterfall look low-detail next to gqrx). At 30 Hz, 512 rows is
/// ~17 s of history and fills in that long on launch. 2048x512x4 ≈ 4 MB.
const WF_COLS: usize = 2048;
const WF_ROWS: usize = 512;
/// dB window the display spans, above the tracked floor.
const SPAN: f32 = 80.0;
/// How far below the median the floor sits, so noise renders dark rather than mid-bright.
const FLOOR_MARGIN: f32 = 12.0;
/// Smoothing for the floor estimate (per frame).
const FLOOR_ALPHA: f32 = 0.1;

/// The scrolling waterfall's retained state. Rows are kept newest-first and reassembled into a
/// contiguous BGRA texture on each new spectrum frame.
pub struct Waterfall {
    rows: VecDeque<Box<[u8]>>,
    /// Wall-clock instant each row was pushed, aligned with `rows`, for the hover time readout.
    times: VecDeque<Instant>,
    scratch: Vec<u8>,
    image: Option<Arc<RenderImage>>,
    floor: Option<f32>,
    last_seq: u64,
    colormap: Colormap,
    /// When set, the dB window is fixed to this `(min, max)` instead of auto-tracking the floor.
    range_override: Option<(f32, f32)>,
}

impl Waterfall {
    pub fn new() -> Self {
        Self {
            rows: VecDeque::with_capacity(WF_ROWS),
            times: VecDeque::with_capacity(WF_ROWS),
            scratch: vec![0u8; WF_COLS * WF_ROWS * 4],
            image: None,
            floor: None,
            last_seq: 0,
            colormap: Colormap::default(),
            range_override: None,
        }
    }

    /// How long ago the waterfall row at vertical fraction `y` (0 = top/newest) was captured.
    pub fn row_age(&self, y: f32) -> Option<Duration> {
        let row = ((y.clamp(0.0, 1.0) * WF_ROWS as f32) as usize).min(WF_ROWS - 1);
        self.times.get(row).map(|t| t.elapsed())
    }

    pub fn set_colormap(&mut self, colormap: Colormap) {
        self.colormap = colormap;
    }

    /// Fix the dB window, or pass `None` to resume auto-tracking the noise floor.
    pub fn set_range_override(&mut self, range: Option<(f32, f32)>) {
        self.range_override = range;
    }

    /// The shared dB window `(floor, ceil)` for the spectrum and colormap.
    pub fn range(&self) -> (f32, f32) {
        if let Some(range) = self.range_override {
            return range;
        }
        let floor = self.floor.unwrap_or(-90.0) - FLOOR_MARGIN;
        (floor, floor + SPAN)
    }

    pub fn image(&self) -> Option<Arc<RenderImage>> {
        self.image.clone()
    }

    /// Fold one spectrum frame into the waterfall. Returns the previous texture (if replaced)
    /// so the caller can release its GPU tile via `Window::drop_image`. A no-op if the frame
    /// hasn't advanced.
    pub fn push(&mut self, frame: &SpectrumFrame) -> Option<Arc<RenderImage>> {
        if frame.seq == 0 || frame.seq == self.last_seq {
            return None;
        }
        self.last_seq = frame.seq;

        let median = median(&frame.bins_db)?;
        self.floor = Some(match self.floor {
            Some(f) => f * (1.0 - FLOOR_ALPHA) + median * FLOOR_ALPHA,
            None => median,
        });
        let (floor, ceil) = self.range();

        self.rows
            .push_front(make_row(&frame.bins_db, floor, ceil, self.colormap.lut()));
        self.times.push_front(Instant::now());
        while self.rows.len() > WF_ROWS {
            self.rows.pop_back();
            self.times.pop_back();
        }

        let row_bytes = WF_COLS * 4;
        for (i, row) in self.rows.iter().enumerate() {
            self.scratch[i * row_bytes..(i + 1) * row_bytes].copy_from_slice(row);
        }
        // Clear rows not yet filled during the initial ramp-up.
        for b in &mut self.scratch[self.rows.len() * row_bytes..] {
            *b = 0;
        }

        let buffer = ImageBuffer::<Rgba<u8>, _>::from_raw(
            WF_COLS as u32,
            WF_ROWS as u32,
            self.scratch.clone(),
        )
        .expect("waterfall buffer dimensions");
        let image = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));
        self.image.replace(image)
    }
}

/// The spectrum line over a dB/frequency grid, painted from the given (display-smoothed) bins.
/// `vlines`/`hlines` are grid-line positions as fractions across/down the plot, so they line up
/// exactly with the axis labels (which are placed at the same fractions).
pub fn spectrum(
    bins: Vec<f32>,
    range: (f32, f32),
    line: Hsla,
    grid: Hsla,
    vlines: Vec<f32>,
    hlines: Vec<f32>,
) -> impl IntoElement {
    canvas(
        move |bounds, _window, _cx| bounds,
        move |_bounds, bounds, window, _cx| {
            paint_grid(window, bounds, &vlines, &hlines, grid);
            if bins.len() >= 2 {
                if let Some(path) = spectrum_path(&bins, bounds, range) {
                    window.paint_path(path, line);
                }
            }
        },
    )
    .size_full()
}

/// Left gutter of dB labels, each centered on its gridline.
pub fn db_axis(ticks: &[(f32, String)], color: Hsla) -> impl IntoElement {
    div()
        .relative()
        .h_full()
        .w(tokens::DB_AXIS_WIDTH)
        .text_size(tokens::TEXT_AXIS)
        .text_color(color)
        .children(ticks.iter().map(|(f, label)| {
            div()
                .absolute()
                .top(relative(*f))
                .left_0()
                .right_0()
                .h(px(14.))
                .mt(px(-7.))
                .flex()
                .items_center()
                .justify_end()
                .pr(px(5.))
                .child(label.clone())
        }))
}

/// Horizontal frequency scale, each label centered on its gridline.
pub fn freq_scale(ticks: &[(f32, String)], color: Hsla) -> impl IntoElement {
    div()
        .relative()
        .w_full()
        .h(px(16.))
        .text_size(tokens::TEXT_AXIS)
        .text_color(color)
        .children(ticks.iter().map(|(f, label)| {
            div()
                .absolute()
                .left(relative(*f))
                .top_0()
                .w(px(80.))
                .ml(px(-40.))
                .flex()
                .justify_center()
                .child(label.clone())
        }))
}

/// Frequency-axis ticks across `center_freq ± sample_rate/2`, at round MHz/kHz steps. Returns
/// `(fraction across the plot, label)`.
pub fn freq_ticks(center_freq: u64, sample_rate: u32) -> Vec<(f32, String)> {
    let span = sample_rate as f64;
    if span <= 0.0 {
        return Vec::new();
    }
    let lo = center_freq as f64 - span / 2.0;
    let step = nice_step(span, 8.0);
    let mut out = Vec::new();
    let mut t = (lo / step).ceil() * step;
    while t <= lo + span + 1.0 {
        let f = ((t - lo) / span) as f32;
        if (0.0..=1.0).contains(&f) {
            out.push((f, fmt_mhz(t)));
        }
        t += step;
    }
    out
}

/// dB-axis ticks across `(floor, ceil)` at round steps. Returns `(fraction down from the top,
/// label)`.
pub fn db_ticks(range: (f32, f32)) -> Vec<(f32, String)> {
    let (floor, ceil) = range;
    let span = (ceil - floor) as f64;
    if span <= 0.0 {
        return Vec::new();
    }
    let step = nice_step(span, 6.0).max(1.0);
    let mut out = Vec::new();
    let mut v = (floor as f64 / step).ceil() * step;
    while v <= ceil as f64 + 0.001 {
        let f = ((ceil as f64 - v) / span) as f32;
        if (0.0..=1.0).contains(&f) {
            out.push((f, format!("{v:.0}")));
        }
        v += step;
    }
    out
}

/// Round a raw axis interval to a 1/2/5×10ⁿ step so labels land on readable values.
fn nice_step(span: f64, target: f64) -> f64 {
    if span <= 0.0 {
        return 1.0;
    }
    let raw = span / target;
    let mag = 10f64.powf(raw.log10().floor());
    let norm = raw / mag;
    let nice = if norm < 1.5 {
        1.0
    } else if norm < 3.0 {
        2.0
    } else if norm < 7.0 {
        5.0
    } else {
        10.0
    };
    nice * mag
}

fn fmt_mhz(hz: f64) -> String {
    let s = format!("{:.3}", hz / 1e6);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The waterfall texture, painted to fill the panel exactly so it lines up with the spectrum
/// above it. Empty until the first frame. Painted via `paint_image` rather than an `img` element
/// because `img` imposes the texture's aspect ratio on the layout, which letterboxes the panel
/// and shifts the frequency axis out of alignment with the spectrum.
pub fn waterfall(image: Option<Arc<RenderImage>>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            if let Some(image) = image {
                let _ = window.paint_image(bounds, Corners::default(), image, 0, false);
            }
        },
    )
    .size_full()
}

/// The time-domain waveform: the real part of recent IQ across the width.
pub fn waveform(engine: Arc<Engine>, line: Hsla) -> impl IntoElement {
    canvas(
        move |bounds, _window, _cx| (engine.waveform(), bounds),
        move |_bounds, (frame, bounds), window, _cx| {
            if !frame.samples.is_empty() {
                if let Some(path) = waveform_path(&frame.samples, bounds) {
                    window.paint_path(path, line);
                }
            }
        },
    )
    .size_full()
}

/// Median of the finite bins — a robust noise-floor estimate.
fn median(bins: &[f32]) -> Option<f32> {
    let mut finite: Vec<f32> = bins.iter().copied().filter(|d| d.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    finite.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    Some(finite[finite.len() / 2])
}

/// Colormap `bins` into one BGRA row: each of `WF_COLS` columns is the peak dB over its bins,
/// normalized across `[floor, ceil]`.
fn make_row(bins: &[f32], floor: f32, ceil: f32, lut: &[[u8; 4]; 256]) -> Box<[u8]> {
    let span = (ceil - floor).max(1.0);
    let mut row = vec![0u8; WF_COLS * 4];
    for c in 0..WF_COLS {
        let t = ((column_db(bins, c, WF_COLS) - floor) / span).clamp(0.0, 1.0);
        let idx = ((t * 255.0) as usize).min(255);
        row[c * 4..c * 4 + 4].copy_from_slice(&lut[idx]);
    }
    row.into_boxed_slice()
}

fn spectrum_path(bins: &[f32], bounds: Bounds<Pixels>, range: (f32, f32)) -> Option<Path<Pixels>> {
    let (floor, ceil) = range;
    let span = (ceil - floor).max(1.0);
    let (x0, y0, w, h) = xywh(bounds);
    let cols = (w as usize).max(2);

    let mut builder = PathBuilder::stroke(px(1.0));
    for c in 0..cols {
        let t = ((column_db(bins, c, cols) - floor) / span).clamp(0.0, 1.0);
        let x = px(x0 + w * c as f32 / cols as f32);
        let y = px(y0 + h * (1.0 - t));
        if c == 0 {
            builder.move_to(point(x, y));
        } else {
            builder.line_to(point(x, y));
        }
    }
    builder.build().ok()
}

/// Grid lines at the given fractional positions (`hlines` horizontal, `vlines` vertical), so the
/// grid aligns with the axis labels.
fn paint_grid(
    window: &mut gpui::Window,
    bounds: Bounds<Pixels>,
    vlines: &[f32],
    hlines: &[f32],
    color: Hsla,
) {
    let (x0, y0, w, h) = xywh(bounds);
    for &f in hlines {
        let y = px(y0 + h * f);
        let mut builder = PathBuilder::stroke(px(1.0));
        builder.move_to(point(px(x0), y));
        builder.line_to(point(px(x0 + w), y));
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
    for &f in vlines {
        let x = px(x0 + w * f);
        let mut builder = PathBuilder::stroke(px(1.0));
        builder.move_to(point(x, px(y0)));
        builder.line_to(point(x, px(y0 + h)));
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

fn waveform_path(samples: &[sdr_engine::Iq], bounds: Bounds<Pixels>) -> Option<Path<Pixels>> {
    let n = samples.len();
    if n < 2 {
        return None;
    }
    let (x0, y0, w, h) = xywh(bounds);
    let mut builder = PathBuilder::stroke(px(1.0));
    for (i, s) in samples.iter().enumerate() {
        // Real part in [-1, 1] → top..bottom.
        let t = (s.re.clamp(-1.0, 1.0) + 1.0) / 2.0;
        let x = px(x0 + w * i as f32 / (n - 1) as f32);
        let y = px(y0 + h * (1.0 - t));
        if i == 0 {
            builder.move_to(point(x, y));
        } else {
            builder.line_to(point(x, y));
        }
    }
    builder.build().ok()
}

/// Peak dB in column `c` of `cols`, max-pooled across the bins it spans so signals survive
/// downsampling.
fn column_db(bins: &[f32], c: usize, cols: usize) -> f32 {
    let n = bins.len();
    let start = c * n / cols;
    let end = ((c + 1) * n / cols).clamp(start + 1, n);
    bins[start..end]
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max)
}

/// `bounds` as `(x, y, width, height)` in plain f32 for path arithmetic.
fn xywh(bounds: Bounds<Pixels>) -> (f32, f32, f32, f32) {
    (
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(seq: u64) -> SpectrumFrame {
        // A noise floor at -80 dB with one strong bin at the center.
        let bins: Vec<f32> = (0..4096)
            .map(|i| if i == 2048 { -10.0 } else { -80.0 })
            .collect();
        SpectrumFrame {
            bins_db: bins.into_boxed_slice(),
            fft_size: 4096,
            center_freq: 100_000_000,
            sample_rate: 2_048_000,
            seq,
        }
    }

    #[test]
    fn push_builds_texture_and_tracks_floor() {
        let mut wf = Waterfall::new();
        assert!(wf.image().is_none());

        // First frame builds a texture (no prior to drop).
        assert!(wf.push(&frame(1)).is_none());
        assert!(wf.image().is_some());

        // Floor tracks the median (~ -80), and the window is SPAN wide.
        let (floor, ceil) = wf.range();
        assert!(
            (floor - (-80.0 - FLOOR_MARGIN)).abs() < 1.0,
            "floor = {floor}"
        );
        assert!((ceil - floor - SPAN).abs() < 0.01);

        // A new seq replaces the texture, returning the old one to drop.
        assert!(wf.push(&frame(2)).is_some());
        // A repeated seq is a no-op.
        assert!(wf.push(&frame(2)).is_none());
    }

    #[test]
    fn colormaps_run_dark_to_bright() {
        let lum = |c: [u8; 4]| c[0] as u32 + c[1] as u32 + c[2] as u32;
        for cm in Colormap::ALL {
            let lut = cm.lut();
            assert!(
                lum(lut[0]) < lum(lut[255]),
                "{}: floor should be darker than peak",
                cm.label()
            );
        }
    }
}
