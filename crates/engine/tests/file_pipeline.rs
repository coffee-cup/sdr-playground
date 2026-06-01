//! Drives the whole pipeline (engine reader thread → tap) over a real `FileSource`, with no
//! hardware and no mocks. The fixture is a known complex tone with mean power ≈ 0.25.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use sdr_engine::{Engine, FileSource};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../device/tests/fixtures/tone.cu8")
}

#[test]
fn reads_fixture_through_engine_and_reports_stats() {
    let bytes = std::fs::metadata(fixture()).unwrap().len() as u64;
    let expected_samples = bytes / 2;

    let source = FileSource::open_cu8(fixture(), 2_048_000, 100_000_000).unwrap();
    let engine = Engine::start(Box::new(source));

    // The file drains quickly; wait for the reader thread to reach EOF.
    let deadline = Instant::now() + Duration::from_secs(2);
    let snap = loop {
        let s = engine.snapshot();
        if !s.running {
            break s;
        }
        assert!(Instant::now() < deadline, "reader did not finish in time");
        std::thread::sleep(Duration::from_millis(1));
    };

    assert_eq!(snap.total_samples, expected_samples);
    assert_eq!(snap.center_freq, 100_000_000);
    assert_eq!(snap.sample_rate, 2_048_000);
    // Tone amplitude 0.5 → constant power 0.25.
    assert!(
        (snap.mean_power - 0.25).abs() < 0.01,
        "mean_power = {}",
        snap.mean_power
    );
}
