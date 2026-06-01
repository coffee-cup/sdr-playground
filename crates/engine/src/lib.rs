//! The runtime. Owns the reader thread that pulls IQ from a [`Source`], computes live
//! signal stats, and publishes them through a lock-free tap. UI-agnostic, with the source
//! injected — `app` and `cli` are interchangeable consumers. See `docs/ARCHITECTURE.md`.
//!
//! This is the realtime-core foundation: the reader thread is the producer future stages
//! (FFT, channels, decoders) attach to. It uses plain threads, not async — the orchestration
//! layer (tokio) arrives with decoders and the event bus.

mod snapshot;

use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use sdr_core::Source;
use sdr_device::rtlsdr::DEFAULT_READ_SAMPLES;

pub use snapshot::Snapshot;

// Re-exported so front-ends that depend only on `engine` can construct sources to inject.
pub use sdr_device::{DeviceInfo, FileSource, Gain, RtlConfig, RtlSdrSource};

/// How often the reader thread publishes a fresh snapshot (~20 Hz).
const PUBLISH_INTERVAL: Duration = Duration::from_millis(50);

enum Ctrl {
    Tune(u64),
    Stop,
}

/// A running pipeline over a single source.
pub struct Engine {
    tap: Arc<ArcSwap<Snapshot>>,
    ctrl: Sender<Ctrl>,
    handle: Option<JoinHandle<()>>,
}

impl Engine {
    /// Start reading from `source` on a dedicated thread.
    pub fn start(source: Box<dyn Source>) -> Engine {
        let tap = Arc::new(ArcSwap::from_pointee(Snapshot::initial(
            source.center_freq(),
            source.sample_rate(),
        )));
        let (ctrl, rx) = mpsc::channel();

        let reader_tap = Arc::clone(&tap);
        let handle = thread::Builder::new()
            .name("sdr-reader".into())
            .spawn(move || reader_loop(source, reader_tap, rx))
            .expect("spawn reader thread");

        Engine {
            tap,
            ctrl,
            handle: Some(handle),
        }
    }

    /// The latest snapshot (lock-free).
    pub fn snapshot(&self) -> Snapshot {
        **self.tap.load()
    }

    /// Request a retune. Applied by the reader thread between reads.
    pub fn tune(&self, hz: u64) {
        let _ = self.ctrl.send(Ctrl::Tune(hz));
    }

    /// Stop the reader thread and wait for it to finish.
    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.ctrl.send(Ctrl::Stop);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn reader_loop(mut source: Box<dyn Source>, tap: Arc<ArcSwap<Snapshot>>, rx: mpsc::Receiver<Ctrl>) {
    let mut scratch = vec![sdr_core::Iq::default(); DEFAULT_READ_SAMPLES];
    let mut total: u64 = 0;

    // Stats accumulate over a publish window, then reset.
    let mut window_count: u64 = 0;
    let mut window_power_sum: f64 = 0.0;
    let mut window_peak: f32 = 0.0;
    let mut last_publish = Instant::now();

    loop {
        match rx.try_recv() {
            Ok(Ctrl::Tune(hz)) => {
                if let Err(e) = source.tune(hz) {
                    eprintln!("tune failed: {e}");
                }
            }
            Ok(Ctrl::Stop) | Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        let n = match source.read(&mut scratch) {
            Ok(0) => {
                let mean = (window_power_sum / window_count.max(1) as f64) as f32;
                publish(&tap, source.as_ref(), total, 0.0, mean, window_peak, false);
                break;
            }
            Ok(n) => n,
            Err(e) => {
                eprintln!("source read failed: {e}");
                let mean = (window_power_sum / window_count.max(1) as f64) as f32;
                publish(&tap, source.as_ref(), total, 0.0, mean, window_peak, false);
                break;
            }
        };

        total += n as u64;
        window_count += n as u64;
        for s in &scratch[..n] {
            let p = s.re * s.re + s.im * s.im;
            window_power_sum += p as f64;
            window_peak = window_peak.max(p);
        }

        let elapsed = last_publish.elapsed();
        if elapsed >= PUBLISH_INTERVAL {
            let throughput = window_count as f64 / elapsed.as_secs_f64();
            let mean_power = (window_power_sum / window_count.max(1) as f64) as f32;
            publish(
                &tap,
                source.as_ref(),
                total,
                throughput,
                mean_power,
                window_peak,
                true,
            );
            window_count = 0;
            window_power_sum = 0.0;
            window_peak = 0.0;
            last_publish = Instant::now();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn publish(
    tap: &ArcSwap<Snapshot>,
    source: &dyn Source,
    total: u64,
    throughput: f64,
    mean_power: f32,
    peak_power: f32,
    running: bool,
) {
    tap.store(Arc::new(Snapshot {
        center_freq: source.center_freq(),
        sample_rate: source.sample_rate(),
        total_samples: total,
        throughput_sps: throughput,
        mean_power,
        peak_power,
        mean_dbfs: snapshot::to_dbfs(mean_power),
        peak_dbfs: snapshot::to_dbfs(peak_power),
        running,
    }));
}
