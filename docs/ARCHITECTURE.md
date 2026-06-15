# Architecture

This document describes the high-level structure: how signal flows through the system, how concurrency is split, and how the workspace is organized. It covers shape and boundaries rather than files. Implementation lives in code; design decisions live here.

The design serves three properties carried over from the vision: the pipeline is the core abstraction, every stage is observable, and the realtime path never stalls. Most of what follows derives from those three constraints.

---

## System layers

The system separates into two layers with different timing requirements.

The **realtime core** (device, DSP, audio) operates under hard deadlines. A late audio buffer produces an audible glitch, so this layer uses dedicated threads and lock-free ring buffers, with no async runtime and no allocation in the hot loop, to avoid scheduling jitter.

The **orchestration layer** (channel lifecycle, decoders, recording, event bus, UI) is best-effort and IO-bound. It runs on tokio.

The boundary between the two layers is a set of lock-free ring buffers. The realtime core produces into them and never waits on the layer above. The orchestration layer consumes snapshots and never reaches into the hot loop. Data flows upward; control messages (tune, start a channel) flow downward through message queues rather than shared mutable state.

```
        ┌──────────────── realtime core (threads, no async) ─────────────┐
        │  reader thread:                                                 │
 Source ─→ read ─┬─→ wideband FFT ─→ spectrum / waveform / snapshot taps  │
 (device         │                                                        │
  or file)       └─→ IQ ring (rtrb, lock-free) ──────────┐                │
        └──────────────────────────────────────────────┬─┘               │
                          taps (lock-free snapshots)    │ raw IQ          │
        ┌──────────────────────────┴──────── orchestration (worker thread)┘
        │  decode worker:  drain IQ ring ─→ Channel[i] (NCO → decimate
        │                  → FM demod → Decoder) ─→ event bus (mpsc)
        ├─→ UI: GPUI render loop pulling tap snapshots          (app)
        └─→ station table ← drain_events                        (scanner)
```

---

## Signal pipeline

All processing originates from a single stream of raw IQ from the `Source`. The reader thread computes the wideband FFT inline and pushes the same IQ into a lock-free ring; the decode worker drains that ring and runs every active `Channel` over each block. A slow worker drops samples from the ring rather than back-pressuring the device: a dropped sample in a visualization or a marginal decode is acceptable; blocking the device read is not.

A `Channel` is the unit of work:

```
tune (NCO freq shift to baseband) → low-pass + decimate → FM demodulate → decoder
```

Each stage is cheap and composable. Tuning is a complex multiply by the station's offset from the window center, which also moves the station off the RTL-SDR DC spike (a one-pole DC blocker mops up the rest). Decimation discards unused bandwidth: the decoder runs on a ~240 kHz multiplex rather than the full capture rate. The decoder tail consumes the demodulated multiplex and emits events, decoupled from the realtime path.

Scanning the whole FM band uses this: a single RTL-SDR sees ~1-2 MHz at once, so the scanner plans a set of windows covering 88–108 MHz, and for each window registers one channel per station in view, dwells adaptively (leaving a window once it stops yielding new RDS, so empty stretches of the band are abandoned in a couple of seconds instead of burning a fixed dwell), then retunes to the next window.

---

## Observability: stage taps

Stage-level observability is an architectural property, not a UI feature. Every stage publishes its most recent output through a tap: a lock-free, single-slot snapshot that the UI reads at frame rate. Raw IQ, post-filter baseband, demodulated waveform, and recovered bits are each observable without perturbing the signal path. The producer overwrites the slot, the reader takes the latest value, and a missed frame has no consequence.

This addresses a common failure mode in tools such as rtl_433, where a failed decode produces no output and no diagnostic information. With per-stage taps, a failed decode can be traced up the chain to determine whether the signal was absent, the filter was misplaced, the demodulator was misconfigured, or the decoder itself failed.

The tap mechanism and the decoder input mechanism are the same subscription primitive. A stage exposes its output; the UI subscribes to observe it, and a decoder subscribes to consume it. One mechanism serves both cases.

