//! Segmented signal meters: a flat row (or column) of green/yellow/red cells, lit up to a level.
//! Read-only. The level meter in the tuning header and the device-edge meter share this.

use gpui::{div, px, App, Hsla, IntoElement, ParentElement, Styled};
use gpui_component::Colorize;

use crate::ui::palette;

/// Orientation of a segmented meter.
#[derive(Clone, Copy, PartialEq)]
pub enum MeterDir {
    Horizontal,
    Vertical,
}

/// A segmented meter filled to `frac` (0..1) across `segments` cells. Lit cells run green → yellow
/// → red from the quiet end; the rest are dim. Vertical meters fill from the bottom.
pub fn segmented_meter(frac: f32, segments: usize, dir: MeterDir, cx: &App) -> impl IntoElement {
    let p = palette(cx);
    let unlit = p.surface_2.darken(0.45);
    let frac = frac.clamp(0.0, 1.0);
    let lit = (frac * segments as f32).round() as usize;

    // Color a cell by its distance from the quiet end (0 = quiet, segments = loud).
    let color_at = move |from_quiet: usize| -> Hsla {
        if from_quiet >= lit {
            return unlit;
        }
        let pos = from_quiet as f32 / segments as f32;
        if pos < 0.66 {
            p.meter_green
        } else if pos < 0.86 {
            p.meter_yellow
        } else {
            p.meter_red
        }
    };

    match dir {
        MeterDir::Horizontal => div()
            .flex()
            .flex_row()
            .gap(px(2.))
            .w_full()
            .h(px(8.))
            .children((0..segments).map(|i| div().flex_1().bg(color_at(i)))),
        // Loudest cell rendered first so the column reads top = loud, and the lit (quiet) cells
        // land at the bottom.
        MeterDir::Vertical => div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .h_full()
            .w(px(8.))
            .children((0..segments).rev().map(|i| div().flex_1().bg(color_at(i)))),
    }
}
