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
        ┌─────────────────────────── realtime core (threads, no async) ──┐
        │                                                                 │
 Source ─→ pre-trigger ring ─→ broadcast ─┬─→ Channel ─→ demod ─→ ┐       │
 (device                                  ├─→ Channel ─→ demod ─→ ┤       │
  or file)                                └─→ Channel ─→ ...       │       │
        │                                                      mixer ─→ audio out
        │         └─→ wideband FFT ─→ spectrum / waterfall          │      │
        └───────────────────────┬─────────────────────────────────┘      │
                                 │ taps (lock-free snapshots)              │
        ┌────────────────────────┴──────────── orchestration (tokio) ─────┘
        │
        ├─→ decoders (one task each; heavy compute → CPU pool) ─→ events
        ├─→ event bus ─→ timeline / UI
        ├─→ recording (snapshot ring → disk)
        └─→ UI: GPUI render loop pulling tap snapshots
```

---

## Signal pipeline

All processing originates from a single stream of raw IQ from the `Source`. The stream is broadcast to N consumers: each `Channel` and the wideband FFT reads from its own ring buffer, so no consumer contends with another and a slow consumer cannot back-pressure the source. A dropped sample in a visualization is acceptable; blocking the device read is not.

A `Channel` is the unit of work:

```
tune (freq shift to baseband) → low-pass + decimate → demodulate → [decoder tail]
```

Each stage is cheap and composable. Tuning is a complex multiply. Decimation discards unused bandwidth, which is the primary performance lever: DSP runs on a 200 kHz slice rather than the full 2.4 MS/s. Demodulation converts baseband to audio, or passes IQ through to a decoder. The decoder tail is optional and decoupled from the audio path (see Concurrency model).

Demodulated audio from all active channels feeds a single mixer and then the audio device. One output, N channels.

---

## Observability: stage taps

Stage-level observability is an architectural property, not a UI feature. Every stage publishes its most recent output through a tap: a lock-free, single-slot snapshot that the UI reads at frame rate. Raw IQ, post-filter baseband, demodulated waveform, and recovered bits are each observable without perturbing the signal path. The producer overwrites the slot, the reader takes the latest value, and a missed frame has no consequence.

This addresses a common failure mode in tools such as rtl_433, where a failed decode produces no output and no diagnostic information. With per-stage taps, a failed decode can be traced up the chain to determine whether the signal was absent, the filter was misplaced, the demodulator was misconfigured, or the decoder itself failed.

The tap mechanism and the decoder input mechanism are the same subscription primitive. A stage exposes its output; the UI subscribes to observe it, and a decoder subscribes to consume it. One mechanism serves both cases.

---

## Source abstraction and record/replay

A `Source` produces raw IQ. Live hardware and recorded files implement the same trait, so the pipeline is identical whether samples come from the antenna or from disk. Running a saved signal as if live is not a separate mode; it is a file `Source` that paces its reads to the sample rate.

Recording is the inverse operation. At the raw-IQ level, before fan-out, a pre-trigger ring buffer continuously overwrites with the last N seconds of input (configurable, default approximately 1 second). At 2.4 MS/s in `cf32` this is about 19 MB/s, so several seconds in RAM is inexpensive. A save operation snapshots the ring to disk, capturing signal that has already passed, which is the required behavior for transient signals.

Saved files serve as test fixtures. A signal is recorded once and committed; each decoder test replays it through the real pipeline and asserts on the resulting events. The DSP is not mocked, because the fixture is the exact input the hardware would have produced.

This makes the `Source` trait the most important boundary in the system. With it defined correctly, live operation, replay, waterfall scrubbing, and testing share a single code path.

---

## Concurrency model

**Realtime core (dedicated threads, no async):**

- A reader thread loops on `source.read()` and writes into the pre-trigger ring and the broadcast. A device source blocks on USB; a file source paces to real time.
- A DSP pool processes active channels as work units. Multi-core parallelism occurs here: multiple channels, or multiple decoders' compute, run on separate cores, while a single channel runs as straight-line code with no internal threading overhead.
- The audio callback pulls mixed samples from a ring and must always have data available. The rest of the core exists to keep that ring full.

This layer does not use tokio. Async schedulers introduce nondeterministic wakeup latency, which the audio path cannot tolerate.

**Orchestration (tokio):**

- Channel lifecycle, control messages, and the event bus run as async tasks.
- Each decoder runs as one tokio task, making decoders independent and parallel across each other. CPU-bound decode work (FFTs, filtering, image reconstruction) is dispatched to a blocking/compute pool rather than run on async workers: tokio's worker pool is sized for IO, and a CPU-bound task that does not yield would occupy a worker and starve the others. The task handles orchestration; the compute pool handles computation.
- Decode output is always decoupled from the audio path. A decoder emits structured events to the bus and cannot stall audio regardless of its latency. Inexpensive decoders may compute inline, but their output is decoupled either way.
- Recording and file IO are async and run off the hot path.

---

## Workspace

A Cargo workspace with a strict dependency direction. Leaf crates are pure and IO-free, the two front-ends sit on top, and dependencies point inward. No core crate has knowledge of the UI.

- **`core`**: shared types and traits. Sample types, the `Event` type, configuration, and the central traits (`Source`, `Demodulator`, `Decoder`, the tap/stage interface). No dependencies. All other crates depend on it.
- **`dsp`**: signal processing. FFT, filters, decimation, demodulators. No IO, hardware, or async. The most heavily unit-tested crate, since it is both the easiest to test in isolation and the easiest to get subtly wrong.
- **`device`**: `Source` implementations. The RTL-SDR driver (hardware and USB), the file replay source, and the pre-trigger ring. The only crate that accesses hardware.
- **`decode`**: decoder tails. Depends on `dsp` (decoders reuse filters and demodulators) and emits `core::Event` values. Tested against recorded fixtures.
- **`engine`**: the runtime. Assembles a `Source`, channels, decoders, mixer, and event bus into a running system, owns the threading model, and exposes a control surface (tune, add channel, start decoder, record). UI-agnostic, with IO injected: the source is passed in as a trait object, which is what makes the system testable and replayable.
- **`app`**: the GPUI front-end. Depends on `engine`. Handles rendering and has no knowledge of DSP internals beyond the tap snapshots and events that `engine` provides.
- **`cli`**: a headless front-end and the decoder test harness. Depends on `engine`. Pipes a recorded file through the real pipeline and prints or asserts on events. This crate is load-bearing: because `cli` and `app` are peers over the same `engine`, the core cannot acquire a UI dependency, and the same code provides a headless processing tool.

```
core ←── dsp ←── decode ──┐
  ↑       ↑               │
  └── device              │
          ↑               ↓
          └──────────── engine ──→ { app, cli }
