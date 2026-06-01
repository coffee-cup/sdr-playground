//! The runtime. Owns the reader thread that pulls IQ from a [`Source`], computes live signal
//! stats and a wideband spectrum, and publishes them through lock-free taps. UI-agnostic, with
//! the source injected — `app` and `cli` are interchangeable consumers. See `docs/ARCHITECTURE.md`.
//!
//! This is the realtime-core foundation: the reader thread is the producer. It uses plain
//! threads, not async — the orchestration layer (tokio) arrives with decoders and the event
//! bus. The wideband FFT runs inline on the reader thread for now; it is the only consumer of
//! the stream, so the broadcast fan-out and DSP pool are deliberately deferred until the first
//! Channel needs them (see ARCHITECTURE "Concurrency model").

mod frame;
mod snapshot;

use std::sync::mpsc::{self, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use sdr_device::rtlsdr::DEFAULT_READ_SAMPLES;
use sdr_dsp::SpectrumAnalyzer;

pub use frame::{SpectrumFrame, Tap, WaveformFrame};
pub use snapshot::Snapshot;

// Re-exported so front-ends that depend only on `engine` can construct sources and configure
// the pipeline.
pub use sdr_core::{Iq, Source};
pub use sdr_device::{DeviceInfo, FileSource, Gain, RtlConfig, RtlSdrSource};
pub use sdr_dsp::{SpectrumConfig, WindowKind};

/// How often the reader thread publishes fresh taps (~30 Hz). Also the waterfall's row rate.
const PUBLISH_INTERVAL: Duration = Duration::from_millis(33);

/// Pipeline configuration. Sane defaults; the surface exists so the UI and CLI can tune it
/// later without reshaping the engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub spectrum: SpectrumConfig,
    /// How many recent IQ samples the waveform tap exposes.
    pub waveform_samples: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            spectrum: SpectrumConfig::default(),
            waveform_samples: 1024,
        }
    }
}

enum Ctrl {
    Tune(u64),
    Stop,
}

/// A running pipeline over a single source.
pub struct Engine {
    snapshot: Tap<Snapshot>,
    spectrum: Tap<SpectrumFrame>,
    waveform: Tap<WaveformFrame>,
    ctrl: Sender<Ctrl>,
    handle: Option<JoinHandle<()>>,
}

impl Engine {
    /// Start reading from `source` on a dedicated thread.
    pub fn start(source: Box<dyn Source>, config: EngineConfig) -> Engine {
        let (center, rate) = (source.center_freq(), source.sample_rate());
        let snapshot = Arc::new(ArcSwap::from_pointee(Snapshot::initial(center, rate)));
        let spectrum = Arc::new(ArcSwap::from_pointee(SpectrumFrame::initial(
            config.spectrum.fft_size,
            center,
            rate,
        )));
        let waveform = Arc::new(ArcSwap::from_pointee(WaveformFrame::initial(rate)));
        let (ctrl, rx) = mpsc::channel();

        let taps = Taps {
            snapshot: Arc::clone(&snapshot),
            spectrum: Arc::clone(&spectrum),
            waveform: Arc::clone(&waveform),
        };
        let handle = thread::Builder::new()
            .name("sdr-reader".into())
            .spawn(move || reader_loop(source, taps, config, rx))
            .expect("spawn reader thread");

        Engine {
            snapshot,
            spectrum,
            waveform,
            ctrl,
            handle: Some(handle),
        }
    }

    /// The latest scalar stats (lock-free).
    pub fn snapshot(&self) -> Snapshot {
        **self.snapshot.load()
    }

    /// The latest wideband spectrum frame (lock-free).
    pub fn spectrum(&self) -> Arc<SpectrumFrame> {
        self.spectrum.load_full()
    }

