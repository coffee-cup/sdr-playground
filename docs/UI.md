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

**Top bar.** A single status line for the active channel: frequency, mode, bandwidth, gain, and device state, with the command palette entry point (Cmd-K) at the right. Read-only display; editing happens in the Tune section of the inspect panel or through the palette.

**Primary signal display (center).** The largest region and the focus of the app. It stacks the frequency-domain spectrum (FFT line) above the waterfall (spectrogram over time), with a draggable splitter between them so the user sets the ratio. The time-domain waveform is also a primary display and can be promoted into the center (replacing or splitting with the waterfall); by default it appears smaller in the inspect panel. Waterfall and waveform are the two displays that define the app, so both are reachable without leaving the Listen workspace.

**Inspect panel (right).** The observability surface. It shows the output of a selected pipeline stage (raw IQ, baseband, demodulated waveform, recovered bits) plus derived readouts (SNR, bandwidth, symbol rate), and it holds the Tune controls. This panel is what makes a failed decode diagnosable rather than silent, so it is permanent rather than a modal. Collapsible when the user wants maximum display area.

**Bottom pane (tabbed).** The working surface beneath the signal display. Tabs:

- **Decoder**: live decoder activity for the active channel. Shows the decode in progress (frame sync, CRC status, recovered fields) and is the place to watch a decoder succeed or fail in real time.
- **Events**: a running feed of decoded events across all channels (planes, pagers, sensors, ships), newest at top. This is the at-a-glance version of the history; the full history has its own workspace.
- **Channels**: the list of active channels, each with its frequency, mode, and decoder, selectable and removable.

The bottom pane is resizable and collapsible. Tabs were chosen over a fixed split to conserve vertical space for the signal display; a side-by-side split is noted as an alternative below.

**Transport bar.** A full-width strip for record and replay: transport controls, the DVR buffer scrubber with a position marker, the buffer depth readout, the record toggle, and the audio sample rate. Present in every workspace so recording is always one action away.

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
- Panel sizes and collapsed state are persisted per workspace and restored on launch.
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

## Persistence (SQLite)

A single SQLite database, owned by `engine` and surfaced by the UI, backs the stored data. SQLite is the right fit because both bookmarks and history are queryable, filterable tables, and it needs no server.

- **Bookmarks** (the Library): name, frequency, mode, bandwidth, band, favorite flag, last-used time.
- **Events** (the history): timestamp, type, source channel, decoded payload, and an optional reference to the IQ recording segment that produced it. Indexed by time, type, and channel for fast filtering.
- **Layout and session state**: panel sizes, collapsed flags, and last active channel. This may live in SQLite or a small config file; to be decided.

The data-flow side of this (the event bus writing to the store, tuning reading bookmarks) belongs in ARCHITECTURE as a short addition to the `engine` description.

---

## Open questions

- **Events as workspace vs. tab.** Whether the full Events history warrants its own workspace in addition to the bottom-pane live feed, or whether one of the two is redundant. Current lean: keep both, with the tab as the live feed and the workspace as the searchable archive.
- **Waveform placement.** Default location of the time-domain waveform: small in the inspect panel (current default) versus promoted into the center beside the waterfall.
- **Layout state storage.** SQLite versus a config file for panel sizes and session state.
- **Multi-channel display.** How the center behaves with several active channels: one shared spectrum with markers, or stacked per-channel strips. Settle once multi-channel is in use.