```

`engine` exists as a separate crate so that `app` and `cli` are interchangeable consumers. If orchestration lived in `app`, the CLI would have to reimplement it and the two would diverge. A single runtime supports both front-ends.

---

## Key traits

These are interface sketches, not final signatures. They define the boundaries and will be refined in code.

```rust
// A producer of raw IQ. Live hardware and recorded files are peers.
// Implemented over rtl-sdr-rs for the V3; a file source for replay/fixtures.
trait Source {
    fn sample_rate(&self) -> u32;
    fn center_freq(&self) -> u64;
    fn tune(&mut self, hz: u64) -> Result<()>;          // file source: seek or no-op
    fn read(&mut self, out: &mut [Complex<f32>]) -> Result<usize>;
}

// Baseband IQ in, audio out. AM/FM/SSB are implementations.
trait Demodulator {
    fn bandwidth(&self) -> u32;                          // drives the channel's decimation
    fn demod(&mut self, baseband: &[Complex<f32>], audio: &mut Vec<f32>);
}

// Consumes a stage's output, emits structured events when it recovers meaning.
trait Decoder {
    fn input(&self) -> StageKind;                        // IQ | Audio | Bits (what it subscribes to)
    fn process(&mut self, samples: &Samples) -> Vec<Event>;
}
```

The trait sketches leave one question open: decoders do not all consume the same data. Some require baseband IQ, some require demodulated audio, and some require a recovered bitstream. The sketch handles this with a declared input kind and a tagged sample type, so a decoder subscribes to the appropriate stage tap. This is the reason the tap mechanism and the decoder input mechanism are unified. Whether the input type remains a single `Samples` enum or splits into per-kind traits should be settled once several real decoders exist.

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

**Buy, device layer.** `rtl-sdr-rs` (the ccostes crate), behind the `device` crate's `Source` implementation. It is a pure-Rust RTL-SDR driver with no system-library dependency, so it compiles on macOS with nothing to install; it is currently maintained; and the V3 is its well-supported target. Because everything downstream sits behind the `Source` trait, changing drivers is a single-crate change. `soapysdr` is the documented fallback for multi-radio support: it is the mature, vendor-neutral standard, at the cost of requiring system libraries and carrying known driver-level thread-safety issues. The flowgraph frameworks in this space (FutureSDR, rustradio) are excluded as dependencies, because they would own the concurrency model and channel abstraction defined here. They are useful as references.

**Buy, FFT.** `rustfft` (auto-detects AVX/SSE/NEON, supports any size, O(n log n)) with `num-complex` for the sample type. Since rustfft re-exports num-complex, the type is consistent across the stack without conversion. `realfft` is optional, for the real-input spectrum path. This is the complete math-dependency list.

**Build, everything else.** The tuner (complex NCO), FIR filtering and decimation, demodulators (AM/FM/SSB), AGC, and the decoder-side work (clock/symbol recovery, framing, parity). These live in `dsp` and `decode`. They are small, and implementing them is where the project's learning value lies.

---

## Open decisions

The following are deliberately unsettled, to be resolved in code or a later document.

- **Recording file format.** SigMF (raw samples plus a JSON metadata sidecar) is the current preference: it is the community standard, it is interoperable with other tools, and its annotation support can hold expected-event metadata for golden fixtures. The simpler alternative is plain interleaved `cf32`/`cu8` with a minimal header. This should be decided before fixtures accumulate, since migrating them later is costly.
- **Decoder input typing.** A single tagged `Samples` enum versus per-kind traits (see Key traits).
- **Channel scheduling.** Whether the DSP pool is a work-stealing pool (rayon-style) or hand-rolled. Settle this against a measured multi-channel load.
