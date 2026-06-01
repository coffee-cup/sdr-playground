//! Live signal rendering: the spectrum line, the scrolling waterfall, and the time-domain
//! waveform. The waterfall is the only stateful piece — it retains a ring of recent rows that
//! it reassembles into a texture each frame (see `docs/UI.md`, "primary signal display").
//!
//! dB scaling is shared between the spectrum and the waterfall and auto-adapts: the floor
//! tracks a slow average of the per-frame median (≈ the noise floor), so the display stays
//! readable across gain settings without a manual min/max control.

use std::collections::VecDeque;
use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    canvas, div, img, point, px, Bounds, Hsla, ImageSource, IntoElement, ObjectFit, ParentElement,
    Path, PathBuilder, Pixels, RenderImage, Styled, StyledImage,
};
use image::{Frame, ImageBuffer, Rgba};
use sdr_engine::{Engine, SpectrumFrame};

use crate::colormap::Colormap;

/// Waterfall texture width (frequency bins) and height (history rows). The texture is scaled
/// to the panel, so these are fixed regardless of layout size.
const WF_COLS: usize = 1024;
const WF_ROWS: usize = 320;
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
    scratch: Vec<u8>,
    image: Option<Arc<RenderImage>>,
    floor: Option<f32>,
    last_seq: u64,
    colormap: Colormap,
}

impl Waterfall {
    pub fn new() -> Self {
        Self {
            rows: VecDeque::with_capacity(WF_ROWS),
            scratch: vec![0u8; WF_COLS * WF_ROWS * 4],
            image: None,
            floor: None,
            last_seq: 0,
            colormap: Colormap::default(),
        }
    }

    /// The shared dB window `(floor, ceil)` for the spectrum and colormap.
    pub fn range(&self) -> (f32, f32) {
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
        while self.rows.len() > WF_ROWS {
            self.rows.pop_back();
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

/// The spectrum line over a dB grid, painted from the latest frame.
pub fn spectrum(
    engine: Arc<Engine>,
    range: (f32, f32),
    line: Hsla,
    grid: Hsla,
) -> impl IntoElement {
    canvas(
        move |bounds, _window, _cx| (engine.spectrum(), bounds),
        move |_bounds, (frame, bounds), window, _cx| {
            paint_grid(window, bounds, grid);
            if frame.seq > 0 {
                if let Some(path) = spectrum_path(&frame.bins_db, bounds, range) {
                    window.paint_path(path, line);
                }
            }
        },
    )
    .size_full()
}

/// The waterfall texture, scaled to fill the panel. Empty until the first frame.
pub fn waterfall(image: Option<Arc<RenderImage>>) -> impl IntoElement {
    div().size_full().when_some(image, |this, image| {
        this.child(
            img(ImageSource::Render(image))
                .size_full()
                .object_fit(ObjectFit::Fill),
        )
    })
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

/// Max-pool `bins` into `WF_COLS` columns and colormap each over `[floor, ceil]` → one BGRA row.
fn make_row(bins: &[f32], floor: f32, ceil: f32, lut: &[[u8; 4]; 256]) -> Box<[u8]> {
    let n = bins.len();
    let span = (ceil - floor).max(1.0);
    let mut row = vec![0u8; WF_COLS * 4];
    for c in 0..WF_COLS {
        let start = c * n / WF_COLS;
        let end = ((c + 1) * n / WF_COLS).clamp(start + 1, n);
        let db = bins[start..end]
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let t = ((db - floor) / span).clamp(0.0, 1.0);
        let idx = ((t * 255.0) as usize).min(255);
        row[c * 4..c * 4 + 4].copy_from_slice(&lut[idx]);
    }
    row.into_boxed_slice()
}

fn spectrum_path(bins: &[f32], bounds: Bounds<Pixels>, range: (f32, f32)) -> Option<Path<Pixels>> {
    let (floor, ceil) = range;
    let span = (ceil - floor).max(1.0);
    let n = bins.len();
    let cols = (f32::from(bounds.size.width) as usize).max(2);
    let (x0, y0, w, h) = (
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    );

    let mut builder = PathBuilder::stroke(px(1.0));
    for c in 0..cols {
        let start = c * n / cols;
        let end = ((c + 1) * n / cols).clamp(start + 1, n);
        let db = bins[start..end]
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let t = ((db - floor) / span).clamp(0.0, 1.0);
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

/// Horizontal dB gridlines at quarter-height intervals.
fn paint_grid(window: &mut gpui::Window, bounds: Bounds<Pixels>, color: Hsla) {
    let (x0, y0, w, h) = (
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    );
    for i in 1..4 {
        let y = px(y0 + h * i as f32 / 4.0);
        let mut builder = PathBuilder::stroke(px(1.0));
        builder.move_to(point(px(x0), y));
        builder.line_to(point(px(x0 + w), y));
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
    let (x0, y0, w, h) = (
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    );
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
