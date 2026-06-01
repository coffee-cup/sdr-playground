# Vision

> A modern software-defined radio for people who care how their tools feel.
> The radio you wish existed on a Mac.

---

## Why this exists

Software-defined radio is one of the most quietly magical things you can do with a $30 dongle: pull invisible signals out of the air and turn them into sound, data, images, meaning. The capability is extraordinary. The software is not.

The existing tools fall into two camps. The powerful ones (SDR++, GQRX, SDRangel) are dense, capable, and feel like they were designed by accretion — every knob exposed, nothing curated, visuals that look like 2008. The pretty ones don't exist. On macOS in particular, the gap between "what the hardware can do" and "what the software makes pleasant" is wide and unfilled.

Nowhere is that gap wider than in decoding. The air is full of structured signals — aircraft, ships, pagers, weather sensors, satellite imagery — and pulling meaning out of them is the most rewarding thing SDR offers. Yet today it means juggling a pile of disconnected command-line tools, each with its own setup and output format. This project makes decoders first-class: easy to reach, consistent to use, and feeding one shared, searchable view of what's on the air. Turning a signal into meaning should be a click, not an afternoon.

This project closes that gap. It is two things at once, and both matter:

1. **A way to learn SDR by building it.** The fastest way to understand DSP is to implement it — to feel why a window function matters because your spectrum looks wrong without one.
2. **A tool worth using.** Not a toy that proves a point, but the SDR app I'd actually reach for, with the craft of Linear and the restraint of Ghostty applied to a domain that has never had either.

And above all it should feel like a _playground_. Radio is genuinely amazing, and the software should invite exploration rather than punish it — somewhere you can poke at a signal, try a demod, point a decoder at it, and _see_ what happens at every step. GQRX, but better, and with the digital and audio protocol decoders that make the hobby come alive.

If it's only a learning exercise, it dies in a drawer. If it's only a pretty shell, it's a worse SDR++. It has to be both.

---

## Principles

These are the North Star. When a decision is unclear, it should be resolved by appeal to these, not by feature comparison.

**Minimum effective abstraction.** Start simple, minimal, fast. Add complexity only when the absence of it actively hurts. Every layer of indirection has to earn its place by removing more pain than it introduces. This applies to the DSP and the UI alike.

**One pipeline, many tails.** Almost everything this app does is the same operation: tune → filter → decimate → demodulate → (optionally) decode. A "decoder" is a demodulator with a parser bolted on. If that core pipeline is right, every feature is a small leaf, not a new subsystem. Protect this abstraction. It is the difference between a tool that grows gracefully and one that ossifies.

**The pipeline is observable.** You can always see where meaning breaks down. Every stage from antenna to decoded event can be watched — raw IQ, filtered baseband, the demodulated waveform, the recovered bits. When a decode fails, silence is never the only feedback: you can tell whether the signal wasn't there, the demod was wrong, or the decoder choked. This is the difference between a black box and an instrument, and it's what makes the app a place to experiment rather than a tool that either works or doesn't.

**Data, not scrolling text.** Legacy tools dump decoded output as a console firehose. Decoded signals are _structured events_ — a timestamp, a source, a payload — and should be treated as queryable, filterable, exportable data. This single reframe is most of what makes the app feel modern.

**Keyboard-first, mouse-optional.** Radios were built around physical knobs and the software inherited that mouse-heavy DNA. A command palette and real shortcuts make a power user fast. This is where taste in tools like Arc and Linear pays off in a domain that's never had it.

**Performance comes from doing less.** Real-time visualization of high-rate data is the hard constraint. Speed is won by doing less work and fewer allocations — not by cleverness layered on a bloated core. Think in Big O before reaching for parallelism.

**The signal should be beautiful.** The waterfall is the soul of the app. It should be the best-looking spectrogram on the platform. This is not vanity; a clearer display is a more useful instrument.

---

## The core abstraction

Everything flows from a single IQ stream off the device. The architecture is a fan-out:

```
                            ┌─ Channel ─ filter→decimate→demod→[decode]→ audio / events
  Device → IQ stream ──┬────┼─ Channel ─ ...
   (RTL-SDR V3)         │    └─ Channel ─ ...
                        └──→ Wideband FFT → spectrum + waterfall (display)
```

A **Channel** is the unit of work: a complex frequency shift to baseband, a low-pass filter and decimation down to the signal's bandwidth, a demodulator, and an optional decoder tail. You can spawn N of them against the same stream. Listening to three frequencies at once, or decoding pagers while monitoring the airband, is _the same code instantiated three times_ — not three special cases.

Get this right and new capabilities stop being features to architect and become leaves to hang — a new decoder is a small parser on an existing tail, not a new subsystem.

---

## What makes it different

The capabilities that justify the project's existence:

**IQ record & replay — the DVR.** A rolling buffer of recent raw IQ (2.4 MS/s ≈ 19 MB/s; minutes are cheap). The waterfall becomes scrubbable: a signal flashes past, you drag _into the past_, retune, change demod, and decode it after the fact. Almost no tool does this cleanly, and it changes what the radio fundamentally is — not just a window on the present moment, but an instrument you can rewind.

**Multiple simultaneous channels (VFOs).** Falls out of the core abstraction for free. A clear marker of a serious, modern tool.

**Decode event timeline.** Every ADS-B hit, pager message, or 433 MHz sensor reading lands as a structured, timestamped, filterable row — searchable and exportable. This is "data, not text" made concrete.

**Scanner with squelch-triggered capture.** Sweep a frequency list, stop on activity, auto-record audio and log the event. A handheld scanner's job, but with a searchable history.

**Frequency database & band-plan overlay.** A structured, searchable table of known frequencies with click-to-tune, plus band-plan regions painted onto the spectrum so you _see_ what you're looking at instead of memorizing ranges.

**Command palette.** Keyboard-first control over tuning, demod, bookmarks, and decoders.

**Phosphor / persistence display.** The polish layer — deferrable, but it's part of "the signal should be beautiful."

---

## Non-goals

A North Star is defined as much by what it refuses. These are not omissions to revisit later — they are part of what the tool _is_:

- **Digital voice (DMR / P25 / D-STAR).** Trunking logic plus the patent-encumbered AMBE codec. A rabbit hole where SDR projects go to die.
- **A plugin system / decoder ABI.** Decoders are compiled-in, not loaded from an extension interface. The cost of an ABI and a sandbox isn't worth the extensibility for a focused, single-author tool.
- **Transmit.** Receive-only. TX is a different regulatory and hardware universe.
- **A hardware matrix.** Built around the RTL-SDR V3. Supporting every dongle on the market is someone else's project.
- **Exposing every parameter.** Curation over completeness. If a knob doesn't earn its place on screen, it lives in the command palette or nowhere.

---

## What "good" looks like

The project has succeeded when:

- I reach for it instead of SDR++ without thinking about it.
- A signal that's just a smear on the waterfall becomes decoded meaning in a few clicks, with no manual.
- Recording, scrubbing back, and decoding a signal after the fact feels obvious and fast.
- The waterfall is the screenshot people share when they want to show what SDR looks like.
- The codebase still feels like minimum effective abstraction — a sixth decoder is a small file, not a refactor.

If it's powerful but ugly, it failed. If it's beautiful but I don't use it, it failed. The bar is a tool that is both, and that taught me how radio works on the way to being built.
