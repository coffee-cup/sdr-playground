//! A second-order phase-locked loop that tracks a real sinusoid.
//!
//! RDS rides a suppressed subcarrier at 57 kHz, exactly three times the 19 kHz stereo pilot.
//! Locking a PLL to the strong, always-present pilot and tripling its phase regenerates a
//! coherent 57 kHz reference for synchronous detection, which is far more robust than trying
//! to recover the weak suppressed carrier directly.

use std::f32::consts::TAU;

/// Tracks the phase and frequency of an input tone near a nominal frequency.
pub struct Pll {
    /// Current reference phase, radians.
    phase: f32,
    /// Current frequency estimate, radians per sample.
    freq: f32,
    /// Nominal (center) frequency, radians per sample.
    center: f32,
    /// Allowed deviation from center, radians per sample (pull-in range).
    pull: f32,
    /// Loop-filter gains (proportional, integral).
    alpha: f32,
    beta: f32,
}

impl Pll {
    /// A PLL centered on `center_hz` at `sample_rate`, with the given loop bandwidth and
    /// damping (0.707 is a good default). The lock range is ±5% of the center frequency.
    pub fn new(center_hz: f32, sample_rate: u32, loop_bw_hz: f32, damping: f32) -> Self {
        let center = TAU * center_hz / sample_rate as f32;
        let w = TAU * loop_bw_hz / sample_rate as f32;
        let d = 1.0 + 2.0 * damping * w + w * w;
        Self {
            phase: 0.0,
            freq: center,
            center,
            pull: center * 0.05,
            alpha: 4.0 * damping * w / d,
            beta: 4.0 * w * w / d,
        }
    }

    /// Current locked frequency in Hz (for diagnostics and lock checks).
    pub fn freq_hz(&self, sample_rate: u32) -> f32 {
        self.freq * sample_rate as f32 / TAU
    }

    /// Advance one sample against real input `x`, returning the reference phase for this sample.
    /// Multiply the input by `cos(3·phase)` / `sin(3·phase)` to mix the 57 kHz subcarrier to
    /// baseband.
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let phase = self.phase;
        // Phase detector: x·sin(phase) has a low-frequency term ∝ sin(phase − θ_in). Negating
        // it gives an error that is positive when the reference lags the input, which then
        // drives the frequency and phase up toward lock.
        let err = -x * phase.sin();
        self.freq =
            (self.freq + self.beta * err).clamp(self.center - self.pull, self.center + self.pull);
        self.phase += self.freq + self.alpha * err;
        if self.phase >= TAU {
            self.phase -= TAU;
        } else if self.phase < 0.0 {
            self.phase += TAU;
        }
        phase
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_to_a_pilot_tone() {
        let fs = 240_000;
        let pilot_hz = 19_000.0;
        let mut pll = Pll::new(pilot_hz, fs, 200.0, 0.707);

        // Drive with a clean pilot tone. The phase detector expects cos-phase input.
        let n = 60_000;
        let mut corr = 0.0f32;
        for i in 0..n {
            let t = i as f32 / fs as f32;
            let x = (TAU * pilot_hz * t).cos();
            let ref_phase = pll.process_sample(x);
            // After the loop has had time to settle, the reference should align with the input.
            if i > n / 2 {
                corr += x * ref_phase.cos();
            }
        }

        assert!(
            (pll.freq_hz(fs) - pilot_hz).abs() < 50.0,
            "frequency did not lock: {} Hz",
            pll.freq_hz(fs)
        );
        // Positive correlation over the settled half means the reference phase tracks the tone.
        assert!(
            corr > 0.0,
            "reference did not align with input (corr = {corr})"
        );
    }

    #[test]
    fn tracks_a_small_offset() {
        // Pilot slightly off nominal: the loop should still pull in within its range.
        let fs = 240_000;
        let mut pll = Pll::new(19_000.0, fs, 200.0, 0.707);
        let actual = 19_050.0;
        for i in 0..120_000 {
            let t = i as f32 / fs as f32;
            pll.process_sample((TAU * actual * t).cos());
        }
        assert!(
            (pll.freq_hz(fs) - actual).abs() < 30.0,
            "did not track offset: {} Hz",
            pll.freq_hz(fs)
        );
    }
}
