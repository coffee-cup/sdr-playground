//! Hardware smoke test. Ignored by default so CI (no device) skips it; run locally with:
//!     cargo test -p sdr-device -- --ignored

use sdr_core::{Iq, Source};
use sdr_device::{rtlsdr::DEFAULT_READ_SAMPLES, Gain, RtlConfig, RtlSdrSource};

#[test]
#[ignore = "requires RTL-SDR hardware"]
fn opens_and_reads_real_iq() {
    let cfg = RtlConfig {
        freq_hz: 100_000_000,
        sample_rate: 2_048_000,
        gain: Gain::Auto,
    };
    let mut source = RtlSdrSource::open(0, cfg).expect("open RTL-SDR at index 0");
    assert_eq!(source.sample_rate(), 2_048_000);
    assert_eq!(source.center_freq(), 100_000_000);

    let mut out = vec![Iq::default(); DEFAULT_READ_SAMPLES];
    // The first transfer after reset can be short; read a few buffers.
    let mut total = 0;
    let mut power_sum = 0.0f64;
    for _ in 0..8 {
        let n = source.read(&mut out).expect("read IQ");
        total += n;
        for s in &out[..n] {
            power_sum += (s.re * s.re + s.im * s.im) as f64;
        }
    }

    assert!(total > 0, "no samples read from device");
    let mean_power = power_sum / total as f64;
    assert!(
        mean_power > 0.0,
        "mean power was zero — device returned only DC/silence"
    );
}
