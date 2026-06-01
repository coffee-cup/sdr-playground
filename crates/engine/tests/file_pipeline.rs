//! Drives the whole pipeline (engine reader thread → tap) over a real `FileSource`, with no
//! hardware and no mocks. The fixture is a known complex tone with mean power ≈ 0.25.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use sdr_engine::{Engine, EngineConfig, FileSource};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../device/tests/fixtures/tone.cu8")
}

#[test]
fn reads_fixture_through_engine_and_reports_stats() {
    let bytes = std::fs::metadata(fixture()).unwrap().len() as u64;
    let expected_samples = bytes / 2;

    let source = FileSource::open_cu8(fixture(), 2_048_000, 100_000_000).unwrap();
    let engine = Engine::start(Box::new(source), EngineConfig::default());

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

    // The same fixture, transformed: a single tone must show as one dominant spectral peak,
    // and at least one frame must have been published.
    let spec = engine.spectrum();
    assert!(spec.seq > 0, "no spectrum frame published");
    assert_eq!(spec.bins_db.len(), spec.fft_size);

    let (peak_bin, peak_db) = spec
        .bins_db
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .map(|(i, &db)| (i, db))
        .unwrap();
    let mean_db = spec
        .bins_db
        .iter()
        .copied()
        .filter(|d| d.is_finite())
        .sum::<f32>()
        / spec.fft_size as f32;
    assert!(
        peak_db - mean_db > 20.0,
        "tone should stand >20 dB above the mean (peak {peak_db:.1} dB @ bin {peak_bin}, mean {mean_db:.1} dB)"
    );
}
