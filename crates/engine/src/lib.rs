//! The runtime. Owns the reader thread that pulls IQ from a [`Source`], computes live signal
//! stats and a wideband spectrum, and publishes them through lock-free taps. UI-agnostic, with
//! the source injected — `app` and `cli` are interchangeable consumers. See `docs/ARCHITECTURE.md`.
//!
//! This is the realtime-core foundation: the reader thread is the producer. It uses plain
//! threads, not async — the orchestration layer (tokio) arrives with decoders and the event
//! bus. The wideband FFT runs inline on the reader thread for now; it is the only consumer of
//! the stream, so the broadcast fan-out and DSP pool are deliberately deferred until the first
//! Channel needs them (see ARCHITECTURE "Concurrency model").

mod channel;
mod frame;
mod snapshot;

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use sdr_device::rtlsdr::DEFAULT_READ_SAMPLES;
use sdr_dsp::SpectrumAnalyzer;

pub use channel::{Channel, ChannelSpec};
pub use frame::{SpectrumFrame, Tap, WaveformFrame};
pub use snapshot::Snapshot;

// Re-exported so front-ends that depend only on `engine` can construct sources and configure
// the pipeline.
pub use sdr_core::{Iq, Source};
pub use sdr_decode::{pty_name, Decoder, Event, RdsEvent};
pub use sdr_device::convert::iq_to_cu8;
pub use sdr_device::{DeviceInfo, FileSource, Gain, RtlConfig, RtlSdrSource};
pub use sdr_dsp::{SpectrumConfig, WindowKind};

/// IQ ring-buffer capacity between the realtime reader and the decode worker (~0.2 s at
/// 2.4 MS/s). On overflow the reader drops samples rather than block.
const IQ_RING_CAP: usize = 1 << 19;

/// A decoded event tagged with the channel that produced it. The offset is relative to the
/// tuned center, so a front-end that knows the center maps it to an absolute frequency.
#[derive(Debug, Clone)]
pub struct ChannelEvent {
    pub offset_hz: f64,
    pub event: Event,
}

/// How often the reader thread publishes fresh taps (~30 Hz). Also the waterfall's row rate.
const PUBLISH_INTERVAL: Duration = Duration::from_millis(33);

/// Clamp a requested frame rate to a sane range and convert it to a publish interval.
fn fps_to_interval(fps: f32) -> Duration {
    Duration::from_secs_f32(1.0 / fps.clamp(1.0, 120.0))
}

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
    /// Swap the spectrum analyzer live (FFT size and/or window) without reopening the device.
    SetSpectrum(SpectrumConfig),
    /// Change the publish/waterfall-row rate in frames per second.
    SetFps(f32),
    Stop,
}

/// A running pipeline over a single source.
pub struct Engine {
    snapshot: Tap<Snapshot>,
    spectrum: Tap<SpectrumFrame>,
    waveform: Tap<WaveformFrame>,
    ctrl: Sender<Ctrl>,
    handle: Option<JoinHandle<()>>,
    /// Replace the active decode channels (sent to the decode worker). `Option` so `Drop` can
    /// disconnect it before joining the worker, which is the worker's exit signal.
    channels: Option<Sender<Vec<ChannelSpec>>>,
    /// Decoded events from the worker, drained by the front-end. The `Mutex` keeps `Engine`
    /// `Sync` (an `mpsc::Receiver` is not) so front-ends can hold it in an `Arc`.
    events: Mutex<Receiver<ChannelEvent>>,
    worker: Option<JoinHandle<()>>,
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

