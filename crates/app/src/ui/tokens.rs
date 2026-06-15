//! Design tokens: the small fixed scale the UI is built from. Sizes live here (one source of
//! truth) so chrome stays visually consistent; colors and font families come from the theme.

use gpui::{px, Pixels};

/// Type scale (px). Labels/reading text use the proportional UI font; values/data use the
/// theme's mono family.
pub const TEXT_SM: Pixels = px(11.);
pub const TEXT_MD: Pixels = px(13.);

/// Axis tick labels (spectrum dB + frequency scale) and the signal-level meter.
pub const TEXT_AXIS: Pixels = px(12.);

/// Width of the spectrum dB-axis gutter; the frequency scale is inset by the same amount so its
/// labels line up under the plot.
pub const DB_AXIS_WIDTH: Pixels = px(44.);