    /// The latest time-domain waveform frame (lock-free).
    pub fn waveform(&self) -> Arc<WaveformFrame> {
        self.waveform.load_full()
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

/// The reader thread's handles to the published taps.
struct Taps {
    snapshot: Tap<Snapshot>,
    spectrum: Tap<SpectrumFrame>,
    waveform: Tap<WaveformFrame>,
}

fn reader_loop(
    mut source: Box<dyn Source>,
    taps: Taps,
    config: EngineConfig,
    rx: mpsc::Receiver<Ctrl>,
) {
    let fft_size = config.spectrum.fft_size;
    let waveform_samples = config.waveform_samples.min(fft_size);
    let mut analyzer = SpectrumAnalyzer::new(config.spectrum);

    let mut scratch = vec![Iq::default(); DEFAULT_READ_SAMPLES];

    // Incoming samples are gathered into `fft_size` blocks; each completed block is folded into
    // the analyzer's running average, and the most recent one feeds the waveform tap. Averaging
    // every block in a publish window (Welch's method) is what keeps the waterfall clean.
    let mut block = vec![Iq::default(); fft_size];
    let mut block_pos = 0usize;
    let mut last_block = vec![Iq::default(); fft_size];

    let mut total: u64 = 0;
    let mut seq: u64 = 0;

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
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                eprintln!("source read failed: {e}");
                break;
            }
        };

        total += n as u64;
        window_count += n as u64;
        for &s in &scratch[..n] {
            window_power_sum += (s.re * s.re + s.im * s.im) as f64;
            window_peak = window_peak.max(s.re * s.re + s.im * s.im);
            block[block_pos] = s;
            block_pos += 1;
            if block_pos == fft_size {
                analyzer.accumulate(&block);
                last_block.copy_from_slice(&block);
                block_pos = 0;
            }
        }

        let elapsed = last_publish.elapsed();
        if elapsed >= PUBLISH_INTERVAL {
            let throughput = window_count as f64 / elapsed.as_secs_f64();
            let mean_power = (window_power_sum / window_count.max(1) as f64) as f32;
            publish_snapshot(
                &taps.snapshot,
                source.as_ref(),
                total,
                throughput,
                mean_power,
                window_peak,
                true,
            );
            if analyzer.pending() {
                seq += 1;
                publish_frames(
                    &taps,
                    &mut analyzer,
                    &last_block,
                    waveform_samples,
                    source.as_ref(),
                    seq,
                );
            }
            window_count = 0;
            window_power_sum = 0.0;
            window_peak = 0.0;
            last_publish = Instant::now();
        }
    }

    // Final flush. Emit one last frame if any block accumulated (the path a short file takes —
    // it drains before the first publish tick), then the stopped snapshot last: consumers wait
    // on `running == false`, so everything else must be visible before it.
    if analyzer.pending() {
        seq += 1;
        publish_frames(
            &taps,
            &mut analyzer,
            &last_block,
            waveform_samples,
            source.as_ref(),
            seq,
        );
    }
    let mean_power = (window_power_sum / window_count.max(1) as f64) as f32;
    publish_snapshot(
        &taps.snapshot,
        source.as_ref(),
        total,
        0.0,
        mean_power,
        window_peak,
        false,
    );
}

#[allow(clippy::too_many_arguments)]
fn publish_snapshot(
    tap: &Tap<Snapshot>,
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

fn publish_frames(
    taps: &Taps,
    analyzer: &mut SpectrumAnalyzer,
    last_block: &[Iq],
    waveform_samples: usize,
    source: &dyn Source,
    seq: u64,
) {
    let bins = analyzer.finish();
    taps.spectrum.store(Arc::new(SpectrumFrame {
        bins_db: bins.to_vec().into_boxed_slice(),
        fft_size: bins.len(),
        center_freq: source.center_freq(),
        sample_rate: source.sample_rate(),
        seq,
    }));

    let recent = &last_block[last_block.len() - waveform_samples..];
    taps.waveform.store(Arc::new(WaveformFrame {
        samples: recent.to_vec().into_boxed_slice(),
        sample_rate: source.sample_rate(),
        seq,
    }));
}