Concretely, a tap is a single-slot `Arc<ArcSwap<T>>`: the producer stores, a reader loads the latest. Scalar stats are a small `Copy` `Snapshot`; array-valued stages (the wideband `SpectrumFrame`, the time-domain `WaveformFrame`) carry a buffer behind the `Arc` and a monotonic `seq` so a reader can tell whether the frame advanced. The engine publishes all three; `app` reads them at frame rate and `cli` renders them headless.

---

## Source abstraction and record/replay

A `Source` produces raw IQ. Live hardware and recorded files implement the same trait, so the pipeline is identical whether samples come from the antenna or from disk. Running a saved signal as if live is not a separate mode; it is a file `Source` that paces its reads to the sample rate.

Recording is the inverse operation. Today `cli record` opens a `Source` and streams raw `cu8` to disk (the drop-free reader thread makes this lossless even at high rates). A pre-trigger ring (continuously overwriting the last N seconds, snapshotted on a trigger to capture transients) is the planned upgrade.

Saved files serve as test fixtures. A signal is recorded once and committed; a test replays it through the real pipeline and asserts on the resulting events. The DSP is not mocked, because the fixture is the exact input the hardware would have produced.

This makes the `Source` trait the most important boundary in the system. With it defined correctly, live operation, replay, waterfall scrubbing, and testing share a single code path.

---

## Concurrency model

**Realtime core (dedicated threads, no async):**

- The RTL-SDR `Source` runs its own reader thread that does nothing but `read_sync` into a queue. `rtl-sdr-rs` exposes only synchronous bulk reads, and any work between reads lets the device FIFO overflow and drop samples (which leaves the spectrum intact but corrupts data like RDS). A dedicated tight read loop keeps the device drained; `Source::read` drains the queue, and retunes are applied between reads via an atomic.
- The engine's reader thread loops on `source.read()`, computes the wideband FFT inline (publishing the spectrum/waveform taps), and pushes the raw IQ into a lock-free SPSC ring (`rtrb`) for the decode worker. This ring is the realized "broadcast" boundary; on overflow the reader drops rather than blocks. Control messages are applied between reads: retune (`Tune`), reconfigure the spectrum analyzer (`SetSpectrum`), publish rate (`SetFps`), and `Stop`.
- The audio callback (not yet built) will pull mixed samples from a ring; the rest of the core exists to keep that ring full.

This layer does not use tokio. Async schedulers introduce nondeterministic wakeup latency, which the audio path cannot tolerate.

**Orchestration (a worker thread today, not tokio):**

- A decode worker thread drains the IQ ring and runs each active `Channel` (NCO mixer → low-pass + decimate → FM demodulate → decoder), forwarding decoded `Event`s back over an `mpsc` event bus. The front-end drains the bus with `Engine::drain_events`. Channels are replaced atomically via `Engine::set_channels` (the band scanner uses this to swap the per-window station set on each retune).
- This is a plain thread, not tokio: the decode path is CPU-bound, not async IO, so a worker thread is the minimum effective abstraction and matches the realtime style. tokio is deferred until genuinely async orchestration (network export, disk recording) needs it. Multi-channel parallelism (rayon over channels) and an FFT channelizer are the scaling levers if one worker can't keep up; today a handful of channels at ~1 MS/s keeps up.
- Decode output is decoupled from the realtime path: a decoder emits structured events and cannot stall the reader regardless of its latency.

---

## Workspace

A Cargo workspace with a strict dependency direction. Leaf crates are pure and IO-free, the two front-ends sit on top, and dependencies point inward. No core crate has knowledge of the UI.

