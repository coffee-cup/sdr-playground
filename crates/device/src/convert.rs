//! RTL-SDR sample conversion.
//!
//! The RTL2832U delivers interleaved unsigned 8-bit samples (I, Q, I, Q, …) with the ADC
//! midpoint — DC — at 127.5. We normalize so full scale maps to ±1.0.

use sdr_core::Iq;

/// The unsigned-8 value corresponding to zero signal (ADC midpoint).
pub const DC_OFFSET: f32 = 127.5;

/// Scale that maps the [0, 255] range to roughly [-1.0, 1.0].
pub const SCALE: f32 = 1.0 / 127.5;

/// Convert interleaved unsigned-8 IQ bytes into complex samples.
///
/// Writes `min(bytes.len() / 2, out.len())` samples and returns that count. A trailing odd
/// byte (incomplete I/Q pair) is ignored.
pub fn cu8_to_iq(bytes: &[u8], out: &mut [Iq]) -> usize {
    let n = (bytes.len() / 2).min(out.len());
    for (sample, pair) in out[..n].iter_mut().zip(bytes.chunks_exact(2)) {
        let i = (pair[0] as f32 - DC_OFFSET) * SCALE;
        let q = (pair[1] as f32 - DC_OFFSET) * SCALE;
        *sample = Iq::new(i, q);
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midscale_is_near_zero() {
        let mut out = [Iq::default(); 2];
        // 127 and 128 straddle the 127.5 DC midpoint.
        let n = cu8_to_iq(&[127, 128, 128, 127], &mut out);
        assert_eq!(n, 2);
        for s in &out {
            assert!(s.re.abs() < 0.01, "re = {}", s.re);
            assert!(s.im.abs() < 0.01, "im = {}", s.im);
        }
    }

    #[test]
    fn extremes_map_into_unit_range() {
        let mut out = [Iq::default(); 2];
        cu8_to_iq(&[0, 0, 255, 255], &mut out);
        assert!((out[0].re - -1.0).abs() < 0.01);
        assert!((out[0].im - -1.0).abs() < 0.01);
        assert!((out[1].re - 1.0).abs() < 0.01);
        assert!((out[1].im - 1.0).abs() < 0.01);
        for s in &out {
            assert!((-1.0..=1.0).contains(&s.re));
            assert!((-1.0..=1.0).contains(&s.im));
        }
    }

    #[test]
    fn interleave_order_is_i_then_q() {
        let mut out = [Iq::default(); 1];
        // I = 255 (≈ +1), Q = 0 (≈ -1): catches a swapped I/Q.
        cu8_to_iq(&[255, 0], &mut out);
        assert!(out[0].re > 0.5, "re = {}", out[0].re);
        assert!(out[0].im < -0.5, "im = {}", out[0].im);
    }

    #[test]
    fn truncates_to_shorter_side_and_ignores_odd_tail() {
        let mut out = [Iq::default(); 1];
        // Three pairs of input, room for one sample.
        assert_eq!(cu8_to_iq(&[0, 0, 255, 255, 127, 127], &mut out), 1);

        let mut out = [Iq::default(); 4];
        // Five bytes => two whole pairs, trailing byte ignored.
        assert_eq!(cu8_to_iq(&[1, 2, 3, 4, 5], &mut out), 2);
    }
}
