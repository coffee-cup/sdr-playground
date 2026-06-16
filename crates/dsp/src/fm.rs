//! FM demodulation by phase discrimination.
//!
//! The information on an FM carrier is its instantaneous frequency, which is the derivative of
//! its phase. For complex baseband, the phase step between consecutive samples is
//! `arg(x[n] · conj(x[n-1]))`, so that product's argument is the demodulated signal. For
//! broadcast FM the result is the multiplex (MPX): mono audio, the 19 kHz pilot, the stereo
//! difference around 38 kHz, and the RDS subcarrier at 57 kHz.

use sdr_core::Iq;

/// A streaming FM discriminator. Carries the previous sample across calls so block boundaries
/// are seamless.
pub struct FmDemod {
    prev: Iq,
    gain: f32,
}

impl FmDemod {
    /// `gain` scales the discriminator output. For a calibrated level use
    /// `sample_rate / (2π · deviation_hz)`; downstream decoders only care about relative
    /// amplitude, so `1.0` is fine there.
    pub fn new(gain: f32) -> Self {
        Self {
            prev: Iq::new(0.0, 0.0),
            gain,
        }
    }

    /// Demodulate `input`, appending one real output per input sample to `out`.
    pub fn process(&mut self, input: &[Iq], out: &mut Vec<f32>) {
        for &x in input {
            let d = x * self.prev.conj();
            out.push(d.arg() * self.gain);
            self.prev = x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// A complex carrier frequency-modulated by a single sine tone.
    fn fm_signal(n: usize, fs: u32, audio_hz: f64, deviation_hz: f64) -> Vec<Iq> {
        let mut phase = 0.0f64;
        (0..n)
            .map(|i| {
                let t = i as f64 / fs as f64;
                let inst = deviation_hz * (TAU as f64 * audio_hz * t).sin();
                phase += TAU as f64 * inst / fs as f64;
                Iq::new(phase.cos() as f32, phase.sin() as f32)
            })
            .collect()
    }

    #[test]
    fn recovers_the_modulating_tone() {
        let fs = 240_000;
        let audio_hz = 1_000.0;
        let deviation = 5_000.0;
        let sig = fm_signal(4096, fs, audio_hz, deviation);

        let mut fm = FmDemod::new(fs as f32 / (TAU * deviation as f32));
        let mut out = Vec::new();
        fm.process(&sig, &mut out);

        // The demodulated signal is a 1 kHz sine; check it crosses zero and swings near ±1
        // (the deviation-normalized amplitude), ignoring the first-sample transient.
        let tail = &out[2..];
        let peak = tail.iter().fold(0.0f32, |m, &v| m.max(v.abs()));
        assert!(peak > 0.8 && peak < 1.3, "recovered amplitude {peak}");
    }

    #[test]
    fn constant_offset_reads_as_constant_frequency() {
        // A pure +10 kHz tone has constant instantaneous frequency, so the discriminator output
        // is a constant proportional to that frequency.
        let fs = 240_000;
        let n = 1024;
        let sig: Vec<Iq> = (0..n)
            .map(|i| {
                let p = TAU as f64 * 10_000.0 * i as f64 / fs as f64;
                Iq::new(p.cos() as f32, p.sin() as f32)
            })
            .collect();
        let mut fm = FmDemod::new(1.0);
        let mut out = Vec::new();
        fm.process(&sig, &mut out);
        let want = TAU * 10_000.0 / fs as f32;
        for &v in &out[2..] {
            assert!((v - want).abs() < 1e-3, "got {v}, want {want}");
        }
    }
}
