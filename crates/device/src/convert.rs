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

/// Convert complex samples back into interleaved unsigned-8 IQ bytes, the inverse of
/// [`cu8_to_iq`]. Used to record a live IQ stream to a replayable `cu8` file.
///
/// Writes `min(samples.len(), out.len() / 2)` samples (two bytes each) and returns the byte
/// count. Values outside ±1.0 saturate to the 8-bit range rather than wrap.
pub fn iq_to_cu8(samples: &[Iq], out: &mut [u8]) -> usize {
    let n = samples.len().min(out.len() / 2);
    for (pair, s) in out.chunks_exact_mut(2).zip(&samples[..n]) {
        pair[0] = to_u8(s.re);
        pair[1] = to_u8(s.im);
    }
    n * 2
}

/// Map a normalized sample back to the [0, 255] ADC range, saturating at the ends.
fn to_u8(v: f32) -> u8 {
    (v / SCALE + DC_OFFSET).round().clamp(0.0, 255.0) as u8
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
    fn iq_to_cu8_round_trips_through_cu8_to_iq() {
        // Original bytes survive a cu8 -> iq -> cu8 round trip (rounding is exact at integers).
        let bytes = [0u8, 64, 127, 128, 200, 255];
        let mut iq = [Iq::default(); 3];
        cu8_to_iq(&bytes, &mut iq);
        let mut back = [0u8; 6];
        assert_eq!(iq_to_cu8(&iq, &mut back), 6);
        assert_eq!(back, bytes);
    }

    #[test]
    fn iq_to_cu8_saturates_out_of_range() {
        let mut out = [0u8; 4];
        assert_eq!(
            iq_to_cu8(&[Iq::new(-2.0, 2.0), Iq::new(0.0, 0.0)], &mut out),
            4
        );
        assert_eq!(out[0], 0); // re clamps to floor
        assert_eq!(out[1], 255); // im clamps to ceil
        assert_eq!(out[2], 128); // 0.0 -> midpoint (127.5 rounds to 128)
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
