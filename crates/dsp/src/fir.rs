//! FIR low-pass filtering and decimation. After a [`Nco`](crate::Nco) shifts a channel to
//! baseband, a low-pass FIR removes everything outside the channel and decimation drops the
//! sample rate to something a demodulator can work with cheaply.

use std::f32::consts::{PI, TAU};

use sdr_core::Iq;

/// Design a linear-phase low-pass FIR by the windowed-sinc method (Hann window).
///
/// `cutoff` is the passband edge as a fraction of the sample rate (cycles per sample, so
/// 0.0..0.5). `num_taps` is forced odd to give a Type-I symmetric filter with an integer
/// group delay. DC gain is normalized to 1.
pub fn lowpass(num_taps: usize, cutoff: f32) -> Vec<f32> {
    let n = num_taps | 1;
    let m = (n - 1) as f32;
    let mut taps: Vec<f32> = (0..n)
        .map(|i| {
            let x = i as f32 - m / 2.0;
            // 2·fc·sinc(2·fc·x) with sinc(t) = sin(πt)/(πt); the x == 0 limit is 2·fc.
            let sinc = if x == 0.0 {
                2.0 * cutoff
            } else {
                (TAU * cutoff * x).sin() / (PI * x)
            };
            let hann = 0.5 - 0.5 * (TAU * i as f32 / m).cos();
            sinc * hann
        })
        .collect();
    let sum: f32 = taps.iter().sum();
    if sum != 0.0 {
        for t in &mut taps {
            *t /= sum;
        }
    }
    taps
}

/// Design a linear-phase band-pass FIR by modulating a low-pass prototype up to `center`.
/// `center` and `half_width` are fractions of the sample rate; the passband spans
/// `center ± half_width`. Used to isolate the 19 kHz pilot from the rest of the FM multiplex.
pub fn bandpass(num_taps: usize, center: f32, half_width: f32) -> Vec<f32> {
    let proto = lowpass(num_taps, half_width);
    let mid = (proto.len() - 1) as f32 / 2.0;
    proto
        .iter()
        .enumerate()
        .map(|(i, &h)| h * 2.0 * (TAU * center * (i as f32 - mid)).cos())
        .collect()
}

/// Root-raised-cosine pulse, sampled at `sps` samples per symbol over `±span` symbols, with
/// roll-off `beta`. Used as the matched filter for the RDS biphase chip pulse. Normalized to
/// unit energy.
pub fn root_raised_cosine(sps: f32, span: usize, beta: f32) -> Vec<f32> {
    let n = (2.0 * span as f32 * sps).round() as usize | 1;
    let mid = (n - 1) as f32 / 2.0;
    let mut taps: Vec<f32> = (0..n)
        .map(|i| {
            let t = (i as f32 - mid) / sps;
            if t.abs() < 1e-6 {
                1.0 - beta + 4.0 * beta / PI
            } else if (t.abs() - 1.0 / (4.0 * beta)).abs() < 1e-4 {
                let a = 4.0 * beta;
                (beta / 2.0_f32.sqrt())
                    * ((1.0 + 2.0 / PI) * (PI / a).sin() + (1.0 - 2.0 / PI) * (PI / a).cos())
            } else {
                let num =
                    (PI * t * (1.0 - beta)).sin() + 4.0 * beta * t * (PI * t * (1.0 + beta)).cos();
                let den = PI * t * (1.0 - (4.0 * beta * t).powi(2));
                num / den
            }
        })
        .collect();
    let energy = taps.iter().map(|t| t * t).sum::<f32>().sqrt();
    if energy > 0.0 {
        for t in &mut taps {
            *t /= energy;
        }
    }
    taps
}

/// A streaming real FIR filter (no decimation). Holds a ring buffer so feeding is
/// allocation-free.
pub struct FirFilter {
    taps: Box<[f32]>,
    ring: Box<[f32]>,
    pos: usize,
}

impl FirFilter {
    pub fn new(taps: Vec<f32>) -> Self {
        let len = taps.len().max(1);
        Self {
            taps: taps.into_boxed_slice(),
            ring: vec![0.0; len].into_boxed_slice(),
            pos: 0,
        }
    }

