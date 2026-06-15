//! Decode-throughput benchmark: how fast one `Channel` turns recorded RTL-SDR IQ into RDS.
//!
//! Replays the committed 250 kS/s capture of 92.5 MHz through the same channel the scanner runs,
//! so the number tracks the realtime decode budget (samples/s vs the 250k capture rate gives the
//! per-channel realtime factor). Run with `mise run bench`.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use sdr_engine::{Channel, ChannelSpec, FileSource, Iq, Source};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/rds_thebeat_250k.cu8"
);
const RATE: u32 = 250_000;

fn load_fixture() -> Vec<Iq> {
    let mut src = FileSource::open_cu8(FIXTURE, RATE, 92_500_000).expect("open fixture");
    let mut iq = Vec::new();
    let mut buf = vec![Iq::default(); 1 << 16];
    while let Ok(n) = src.read(&mut buf) {
        if n == 0 {
            break;
        }
        iq.extend_from_slice(&buf[..n]);
    }
    iq
}

fn decode_throughput(c: &mut Criterion) {
    let iq = load_fixture();
    let mut group = c.benchmark_group("rds_decode");
    group.sample_size(10);
    group.throughput(Throughput::Elements(iq.len() as u64));
    group.bench_function("channel_250k_fixture", |b| {
        b.iter(|| {
            // Fresh channel each run so filter/loop state starts cold (worst case).
            let mut ch = Channel::new(ChannelSpec::rds(0.0), RATE);
            for chunk in iq.chunks(1 << 16) {
                black_box(ch.feed(chunk));
            }
        });
    });
    group.finish();
}

criterion_group!(benches, decode_throughput);
criterion_main!(benches);