        // Realtime reader -> decode worker handoff: a lock-free IQ ring and an event bus back.
        let (iq_tx, iq_rx) = rtrb::RingBuffer::<Iq>::new(IQ_RING_CAP);
        let (events_tx, events_rx) = mpsc::channel();
        let (channels_tx, channels_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("sdr-decode".into())
            .spawn(move || decode_loop(iq_rx, rate, channels_rx, events_tx))
            .expect("spawn decode worker");

        let taps = Taps {
            snapshot: Arc::clone(&snapshot),
            spectrum: Arc::clone(&spectrum),
            waveform: Arc::clone(&waveform),
        };
        let handle = thread::Builder::new()
            .name("sdr-reader".into())
            .spawn(move || reader_loop(source, taps, config, rx, iq_tx))
            .expect("spawn reader thread");

        Engine {
            snapshot,
            spectrum,
            waveform,
            ctrl,
            handle: Some(handle),
            channels: Some(channels_tx),
            events: Mutex::new(events_rx),
            worker: Some(worker),
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

    /// Reconfigure the spectrum analyzer (FFT size, window) live. The reader rebuilds it between
    /// reads; the device stream is uninterrupted.
    pub fn set_spectrum(&self, config: SpectrumConfig) {
        let _ = self.ctrl.send(Ctrl::SetSpectrum(config));
    }

    /// Set the publish/waterfall-row rate in frames per second.
    pub fn set_fps(&self, fps: f32) {
        let _ = self.ctrl.send(Ctrl::SetFps(fps));
    }

    /// Replace the set of decode channels. The worker rebuilds them and flushes stale IQ, so a
    /// front-end retunes then calls this with the new window's channels.
    pub fn set_channels(&self, specs: Vec<ChannelSpec>) {
        if let Some(channels) = &self.channels {
            let _ = channels.send(specs);
        }
    }

    /// Drain all decoded events produced since the last call.
    pub fn drain_events(&self) -> Vec<ChannelEvent> {
        self.events.lock().unwrap().try_iter().collect()
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
        // Disconnect the channel sender so the worker sees `Disconnected` and exits, then join it.
        self.channels.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// The decode worker: drains IQ from the realtime reader, runs each active channel, and forwards
/// decoded events. Lives in the orchestration tier (a plain thread, off the realtime path).
fn decode_loop(
    mut iq_rx: rtrb::Consumer<Iq>,
    input_rate: u32,
    channels_rx: Receiver<Vec<ChannelSpec>>,
    events_tx: Sender<ChannelEvent>,
) {
    let mut channels: Vec<Channel> = Vec::new();
    let mut block: Vec<Iq> = Vec::new();
    loop {
        match channels_rx.try_recv() {
            Ok(specs) => {
                channels = specs
                    .into_iter()
                    .map(|s| Channel::new(s, input_rate))
                    .collect();
                // Discard IQ captured before the retune that accompanies a channel change.
                while iq_rx.pop().is_ok() {}
            }
            Err(TryRecvError::Disconnected) => break,
            Err(TryRecvError::Empty) => {}
        }

        let avail = iq_rx.slots();
        if avail == 0 {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        if let Ok(chunk) = iq_rx.read_chunk(avail) {
            let (a, b) = chunk.as_slices();
            block.clear();
            block.extend_from_slice(a);
            block.extend_from_slice(b);
            chunk.commit_all();
        }
        for ch in &mut channels {
            let offset_hz = ch.offset_hz();
            for event in ch.feed(&block) {
                if events_tx.send(ChannelEvent { offset_hz, event }).is_err() {
                    return;
                }
            }
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
    mut iq_tx: rtrb::Producer<Iq>,
) {
    // The configured waveform length is the ceiling; the live value is re-clamped whenever the
    // FFT size changes (a block is never shorter than the slice the waveform tap exposes).
    let waveform_target = config.waveform_samples;
    let mut fft_size = config.spectrum.fft_size;
    let mut waveform_samples = waveform_target.min(fft_size);
    let mut analyzer = SpectrumAnalyzer::new(config.spectrum);
    let mut publish_interval = PUBLISH_INTERVAL;

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
            Ok(Ctrl::SetSpectrum(spectrum)) => {
                fft_size = spectrum.fft_size;
                waveform_samples = waveform_target.min(fft_size);
                analyzer = SpectrumAnalyzer::new(spectrum);
                block.resize(fft_size, Iq::default());
                last_block.resize(fft_size, Iq::default());
                block_pos = 0;
            }
            Ok(Ctrl::SetFps(fps)) => publish_interval = fps_to_interval(fps),
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

        // Hand the raw IQ to the decode worker (best-effort: drop on ring overflow).
        let want = n.min(iq_tx.slots());
        if want > 0 {
            if let Ok(chunk) = iq_tx.write_chunk_uninit(want) {
                chunk.fill_from_iter(scratch[..want].iter().copied());
            }
        }

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
        if elapsed >= publish_interval {
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
    let (tune_min, tune_max) = source.tune_range();
    tap.store(Arc::new(Snapshot {
        center_freq: source.center_freq(),
        sample_rate: source.sample_rate(),
        total_samples: total,
        throughput_sps: throughput,
        mean_power,
        peak_power,
        mean_dbfs: snapshot::to_dbfs(mean_power),
        peak_dbfs: snapshot::to_dbfs(peak_power),
        tune_min,
        tune_max,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    /// An endless source of constant samples, so the reader keeps running while we reconfigure it.
    struct EndlessSource {
        rate: u32,
        freq: u64,
    }

    impl Source for EndlessSource {
        fn sample_rate(&self) -> u32 {
            self.rate
        }
        fn center_freq(&self) -> u64 {
            self.freq
        }
        fn tune(&mut self, hz: u64) -> sdr_core::Result<()> {
            self.freq = hz;
            Ok(())
        }
        fn read(&mut self, out: &mut [Iq]) -> sdr_core::Result<usize> {
            out.fill(Iq::new(0.5, 0.0));
            Ok(out.len())
        }
    }

    /// Wait for a published spectrum frame whose `fft_size` matches `want`, or panic on timeout.
    fn wait_for_fft_size(engine: &Engine, want: usize) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let spec = engine.spectrum();
            if spec.seq > 0 && spec.fft_size == want {
                return spec.seq;
            }
            assert!(
                Instant::now() < deadline,
                "no frame with fft_size {want} (saw {})",
                spec.fft_size
            );
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn set_spectrum_reconfigures_fft_size_live() {
        let source = EndlessSource {
            rate: 2_048_000,
            freq: 100_000_000,
        };
        let engine = Engine::start(
            Box::new(source),
            EngineConfig {
                spectrum: SpectrumConfig {
                    fft_size: 8192,
                    window: WindowKind::Hann,
                },
                waveform_samples: 1024,
            },
        );

        wait_for_fft_size(&engine, 8192);

        engine.set_spectrum(SpectrumConfig {
            fft_size: 2048,
            window: WindowKind::Hann,
        });
        let seq = wait_for_fft_size(&engine, 2048);
        assert!(seq > 0);

        engine.stop();
    }
}
