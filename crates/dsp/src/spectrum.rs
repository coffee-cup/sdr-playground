//! Wideband power spectrum: window → FFT → magnitude in dBFS, arranged DC-center so the
//! output maps directly onto a display spanning `center_freq ± sample_rate/2`.

use sdr_core::Iq;

use crate::fft::Forward;
use crate::window::{self, WindowKind};

/// Spectrum analysis parameters. Sane defaults; exposed for future per-channel configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpectrumConfig {
    /// Transform size in samples. Larger = finer frequency resolution, coarser in time.
    pub fft_size: usize,
    pub window: WindowKind,
}

impl Default for SpectrumConfig {
    fn default() -> Self {
        Self {
            fft_size: 8192,
            window: WindowKind::Hann,
        }
    }
}

/// Computes the power spectrum of fixed-size IQ blocks. Holds the FFT plan, window
/// coefficients, and scratch/output buffers so steady-state processing allocates nothing.
///
/// Blocks can be averaged (Welch's method): [`accumulate`](Self::accumulate) several blocks,
/// then [`finish`](Self::finish) to read their mean. Averaging trades time resolution for a
/// far smoother estimate — the difference between a grainy waterfall and a clean one.
pub struct SpectrumAnalyzer {
    config: SpectrumConfig,
    fft: Forward,
    window: Vec<f32>,
    /// `20·log10(N · coherent_gain)`, subtracted so a full-scale on-bin tone reads 0 dBFS.
    norm_db: f32,
    buf: Vec<Iq>,
    /// Accumulated `|X[k]|²` across blocks, in natural (un-shifted) bin order.
    power: Vec<f32>,
    count: u32,
    bins_db: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new(config: SpectrumConfig) -> Self {
        let n = config.fft_size;
        let window = config.window.coefficients(n);
        let norm_db = 20.0 * (n as f32 * window::coherent_gain(&window)).log10();
        Self {
            fft: Forward::new(n),
            window,
            norm_db,
            buf: vec![Iq::default(); n],
            power: vec![0.0; n],
            count: 0,
            bins_db: vec![f32::NEG_INFINITY; n],
            config,
        }
    }

    pub fn fft_size(&self) -> usize {
        self.config.fft_size
    }

    /// Whether any block has been accumulated since the last [`finish`](Self::finish).
    pub fn pending(&self) -> bool {
        self.count > 0
    }

    /// Window and transform one block, adding its power into the running average. `input` must
    /// be at least `fft_size` long; any excess is ignored.
    pub fn accumulate(&mut self, input: &[Iq]) {
        let n = self.config.fft_size;
        let input = &input[..n];

        for (dst, (&src, &w)) in self.buf.iter_mut().zip(input.iter().zip(&self.window)) {
            *dst = src * w;
        }
        self.fft.process(&mut self.buf);
        for (acc, sample) in self.power.iter_mut().zip(&self.buf) {
            *acc += sample.norm_sqr();
        }
        self.count += 1;
    }

    /// Average the accumulated blocks and return fftshifted dBFS bins of length `fft_size`: DC
    /// at the center index `fft_size/2`, negative frequencies to the left, positive to the
    /// right. Resets the accumulator. With nothing accumulated, returns the previous result.
    pub fn finish(&mut self) -> &[f32] {
        if self.count == 0 {
            return &self.bins_db;
        }
        let n = self.config.fft_size;
        let half = n / 2;
        let count = self.count as f32;
        for (k, &p) in self.power.iter().enumerate() {
            let mean = p / count;
            let db = if mean > 0.0 {
                10.0 * mean.log10() - self.norm_db
            } else {
                f32::NEG_INFINITY
            };
            self.bins_db[(k + half) % n] = db;
        }
        self.power.iter_mut().for_each(|p| *p = 0.0);
        self.count = 0;
        &self.bins_db
    }

    /// Power spectrum of a single block — convenience for `accumulate` + `finish`.
    pub fn process(&mut self, input: &[Iq]) -> &[f32] {
        self.accumulate(input);
        self.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// A unit-amplitude complex exponential at FFT bin `k0` (may be negative, wrapping).
    fn tone(n: usize, k0: i64) -> Vec<Iq> {
        (0..n)
            .map(|i| {
                let phase = TAU * (k0 as f32) * (i as f32) / (n as f32);
                Iq::new(phase.cos(), phase.sin())
            })
            .collect()
    }

    fn argmax(bins: &[f32]) -> usize {
        bins.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0
    }

    #[test]
    fn dc_lands_at_center() {
        let n = 1024;
        let mut sa = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: n,
            window: WindowKind::Hann,
        });
        let dc = vec![Iq::new(1.0, 0.0); n];
        let bins = sa.process(&dc);
        assert_eq!(argmax(bins), n / 2, "DC must sit at the center index");
        assert!(
            (bins[n / 2]).abs() < 0.1,
            "full-scale DC should read ~0 dBFS, got {}",
            bins[n / 2]
        );
    }

    #[test]
    fn positive_tone_lands_right_of_center() {
        let n = 1024;
        let k0 = 64;
        let mut sa = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: n,
            window: WindowKind::Rectangular,
        });
        let bins = sa.process(&tone(n, k0));
        assert_eq!(argmax(bins), n / 2 + k0 as usize);
    }

    #[test]
    fn negative_tone_lands_left_of_center() {
        let n = 1024;
        let k0 = -64;
        let mut sa = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: n,
            window: WindowKind::Rectangular,
        });
        let bins = sa.process(&tone(n, k0));
        assert_eq!(argmax(bins), n / 2 - 64);
    }

    #[test]
    fn full_scale_tone_reads_zero_dbfs() {
        let n = 1024;
        let k0 = 100;
        let mut sa = SpectrumAnalyzer::new(SpectrumConfig {
            fft_size: n,
            window: WindowKind::Hann,
        });
        let bins = sa.process(&tone(n, k0));
        let peak = bins[n / 2 + k0 as usize];
        assert!(
            (peak).abs() < 0.2,
            "on-bin full-scale tone should normalize to ~0 dBFS, got {peak}"
        );
    }
}