    /// Filter one sample.
    pub fn process_sample(&mut self, x: f32) -> f32 {
        self.ring[self.pos] = x;
        self.pos = (self.pos + 1) % self.ring.len();
        let len = self.ring.len();
        let mut acc = 0.0;
        for (k, &t) in self.taps.iter().enumerate() {
            acc += self.ring[(self.pos + k) % len] * t;
        }
        acc
    }
}

/// A decimating low-pass FIR over complex samples: filters with the given taps and keeps every
/// `decim`-th output. Holds a ring buffer of recent input so feeding is allocation-free.
pub struct FirDecimator {
    taps: Box<[f32]>,
    decim: usize,
    ring: Box<[Iq]>,
    pos: usize,
    since: usize,
}

impl FirDecimator {
    pub fn new(taps: Vec<f32>, decim: usize) -> Self {
        assert!(decim >= 1, "decimation must be >= 1");
        let len = taps.len().max(1);
        Self {
            taps: taps.into_boxed_slice(),
            decim,
            ring: vec![Iq::default(); len].into_boxed_slice(),
            pos: 0,
            since: 0,
        }
    }

    /// Output sample rate for an input at `input_rate`.
    pub fn output_rate(&self, input_rate: u32) -> u32 {
        input_rate / self.decim as u32
    }

    /// Filter `input` and append decimated outputs to `out`.
    pub fn process(&mut self, input: &[Iq], out: &mut Vec<Iq>) {
        for &x in input {
            self.ring[self.pos] = x;
            self.pos = (self.pos + 1) % self.ring.len();
            self.since += 1;
            if self.since == self.decim {
                self.since = 0;
                out.push(self.dot());
            }
        }
    }

    /// Convolve the taps with the ring's contents. The taps are symmetric (linear phase), so
    /// applying them oldest-to-newest gives the same result as the formal time-reversed sum.
    fn dot(&self) -> Iq {
        let len = self.ring.len();
        let mut acc = Iq::default();
        for (k, &t) in self.taps.iter().enumerate() {
            let idx = (self.pos + k) % len;
            acc += self.ring[idx] * t;
        }
        acc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, freq_hz: f64, fs: u32) -> Vec<Iq> {
        (0..n)
            .map(|i| {
                let p = TAU as f64 * freq_hz * i as f64 / fs as f64;
                Iq::new(p.cos() as f32, p.sin() as f32)
            })
            .collect()
    }

    fn power(samples: &[Iq]) -> f32 {
        samples.iter().map(|s| s.norm_sqr()).sum::<f32>() / samples.len().max(1) as f32
    }

    #[test]
    fn taps_are_odd_length_and_unit_dc_gain() {
        let taps = lowpass(64, 0.1);
        assert_eq!(taps.len() % 2, 1);
        assert!((taps.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn passes_in_band_blocks_out_of_band() {
        let fs = 1_000_000;
        // Cutoff at 0.1·fs = 100 kHz. A 20 kHz tone passes; a 300 kHz tone is rejected.
        let taps = lowpass(127, 0.1);

        let mut dec = FirDecimator::new(taps.clone(), 1);
        let mut passed = Vec::new();
        dec.process(&tone(8192, 20_000.0, fs), &mut passed);
        assert!(power(&passed[200..]) > 0.5, "in-band tone should survive");

        let mut dec = FirDecimator::new(taps, 1);
        let mut blocked = Vec::new();
        dec.process(&tone(8192, 300_000.0, fs), &mut blocked);
        assert!(
            power(&blocked[200..]) < 0.01,
            "out-of-band tone should be rejected"
        );
    }

    #[test]
    fn decimation_reduces_output_count_and_rate() {
        let mut dec = FirDecimator::new(lowpass(31, 0.05), 10);
        assert_eq!(dec.output_rate(2_400_000), 240_000);
        let mut out = Vec::new();
        dec.process(&vec![Iq::new(1.0, 0.0); 1000], &mut out);
        assert_eq!(out.len(), 100);
    }
}
