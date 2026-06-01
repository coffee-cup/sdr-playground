//! Window functions applied before the FFT to trade main-lobe width against side-lobe
//! suppression. A rectangular window has the sharpest peak but leaks energy across the
//! spectrum; Hann is the general-purpose default.

use std::f32::consts::PI;

/// A window function, multiplied sample-wise over an FFT input block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowKind {
    /// No window (boxcar): sharpest main lobe, worst side lobes.
    Rectangular,
    /// Hann (raised cosine): a good general-purpose default.
    #[default]
    Hann,
}

impl WindowKind {
    /// Window coefficients for a block of `len` samples. The Hann window uses the periodic
    /// (DFT-even) form — denominator `len`, not `len - 1` — which is the correct variant for
    /// spectral analysis with an FFT.
    pub fn coefficients(self, len: usize) -> Vec<f32> {
        match self {
            WindowKind::Rectangular => vec![1.0; len],
            WindowKind::Hann => (0..len)
                .map(|n| 0.5 - 0.5 * (2.0 * PI * n as f32 / len as f32).cos())
                .collect(),
        }
    }
}

/// Coherent gain of a window: the mean of its coefficients. A windowed FFT scales an on-bin
/// tone's peak by `len * coherent_gain`, so dividing by that restores 0 dBFS for a full-scale
/// tone regardless of window choice.
pub fn coherent_gain(coefficients: &[f32]) -> f32 {
    coefficients.iter().sum::<f32>() / coefficients.len() as f32
}
