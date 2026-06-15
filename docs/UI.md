# UI

This document describes the application layout and the behavior of its regions. It covers structure and interaction, not visual styling (color, type, spacing live with the design tokens in the frontend code).

The layout follows four principles from the vision: the signal display is primary, observability is a first-class surface rather than a modal, the app is keyboard-first, and it should feel like a professional tool. The chosen model is a curated set of workspaces with fixed regions and resizable splitters.

---

## The frame

A fixed arrangement: a nav rail on the far left, a primary signal display in the center, an inspect panel on the right, a tabbed working pane along the bottom, and a transport bar beneath everything. Every splitter is draggable and the inspect and bottom panes are collapsible. Regions do not reorder or detach.

```
┌───┬──────────────────────────────────────────────┬───────────────┐
│   │  133.700 MHz   AM   bw 12k   gain 28      ⌘K  │               │
│ ◉ ├──────────────────────────────────────────────┤   INSPECT     │
│ ▤ │  ▁▂▃▅▇ spectrum (FFT) ▇▅▃▂▁                   │  stage: demod │
│ ⧉ │                                              │   ∿ waveform   │
│ ⊚ │ ░░░░░░░░░ waterfall ░░░░░░░░░░░░░░░░░░░░░░░░░  │   SNR   18 dB  │
│   │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │   bw    12 kHz │
│   │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │                │
│   │ ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │   TUNE         │
│   ├──────────────────────────────────────────────┤   freq   mode  │
│   │ [ Decoder | Events | Channels ]              │   bw     gain  │
│   │  AC123   39000ft   hdg 270        ADS-B      │   squelch      │
│   │  pager: "call ext 4471"           POCSAG     │                │
│   │  Acurite 0x3f   21.4°C   58%RH    433        │                │
├───┴──────────────────────────────────────────────┴───────────────┤
│ ◀◀  ▮▮  ▶▶   [====DVR buffer====○=========]  -12s    ● REC  44.1k  │
└───────────────────────────────────────────────────────────────────┘
  nav:  ◉ Listen    ▤ Library    ⧉ Recordings    ⊚ Settings
```

---

## Regions

**Nav rail.** A narrow icon column on the far left that switches the top-level workspace. It is always visible and never holds content itself. Workspaces are listed under Workspaces below.

**Top bar.** A compact always-visible status line: the active frequency and sample rate (and, later, mode/bandwidth/gain), with the command palette entry point (Cmd-K) at the right. Read-only; inline tuning lives in the center header (below) and the palette.

**Primary signal display (center).** The largest region and the focus of the app. A tuning header runs across the top: a large frequency selector on the left and a read-only signal-level meter (−100..0 dBFS) on the right. The frequency selector is absolute tuning: each decimal digit is independently editable, so hovering a digit and scrolling the wheel steps that place (with carry), and typing a number writes it in and advances. Editing it does a full hardware retune, recentering the captured window on the entered frequency (marker at center). Clicking or dragging anywhere on the spectrum or waterfall instead offsets the tuned marker to the frequency under the cursor without recentering, so the picture stays still while tuning (the device captures a `sample_rate`-wide window, so moving within it needs no retune; dragging clamps to the window edge). The tuned (marker) frequency is the only frequency shown; the hardware center stays invisible. All tuning is clamped to the connected tuner's frequency range (reported by the device), so the radio cannot be driven out of range. Below the header it stacks the frequency-domain spectrum (FFT line) above the waterfall (spectrogram over time), with a draggable splitter between them so the user sets the ratio. The spectrum carries a dB axis on the left and a frequency scale along the bottom, between it and the waterfall; a red tuned-frequency marker and a translucent channel-bandwidth band overlay the spectrum, and hovering either pane shows a readout of the frequency under the cursor (and, on the waterfall, how long ago that row was captured). The spectrum and waterfall share one dB scale that by default auto-tracks the noise floor, with an optional manual min/max; the waterfall scrolls newest-on-top through a selectable perceptual colormap. The time-domain waveform is also a primary display and can be promoted into the center (replacing or splitting with the waterfall); by default it appears smaller in the inspect panel. Waterfall and waveform are the two displays that define the app, so both are reachable without leaving the Listen workspace.

