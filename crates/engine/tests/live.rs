//! Live validation against a plugged-in RTL-SDR. Ignored by default (needs hardware and runs in
//! realtime); run with `mise run validate-live`. Opens the device, decodes RDS from one station
//! through the full engine, and asserts a PI code came out, exercising the entire live pipeline
//! (USB reader -> IQ ring -> decode worker -> channel -> event bus). If no device is present it
//! prints a skip notice and passes, so the task is safe to run anywhere.
//!
//! Defaults to 92.5 MHz (Montreal "The Beat", PI 0xCC24). Override the station with the
//! `SDR_LIVE_FREQ_HZ` environment variable, e.g. `SDR_LIVE_FREQ_HZ=98500000`.

use std::time::{Duration, Instant};

use sdr_engine::{
    ChannelSpec, Engine, EngineConfig, Event, Gain, RdsEvent, RtlConfig, RtlSdrSource,
};

#[test]
#[ignore = "requires a plugged-in RTL-SDR"]
fn decodes_a_live_station() {
    let freq: u64 = std::env::var("SDR_LIVE_FREQ_HZ")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(92_500_000);

    // Offset-tune like the scanner: place the station 250 kHz off the window center, away from
    // the RTL-SDR DC spike, and decode it through one RDS channel at that offset.
    let offset = 250_000.0;
    let center = freq.saturating_sub(offset as u64);
    let cfg = RtlConfig {
        freq_hz: center,
        sample_rate: 1_024_000,
        gain: Gain::Manual(400),
    };
    let source = match RtlSdrSource::open(0, cfg) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("SKIP: no RTL-SDR available ({e})");
            return;
        }
    };

    let engine = Engine::start(Box::new(source), EngineConfig::default());
    engine.tune(center);
    engine.set_channels(vec![ChannelSpec::rds(offset)]);

    let deadline = Instant::now() + Duration::from_secs(25);
    let mut pi = None;
    let mut ps = None;
    while Instant::now() < deadline && pi.is_none() {
        for ce in engine.drain_events() {
            match ce.event {
                Event::Rds(RdsEvent::Pi(v)) => pi = Some(v),
                Event::Rds(RdsEvent::ProgramService(s)) => ps = Some(s),
                _ => {}
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    eprintln!(
        "live {:.3} MHz -> PI={pi:04X?} PS={ps:?}",
        freq as f64 / 1e6
    );
    assert!(
        pi.is_some(),
        "no RDS decoded at {:.3} MHz in 25s (weak signal, no RDS, or wrong station)",
        freq as f64 / 1e6
    );
}
