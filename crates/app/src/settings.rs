//! Persisted UI/session state: the single source of truth for everything the user can change
//! (tuning, display, FFT settings, panel sizes). Held by `SdrApp`, loaded on launch and saved
//! on change through [`crate::store`]. App-layer enums keep `core`/`dsp` serde-free.

use sdr_engine::{SpectrumConfig, WindowKind};
use serde::{Deserialize, Serialize};

use crate::colormap::Colormap;
use crate::nav::Workspace;

/// FFT analysis window, mirrored from `dsp::WindowKind` so the persisted form does not pull
/// serde into the DSP crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowChoice {
    Hann,
    Rectangular,
}

impl WindowChoice {
    pub fn label(self) -> &'static str {
        match self {
            WindowChoice::Hann => "Hann",
            WindowChoice::Rectangular => "Rectangular",
        }
    }

    pub fn to_kind(self) -> WindowKind {
        match self {
            WindowChoice::Hann => WindowKind::Hann,
            WindowChoice::Rectangular => WindowKind::Rectangular,
        }
    }
}

/// The FFT sizes offered in the settings panel. Powers of two spanning coarse/fast to fine/slow.
pub const FFT_SIZES: [usize; 5] = [1024, 2048, 4096, 8192, 16384];

/// The frame rates offered in the settings panel (Hz).
pub const FPS_CHOICES: [f32; 4] = [15.0, 25.0, 30.0, 60.0];

/// Marker-bandwidth presets (Hz).
pub const BW_PRESETS: [u32; 5] = [50_000, 100_000, 200_000, 500_000, 1_000_000];

/// Human label for a marker bandwidth.
pub fn bandwidth_label(hz: u32) -> String {
    if hz >= 1_000_000 {
        format!("{:.1} MHz", hz as f64 / 1e6)
    } else {
        format!("{} kHz", hz / 1000)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Hardware center frequency, Hz: where the captured ~`sample_rate`-wide window sits. The
    /// device only re-tunes when the tuned frequency would leave this window; kept invisible.
    pub center_freq: u64,
    /// The frequency the user is tuned to (the marker), Hz. The only frequency shown. Always
    /// within the captured window `center_freq ± sample_rate/2`. `serde(default)` yields 0 for
    /// configs written before this field existed; `SdrApp::new` migrates that to `center_freq`.
    #[serde(default)]
    pub tuned_freq: u64,
    /// Channel bandwidth drawn as the marker rectangle, Hz.
    pub bandwidth: u32,
    pub fft_size: usize,
    pub window: WindowChoice,
    pub fps: f32,
    pub colormap: Colormap,
    /// Exponential display smoothing of the spectrum line, 0 (off) to 1 (frozen).
    pub averaging: f32,
    /// When true the dB window auto-tracks the noise floor; otherwise `db_min`/`db_max` apply.
    pub db_auto: bool,
    pub db_min: f32,
    pub db_max: f32,
    pub active: Workspace,
}

impl Default for Settings {
    fn default() -> Self {
        // Tuned for the canonical first run: an FM broadcast station. 100 MHz sits mid-band; a
        // 200 kHz marker is one WFM channel; light averaging steadies the trace; auto dB keeps
        // the floor readable across gain. The device opens at 2.048 MS/s (see `RtlConfig`).
        Self {
            center_freq: 100_000_000,
            tuned_freq: 100_000_000,
            bandwidth: 200_000,
            fft_size: 8192,
            window: WindowChoice::Hann,
            fps: 30.0,
            colormap: Colormap::default(),
            averaging: 0.3,
            db_auto: true,
            db_min: -100.0,
            db_max: -20.0,
            active: Workspace::Listen,
        }
    }
}

impl Settings {
    pub fn spectrum_config(&self) -> SpectrumConfig {
        SpectrumConfig {
            fft_size: self.fft_size,
            window: self.window.to_kind(),
        }
    }

    /// The fixed dB window when manual, or `None` when the display should auto-track the floor.
    pub fn manual_db_range(&self) -> Option<(f32, f32)> {
        (!self.db_auto).then_some((self.db_min, self.db_max))
    }
}