**Inspect panel (right).** The observability surface. It shows the output of a selected pipeline stage (raw IQ, baseband, demodulated waveform, recovered bits) plus derived readouts (SNR, bandwidth, symbol rate), and it holds the FFT/display settings (transform size, window, colormap, frame rate, averaging, bandwidth, dB scale) as dropdowns from the in-house `ui` component library, plus the Tune controls. This panel is what makes a failed decode diagnosable rather than silent, so it is permanent rather than a modal. Collapsible when the user wants maximum display area.

**Bottom pane (tabbed).** The working surface beneath the signal display. Tabs:

- **Decoder**: live decoder activity for the active channel. Shows the decode in progress (frame sync, CRC status, recovered fields) and is the place to watch a decoder succeed or fail in real time.
- **Events**: a running feed of decoded events across all channels (planes, pagers, sensors, ships), newest at top. This is the at-a-glance version of the history; the full history has its own workspace.
- **Channels**: the list of active channels, each with its frequency, mode, and decoder, selectable and removable.

The bottom pane is resizable and collapsible. Tabs were chosen over a fixed split to conserve vertical space for the signal display; a side-by-side split is noted as an alternative below.

**Transport bar.** A full-width strip for record and replay: transport controls, the DVR buffer scrubber with a position marker, the buffer depth readout, the record toggle, and the audio sample rate. Present in every workspace so recording is always one action away. Play/pause currently freezes and resumes the live display (the device keeps streaming); the DVR controls are not yet wired.

---

## Workspaces

The nav rail switches between these. Each is a full-frame arrangement; the signal display and panels described above belong to Listen.

**Listen** is the live operating view: the frame shown above.

**Library** is the frequency database (bookmarks). A searchable, filterable table; clicking a row tunes to it.

```
┌───┬───────────────────────────────────────────────────────────────┐
│ ◉ │  LIBRARY                                  + add     ⌘K search   │
│ ▤ ├───────────────────────────────────────────────────────────────┤
│ ⧉ │  ★  NAME            FREQ        MODE  BW     BAND      LAST     │
│ ⊚ │  ★  Montreal Air    133.700M    AM    12k    airband   2d ago  │
│   │     NOAA WX         162.400M    NFM   16k    marine    .       │
│   │     CHOM FM          97.700M    WFM   180k   bcast     1h ago  │
│   │     Local POCSAG    148.350M    NFM   12k    pager     5m ago  │
│   │  ───────────────────────────────────────────────────────────  │
│   │  filter: [ airband ▾ ]   [ ☆ favorites ]    row click → tune   │
└───┴───────────────────────────────────────────────────────────────┘
```

**Recordings** lists saved IQ captures (the DVR snapshots and manual recordings), each openable as a file `Source` to replay through the live pipeline.

**Events history** is the full decode timeline: the queryable record of everything found, filterable by type, channel, and time, and exportable. The bottom-pane Events tab is the live feed; this workspace is for browsing and searching the accumulated history. Each event can link back to the IQ segment that produced it, so selecting a row can replay that moment.

```
┌───┬───────────────────────────────────────────────────────────────┐
│ ◉ │  EVENTS                        [ All types ▾ ]  ⌘K  ⤓ export    │
│ ▤ ├───────────────────────────────────────────────────────────────┤
│ ⧉ │  TIME      TYPE     SOURCE       SUMMARY                        │
│ ⊚ │  14:02:11  ADS-B    AC123        FL390   270°   481kt           │
│   │  14:01:54  433      Acurite      21.4°C  58%RH  batt ok         │
│   │  14:01:03  POCSAG   148.350M     "call ext 4471"                │
│   │  13:59:40  AIS      Bay Queen    MMSI 316…  12.3kn  hdg 088     │
│   │  ───────────────────────────────────────────────────────────  │
│   │  filter: type · channel · time          row click → replay IQ  │
└───┴───────────────────────────────────────────────────────────────┘
```

Whether Events deserves a full workspace in addition to the bottom-pane tab, or whether the tab alone is enough, is an open question (below).

---

## Pane behavior

The layout is fixed in arrangement and flexible in sizing. This is the deliberate middle ground between a rigid single layout and a free docking system.

