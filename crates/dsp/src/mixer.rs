//! Frequency translation. Multiplying a stream by a complex exponential shifts the spectrum,
//! used to bring a station that sits at some offset within the captured window down to
//! baseband (0 Hz) before low-pass filtering and decimation.

use std::f32::consts::TAU;

use sdr_core::Iq;

/// A numerically controlled oscillator and complex mixer.
///
/// Rather than call `sin_cos` per sample, it advances the phasor by a fixed complex rotation
/// (`osc *= rot`), which is a couple of multiplies per sample. That accumulates magnitude
/// drift, so the phasor is renormalized to the unit circle periodically.
pub struct Nco {
    osc: Iq,
    rot: Iq,
    since_norm: u32,
}

/// Renormalize the phasor this often. Magnitude drifts slowly, so a wide interval keeps the
/// per-sample cost to two multiplies while holding amplitude error far below the noise floor.
const RENORM_INTERVAL: u32 = 1024;

impl Nco {
    /// A mixer that shifts the spectrum by `shift_hz` at `sample_rate`. A positive shift moves
    /// the spectrum up in frequency; to translate a station at `+offset` in the window down to
    /// baseband, pass `shift_hz = -offset`.
    pub fn new(shift_hz: f64, sample_rate: u32) -> Self {
        let step = (TAU as f64 * shift_hz / sample_rate as f64) as f32;
        Self {
            osc: Iq::new(1.0, 0.0),
            rot: Iq::from_polar(1.0, step),
            since_norm: 0,
        }
    }

    /// Mix `input` into `out` (must be the same length): `out[n] = input[n] · e^{j·phase}`.
    pub fn mix(&mut self, input: &[Iq], out: &mut [Iq]) {
        for (o, &x) in out.iter_mut().zip(input) {
            *o = x * self.osc;
            self.advance();
        }
    }

    /// In-place mix: `buf[n] *= e^{j·phase}`.
    pub fn mix_in_place(&mut self, buf: &mut [Iq]) {
        for x in buf.iter_mut() {
            *x *= self.osc;
            self.advance();
        }
    }

    fn advance(&mut self) {
        self.osc *= self.rot;
        self.since_norm += 1;
        if self.since_norm >= RENORM_INTERVAL {
            self.since_norm = 0;
            let n = self.osc.norm();
            if n > 0.0 {
                self.osc /= n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A complex tone at `freq_hz`.
    fn tone(n: usize, freq_hz: f64, fs: u32) -> Vec<Iq> {
        (0..n)
            .map(|i| {
                let p = TAU as f64 * freq_hz * i as f64 / fs as f64;
                Iq::new(p.cos() as f32, p.sin() as f32)
            })
            .collect()
    }

    #[test]
    fn shift_moves_a_tone_to_dc() {
        let fs = 1_000_000;
        let input = tone(4096, 100_000.0, fs);
        let mut nco = Nco::new(-100_000.0, fs);
        let mut out = vec![Iq::default(); input.len()];
        nco.mix(&input, &mut out);
        // A +100 kHz tone shifted by -100 kHz is DC: every sample collapses to ~(1, 0).
        for s in &out[10..] {
            assert!((s.re - 1.0).abs() < 1e-3, "re = {}", s.re);
            assert!(s.im.abs() < 1e-3, "im = {}", s.im);
        }
    }

    #[test]
    fn magnitude_stays_unit_over_a_long_run() {
        let fs = 2_400_000;
        let mut nco = Nco::new(57_000.0, fs);
        let mut buf = vec![Iq::new(1.0, 0.0); 1_000_000];
        nco.mix_in_place(&mut buf);
        for s in buf.iter().step_by(50_000) {
            assert!((s.norm() - 1.0).abs() < 1e-3, "drifted to {}", s.norm());
        }
    }
}
