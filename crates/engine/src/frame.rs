//! Frame taps: array-valued snapshots of a pipeline stage, published lock-free by the reader
//! thread and read at frame rate by consumers. The scalar [`Snapshot`](crate::Snapshot) is the
//! same idea for cheap `Copy` stats; these carry buffers, so they live behind an `Arc` and a
//! consumer takes the whole frame atomically.

use std::sync::Arc;

use arc_swap::ArcSwap;
use sdr_core::Iq;

/// A lock-free single-slot tap: the producer overwrites, a reader takes the latest. A missed
/// frame has no consequence. The one primitive behind every stage view.
pub type Tap<T> = Arc<ArcSwap<T>>;

/// The wideband power spectrum at a moment in time. Bins are fftshifted (DC at the center
/// index `fft_size / 2`), so bin `i` maps to `center_freq + (i - fft_size/2) · sample_rate /
/// fft_size`. `seq` advances once per published frame; a consumer compares it to detect new data.
#[derive(Debug, Clone)]
pub struct SpectrumFrame {
    /// dBFS magnitude per bin, length `fft_size`, DC-centered.
    pub bins_db: Box<[f32]>,
    pub fft_size: usize,
    pub center_freq: u64,
    pub sample_rate: u32,
    pub seq: u64,
}

impl SpectrumFrame {
    pub(crate) fn initial(fft_size: usize, center_freq: u64, sample_rate: u32) -> Self {
        Self {
            bins_db: vec![f32::NEG_INFINITY; fft_size].into_boxed_slice(),
            fft_size,
            center_freq,
            sample_rate,
            seq: 0,
        }
    }
}

/// A recent slice of raw IQ for the time-domain waveform view. `seq` advances per frame.
#[derive(Debug, Clone)]
pub struct WaveformFrame {
    pub samples: Box<[Iq]>,
    pub sample_rate: u32,
    pub seq: u64,
}

impl WaveformFrame {
    pub(crate) fn initial(sample_rate: u32) -> Self {
        Self {
            samples: Box::new([]),
            sample_rate,
            seq: 0,
        }
    }
}