- Every splitter is draggable, with minimum sizes enforced so no region collapses by accident.
- The inspect panel, the bottom pane, and the nav rail (to icons only) are collapsible.
- The spectrum/waterfall ratio inside the center is itself a draggable splitter.
- Panel sizes and collapsed state are intended to persist per workspace and restore on launch (see Persistence; not wired yet).
- Regions do not reorder, detach, or float. Free docking is out of scope: it moves most of the design effort onto the user and makes a good default impossible to guarantee.

Keyboard-first interaction:

- Cmd-K opens the command palette for tuning, mode and bandwidth changes, bookmark recall, decoder start/stop, and workspace switching.
- Panel toggles and workspace switches have direct shortcuts.
- Tuning, mode, and gain are all reachable without the mouse.

---

## Bottom pane: tabs vs split

The default is tabs, to preserve vertical space for the signal display. The alternative is a side-by-side split, which trades display height for seeing the live decoder and the event feed at once. This is offered as a per-user option rather than a fixed choice.

```
Alternative: split the bottom pane instead of tabbing it
┌──────────────────────────────┬──────────────────────────────┐
│ Decoder (live)               │ Events (feed)                 │
│  bits 1011001011…            │  14:02  ADS-B   AC123         │
│  frame sync ✓   CRC ✓        │  14:01  433     Acurite       │
│  field: altitude 39000       │  14:01  POCSAG  148.350M      │
└──────────────────────────────┴──────────────────────────────┘
```

---

## Persistence

There are two stores, split by the shape of the data.

**UI/session settings** persist in a small embedded key-value store (`redb`) under the platform app-support directory, owned by `app`. These are a single settings record, not a queryable table, so a KV store is the minimum effective fit. There is no manual save: every change is written (debounced) and the full state is restored on launch. Covered today: tuned frequency and the hardware center, marker bandwidth, FFT size and window, frame rate, colormap, display averaging, dB-scale mode and range, and the active workspace. (Panel sizes and collapsed flags are intended to join this set but are not persisted yet.)

**Bookmarks and history** will live in a single SQLite database, owned by `engine` and surfaced by the UI, because both are queryable, filterable tables and it needs no server. Not yet implemented.

- **Bookmarks** (the Library): name, frequency, mode, bandwidth, band, favorite flag, last-used time.
- **Events** (the history): timestamp, type, source channel, decoded payload, and an optional reference to the IQ recording segment that produced it. Indexed by time, type, and channel for fast filtering.

The data-flow side of the SQLite store (the event bus writing to it, tuning reading bookmarks) belongs in ARCHITECTURE as a short addition to the `engine` description.

---

## Scanner (terminal)

The `scanner` crate is a separate, terminal-based front-end (not part of the GPUI frame above): a focused tool that sweeps the FM band and shows what every station is playing, decoded from RDS. It rides the same `engine` as the app, so it is a peer front-end rather than a fork of the pipeline.

`sdr-scan scan` runs a `ratatui` table that fills in as the scanner sweeps: one row per station with RDS, showing frequency, the program-service name, the program type, and the current RadioText (which is where the now-playing "Artist - Title" lives). A status line shows the window being tuned. The band is covered as a sequence of device-bandwidth windows because one RTL-SDR sees only ~1-2 MHz at once; each window is dwelled on long enough for RDS to deliver the slow-arriving PS and RadioText before retuning. `sdr-scan probe --freq <MHz>` is the headless single-station path used to validate decoding against the live device.

---

## Open questions

- **Events as workspace vs. tab.** Whether the full Events history warrants its own workspace in addition to the bottom-pane live feed, or whether one of the two is redundant. Current lean: keep both, with the tab as the live feed and the workspace as the searchable archive.
- **Waveform placement.** Default location of the time-domain waveform: small in the inspect panel (current default) versus promoted into the center beside the waterfall.
- **Layout state storage.** Settled: UI/session settings persist in a `redb` KV store; SQLite is reserved for the queryable bookmarks/events tables. Panel-size persistence still needs wiring (subscribing to the resizable groups' resize events).
- **Multi-channel display.** How the center behaves with several active channels: one shared spectrum with markers, or stacked per-channel strips. Settle once multi-channel is in use.
