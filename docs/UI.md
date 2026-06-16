# UI

This document is the app's **look and feel**: the visual language every screen answers to. It
describes how the app should feel and the vocabulary it's built from, not a pixel-exact layout.

Two anchors:

- **Visual reference:** `docs/style-refs/ableton.html` (open in a browser) is the canonical mockup
  of the look. When a styling question isn't answered here, match it.
- **Implementation:** the look lives in `crates/app/src/ui/` — design tokens, the theme palette
  (`ui/theme.rs`), and the reusable component kit. Build with those; don't re-roll chrome inline.

The design answers to four principles from the vision: the signal display is primary, observability
is a first-class surface (never a modal), the app is keyboard-first, and it should feel like a
professional instrument.

---

## The feel

A flat, dark, **studio-tool** aesthetic in the spirit of Ableton Live's dark theme: warm-gray
surfaces, near-black insets for anything that displays signal, one confident accent, and data drawn
in a cool second color. No gradients, no bevels, no glows, no drop shadows. Density over decoration:
controls are compact and packed, the way a device rack or a mixer is, so the signal gets the room.

The waterfall is the soul of the app and the brightest thing on screen. Everything around it is
quiet so it reads.

---

## Palette

The single source of truth is `crates/app/src/ui/theme.rs`, which writes these into the
gpui-component theme so the whole app (and every gpui-component widget) inherits them. Never
hardcode a color in a component; read it from the theme or the app palette.

| Token | Hex | Use |
|---|---|---|
| Background | `#1c1c1c` | App canvas behind panels |
| Surface | `#2d2d2d` | Panels, title bar, nav, transport |
| Surface raised | `#383838` | Device/section header strips |
| Control | `#3c3c3c` | Raised control fills, value boxes |
| Inset | `#161616` | Near-black: signal display, meters, waveforms, scrubber track |
| Line | `#131313` | Default divider (the common case) |
| Line (light) | `#404040` | The occasional brighter hairline |
| Text | `#d2d2d2` | Primary text |
| Text muted | `#8c8c8c` | Labels, secondary text |
| Text faint | `#5c5c5c` | Tertiary / disabled |
| **Orange** | `#ff8c2b` | The one accent: active states, tuning marker, primary buttons, knobs (text on orange is `#1a1a1a`) |
| **Cyan** | `#4fd0e6` | Data: spectrum trace, readout values |
| Green / Yellow / Red | `#7ec64a` / `#dcc24a` / `#d85a4e` | Meter segments; red also = record |

Spend boldness in one place: orange marks what's active or primary, cyan marks live data, and
everything else stays neutral gray. If a screen needs a third accent, it's probably doing too much.

---

## Type

- **Labels and reading text:** the system UI font.
- **Data** (frequencies, dB, counts, timestamps): JetBrains Mono with tabular figures, so digits
  line up and don't jitter as they change.
- Sentence case for labels ("Bandwidth", not "BANDWIDTH"). No wide letter-spacing, no all-caps
  micro-labels.

---

## Control vocabulary

The reusable kit in `crates/app/src/ui/`. These are the building blocks; compose screens from them.

- **Arc knob** — a flat dial with a colored value arc (orange, or cyan for data-derived params) and
  a pointer. For continuous parameters. The signature device control.
- **Segmented control** — a flat row of buttons; the active cell is an orange fill with dark text.
  For small mutually-exclusive choices (e.g. window function, dB scale auto/manual).
- **Value box** — a flat labeled readout chip (`#3c3c3c` fill, dark line) for status (mode, bandwidth, gain).
- **Dropdown** — flat select for longer discrete lists (FFT size, colormap, frame rate).
- **Segmented meter** — discrete green→yellow→red segments, horizontal (level header) or vertical
  (device edge). Read-only.
- **Flat tabs** — active tab is orange text on the background; a right hairline separates cells.
- **Icon button** — a flat bordered square holding a line icon; orange when active (e.g. play).
- **Surfaces** — `panel` (surface fill) and `inset` (near-black, bordered) for signal displays and wells.
- **Device / section header** — a raised strip with a small power dot (orange when on) and a name.

Geometry: ~2px corner radius, 1px hairline borders, no shadows. Icons are a single intentional line
set, not unicode glyphs.

---

## The frame

A fixed arrangement, sized by draggable splitters; regions don't reorder, detach, or float.

- **Nav rail** (far left) — a narrow icon column that switches the top-level workspace.
- **Signal display** (center, the focus) — a tuning header (digit-editable frequency selector + a
  level meter) above the spectrum (FFT line over an inset) and the waterfall, split by a draggable
  ratio handle. An orange center marker and a translucent bandwidth band overlay both; the spectrum
  trace is cyan. Click the spectrum or waterfall to retune.
- **Inspect panel** (right) — the observability surface, styled as a *device*: a header with a power
  dot, the selected pipeline-stage output (raw IQ → baseband → demod → bits) plus derived readouts
  (SNR, bandwidth, symbol rate), and the display/demod controls (knobs, segmented controls,
  dropdowns). Permanent, not a modal, because it's what makes a failed decode diagnosable. Collapsible.
- **Bottom pane** (tabbed) — Decoder (live decode for the active channel), Events (the live feed of
  decoded events), Channels (active channels). Collapsible.
- **Transport bar** (full width, bottom) — record/replay: transport controls, the DVR scrubber, the
  buffer depth, the record toggle, the audio sample rate. Present in every workspace.

The other workspaces (Library, Recordings, Events history) are full-frame arrangements that reuse
the same vocabulary; Listen is the live operating view described above.

---

## Waterfall colormap

The waterfall has its own perceptual colormaps (see `crates/app/src/colormap.rs`), independent of
the chrome theme, selectable by the user. The default complements the chrome: a cool **Ice** ramp
(deep blue → cyan → white) that lets the orange marker and chrome accents stand out against it.

---

State (panel sizes, last frequency, display settings) persists and restores on launch; the storage
design lives in `docs/ARCHITECTURE.md` (Persistence).