- **`core`**: shared types and the `Source` trait. The `Complex<f32>` sample type (re-exported as `core::Iq`, from `num-complex`) and the error type. IO/async/UI-free. All other crates depend on it.
- **`dsp`**: signal processing. FFT, windowed-sinc FIR (low-pass/band-pass, RRC), decimation, the complex `Nco` mixer, the `FmDemod` discriminator, and a `Pll`. No IO, hardware, or async. The most heavily unit-tested crate.
- **`device`**: `Source` implementations. The RTL-SDR driver (with the drop-free reader thread) and the `FileSource` replay source. The only crate that accesses hardware.
- **`decode`**: decoder tails. Defines the `Decoder` trait and `Event` enum, and contains the RDS decoder (pilot-locked 57 kHz recovery, biphase symbol sync, block sync with single-burst error correction, group parsing → PI/PS/RadioText/PTY, and RT+ for structured now-playing title/artist). Depends on `dsp`. Tested against synthetic signals and a recorded fixture.
- **`engine`**: the runtime. Owns the reader thread, the wideband-FFT taps, the IQ ring, and the decode worker that runs `Channel`s (`engine::channel`) and emits `ChannelEvent`s. Exposes the control surface (tune, set_channels, drain_events). UI-agnostic, with the source injected as a trait object.
- **`app`**: the GPUI front-end. Depends on `engine`. Renders the spectrum/waterfall from tap snapshots.
- **`cli`**: a headless front-end (`device list/info`, `listen`, `record`). Depends on `engine`.
- **`scanner`**: the FM-band RDS scanner. Plans device-bandwidth windows over the station grid, drives the engine (tune + set_channels per window), folds decoded events into a station table, and renders a live TUI (`scan`) or decodes one station headlessly (`probe`). Depends on `engine`.

```
core ←── dsp ←── decode ──┐
  ↑       ↑               │
  └── device              │
          ↑               ↓
          └──────────── engine ──→ { app, cli, scanner }
```

`engine` exists as a separate crate so the front-ends are interchangeable consumers over one runtime. `app`, `cli`, and `scanner` are peers, which keeps the core free of any UI dependency.

---

## Key traits

`Source` (in `core::source`) and `Decoder` (in `decode`) are implemented; the `Demodulator` sketch is unbuilt (FM is a concrete `dsp::FmDemod` for now, since RDS is the only tail).

```rust
// A producer of raw IQ. Live hardware and recorded files are peers.
// Implemented over rtl-sdr-rs (RtlSdrSource) for the V3; FileSource for replay/fixtures.
// `Send` so the engine can own it on a dedicated reader thread.
trait Source: Send {
    fn sample_rate(&self) -> u32;
    fn center_freq(&self) -> u64;
    fn tune(&mut self, hz: u64) -> Result<()>;          // file source: updates reported freq
    fn tune_range(&self) -> (u64, u64);                  // tunable Hz range; UI clamps to it
    fn read(&mut self, out: &mut [Iq]) -> Result<usize>; // Ok(0) = end of stream (EOF)
}

// Baseband IQ in, audio out. AM/FM/SSB are implementations.
trait Demodulator {
    fn bandwidth(&self) -> u32;                          // drives the channel's decimation
    fn demod(&mut self, baseband: &[Complex<f32>], audio: &mut Vec<f32>);
}

// Consumes demodulated samples, emits structured events when it recovers meaning.
// Implemented by `RdsDecoder`; the engine owns one per channel and feeds it FM multiplex blocks.
trait Decoder: Send {
    fn feed(&mut self, samples: &[f32]) -> Vec<Event>;
}
```

The realized `Decoder` takes real demodulated samples (FM multiplex or audio) and returns `Event`s. RDS, the only tail so far, consumes the multiplex; a future raw-IQ decoder would want a different input. The original sketch anticipated this with a declared input kind (`IQ | Audio | Bits`); that generalization is deferred until a second decoder needs it, to avoid an abstraction with one caller.

---

## Testing

The architecture is organized around testability.

- **`dsp` is pure** and is tested with constructed inputs and known outputs: a tone in, the correct bin out.
- **Decoders are tested against recorded fixtures** through the real pipeline, via `cli` or `engine` directly. A signal is recorded once and the expected events are asserted thereafter. The DSP is not mocked, because the fixture is the exact input the hardware would have produced.
- **`engine` takes its IO by injection**, so an integration test passes it a file `Source` and exercises the same code path the app uses.
- The realtime/orchestration split keeps the components that are hard to test (threads, audio callbacks) thin and stable, while the components that change frequently (DSP, decoders) are pure and straightforward to test.

---

## UI and rendering

The front-end uses GPUI with gpui-component: native chrome, theming, dock layout, and virtualized tables for the frequency database and the event timeline.

The signal display is implemented as a custom GPUI `canvas`, since the UI library does not cover it:

