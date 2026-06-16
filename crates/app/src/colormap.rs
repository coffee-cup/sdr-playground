//! Waterfall colormaps. A normalized magnitude in `[0, 1]` indexes a 256-entry BGRA lookup
//! table. New palettes are added here; the set is meant to become a UI picker, so each map
//! carries a label and the registry lists them all.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Colormap {
    /// Cool ramp (deep blue → cyan → white) that complements the app's orange chrome accent.
    /// The default, per `docs/UI.md`.
    #[default]
    Ice,
    /// gqrx's classic waterfall palette: black → blue → cyan → yellow → red → white.
    Gqrx,
    /// Perceptual inferno: dark purple → magenta → orange → pale yellow. More dynamic range,
    /// keeps the noise floor dark.
    Inferno,
}

impl Colormap {
    /// All palettes, for the UI selector.
    pub const ALL: [Colormap; 3] = [Colormap::Ice, Colormap::Gqrx, Colormap::Inferno];

    pub fn label(self) -> &'static str {
        match self {
            Colormap::Ice => "Ice",
            Colormap::Gqrx => "Gqrx",
            Colormap::Inferno => "Inferno",
        }
    }

    /// The palette's 256-entry BGRA lookup table, built once.
    pub fn lut(self) -> &'static [[u8; 4]; 256] {
        match self {
            Colormap::Ice => {
                static LUT: OnceLock<[[u8; 4]; 256]> = OnceLock::new();
                LUT.get_or_init(ice)
            }
            Colormap::Gqrx => {
                static LUT: OnceLock<[[u8; 4]; 256]> = OnceLock::new();
                LUT.get_or_init(gqrx)
            }
            Colormap::Inferno => {
                static LUT: OnceLock<[[u8; 4]; 256]> = OnceLock::new();
                LUT.get_or_init(inferno)
            }
        }
    }
}

/// Ice, interpolated from RGB stops: near-black → blue → cyan → white.
fn ice() -> [[u8; 4]; 256] {
    const STOPS: [(f32, f32, f32, f32); 6] = [
        (0.00, 8.0, 12.0, 20.0),
        (0.35, 16.0, 52.0, 96.0),
        (0.60, 30.0, 120.0, 176.0),
        (0.78, 70.0, 190.0, 224.0),
        (0.90, 150.0, 224.0, 240.0),
        (1.00, 232.0, 246.0, 252.0),
    ];
    stops_to_lut(&STOPS)
}

/// A port of gqrx's default waterfall colormap (`qtgui` plotter), piecewise in index space.
fn gqrx() -> [[u8; 4]; 256] {
    let mut lut = [[0u8; 4]; 256];
    for (idx, entry) in lut.iter_mut().enumerate() {
        let i = idx as i32;
        let (r, g, b) = if i < 20 {
            (0, 0, 0)
        } else if i < 70 {
            (0, 0, 140 * (i - 20) / 50)
        } else if i < 100 {
            (
                60 * (i - 70) / 30,
                125 * (i - 70) / 30,
                115 * (i - 70) / 30 + 140,
            )
        } else if i < 150 {
            (
                195 * (i - 100) / 50 + 60,
                130 * (i - 100) / 50 + 125,
                255 - 255 * (i - 100) / 50,
            )
        } else if i < 250 {
            (255, 255 - 255 * (i - 150) / 100, 0)
        } else {
            (255, 255 * (i - 250) / 5, 255 * (i - 250) / 5)
        };
        *entry = [b as u8, g as u8, r as u8, 255];
    }
    lut
}

/// Inferno, interpolated from RGB stops.
fn inferno() -> [[u8; 4]; 256] {
    const STOPS: [(f32, f32, f32, f32); 8] = [
        (0.00, 0.0, 0.0, 4.0),
        (0.15, 20.0, 11.0, 53.0),
        (0.30, 66.0, 10.0, 104.0),
        (0.45, 120.0, 28.0, 109.0),
        (0.60, 175.0, 55.0, 84.0),
        (0.75, 228.0, 96.0, 43.0),
        (0.90, 251.0, 162.0, 35.0),
        (1.00, 252.0, 255.0, 164.0),
    ];
    stops_to_lut(&STOPS)
}

/// Build a 256-entry BGRA lookup table by linearly interpolating `(t, r, g, b)` RGB stops.
fn stops_to_lut(stops: &[(f32, f32, f32, f32)]) -> [[u8; 4]; 256] {
    let mut lut = [[0u8; 4]; 256];
    for (i, entry) in lut.iter_mut().enumerate() {
        let t = i as f32 / 255.0;
        let mut j = 0;
        while j < stops.len() - 1 && t > stops[j + 1].0 {
            j += 1;
        }
        let (a, b) = (stops[j], stops[j + 1]);
        let f = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
        let r = (a.1 + (b.1 - a.1) * f) as u8;
        let g = (a.2 + (b.2 - a.2) * f) as u8;
        let bl = (a.3 + (b.3 - a.3) * f) as u8;
        *entry = [bl, g, r, 255]; // BGRA
    }
    lut
}
