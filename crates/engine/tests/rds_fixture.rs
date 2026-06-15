//! Golden regression: replay a recorded RTL-SDR capture of a known station through a `Channel`
//! (the exact decode tail the scanner runs) and assert the RDS we validated live against redsea.
//!
//! The fixture is a 250 kS/s `cu8` capture of 92.5 MHz "The Beat" (Montreal), PI 0xCC24. Decoding
//! goes straight through one `Channel` rather than the full `Engine`, so it is deterministic and
//! does not depend on realtime pacing of the IQ ring.

use sdr_engine::{Channel, ChannelSpec, Event, FileSource, Iq, RdsEvent, Source};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/rds_thebeat_250k.cu8"
);
const RATE: u32 = 250_000;
const CENTER: u64 = 92_500_000;

#[test]
fn decodes_recorded_thebeat() {
    let mut src = FileSource::open_cu8(FIXTURE, RATE, CENTER).expect("open fixture");
    // The station sits at the window center (offset 0); the channel's DC blocker clears the spike.
    let mut ch = Channel::new(ChannelSpec::rds(0.0), RATE);
    let mut buf = vec![Iq::default(); 1 << 16];

    let (mut pi, mut pty, mut ps, mut rt) = (None, None, None, None);
    loop {
        let n = src.read(&mut buf).expect("read fixture");
        if n == 0 {
            break;
        }
        for ev in ch.feed(&buf[..n]) {
            match ev {
                Event::Rds(RdsEvent::Pi(v)) => pi = Some(v),
                Event::Rds(RdsEvent::ProgramType(p)) => pty = Some(p),
                Event::Rds(RdsEvent::ProgramService(s)) => ps = Some(s),
                Event::Rds(RdsEvent::RadioText(s)) => rt = Some(s),
            }
        }
    }

    println!("decoded PI={pi:04X?} PTY={pty:?} PS={ps:?} RT={rt:?}");
    assert_eq!(pi, Some(0xCC24), "PI must match the redsea-confirmed value");
    assert_eq!(
        ps.as_deref().map(str::trim),
        Some("TheBeat"),
        "PS mismatch (got {ps:?})"
    );
    assert!(rt.is_some(), "expected RadioText to decode");
}