- **Waterfall**: a scrolling BGRA texture, one new row per frame, uploaded and blitted. A waterfall is an image, so it maps directly onto GPUI's image primitive.
- **Spectrum and stage views**: painted paths driven by the tap snapshots.

GPUI has no custom-shader render pass, so GPU-side effects (shader colormaps, GPU phosphor) are unavailable, and colormapping runs on the CPU before upload. At the V3's data rate this is sufficient. Outgrowing this constraint is the one condition that would justify revisiting the rendering layer.

The UI reads tap snapshots at frame rate and does not reach into the signal path. It observes the system rather than participating in it.

---

## Dependencies: build vs. buy

The division is deliberate: buy the plumbing, build the signal processing. Hardware access and the FFT are solved problems with no learning value in reimplementation. Everything between samples and meaning is the substance of the project and is written by hand.

**Buy, device layer.** `rtl-sdr-rs` (the ccostes crate), behind the `device` crate's `Source` implementation. It is a pure-Rust RTL-SDR driver layered on `rusb`/libusb; the `device` crate enables `rusb`'s `vendored` feature (default-on), which statically compiles libusb into the binary. The shipped binary therefore needs no system library installed at runtime — the app is standalone and works without `sudo` on macOS (the RTL bulk interface is vendor-specific, with no kernel driver to detach); libusb compiling from source is a build-time concern only. The crate is currently maintained, and the V3 is its well-supported target. Because everything downstream sits behind the `Source` trait, changing drivers is a single-crate change. `soapysdr` is the documented fallback for multi-radio support: it is the mature, vendor-neutral standard, at the cost of requiring system libraries and carrying known driver-level thread-safety issues. The flowgraph frameworks in this space (FutureSDR, rustradio) are excluded as dependencies, because they would own the concurrency model and channel abstraction defined here. They are useful as references.

**Buy, FFT.** `rustfft` (auto-detects AVX/SSE/NEON, supports any size, O(n log n)) with `num-complex` for the sample type. Since rustfft re-exports num-complex, the type is consistent across the stack without conversion. `realfft` is optional, for the real-input spectrum path. This is the complete math-dependency list.

**Buy, plumbing.** `rtrb` for the lock-free SPSC IQ ring between the reader and the decode worker; `ratatui` + `crossterm` for the scanner TUI.

**Build, everything else.** The tuner (complex NCO), FIR filtering and decimation, demodulators (AM/FM/SSB), AGC, and the decoder-side work (clock/symbol recovery, framing, parity). These live in `dsp` and `decode`. They are small, and implementing them is where the project's learning value lies.

**Build profile.** `dsp`, `decode`, and `engine` are compiled at `opt-level = 2` even in dev/test builds (a per-package override in the root `Cargo.toml`). Their tight per-sample loops are ~20x slower unoptimized: at opt-level 0 a single channel only just reaches realtime, so the scanner cannot keep up and the decode tests take ~48s. The app and UI crates keep the default opt-level for fast incremental compiles and easy debugging.

---

## Open decisions

The following are deliberately unsettled, to be resolved in code or a later document.

- **Recording file format.** SigMF (raw samples plus a JSON metadata sidecar) is the current preference: it is the community standard, it is interoperable with other tools, and its annotation support can hold expected-event metadata for golden fixtures. The simpler alternative is plain interleaved `cf32`/`cu8` with a minimal header. This should be decided before fixtures accumulate, since migrating them later is costly. Until then, `FileSource` reads headerless raw interleaved `cu8` (the RTL native format), with sample rate and center frequency supplied by the caller.
- **Decoder input typing.** `Decoder::feed(&[f32])` takes demodulated samples today; a per-kind input (IQ vs audio vs bits) waits for a second decoder (see Key traits).
- **Channel scheduling.** The decode worker runs channels sequentially on one thread; this keeps up with a handful of channels at ~1 MS/s. Parallelizing across channels (rayon) or an FFT channelizer is the lever if a wider window or higher rate outgrows it.
- **Capture rate.** `read_sync` (the only API `rtl-sdr-rs` exposes) keeps up drop-free to ~1.2 MS/s but not 2.4 MS/s, so the scanner runs at ~1 MS/s. True async USB streaming (more queued transfers) would lift this.
