# Agent Instructions

A software-defined radio for macOS. Read `docs/` before doing anything.

## Docs (read first)

- `docs/VISION.md` — why this exists and the principles every decision answers to.
- `docs/ARCHITECTURE.md` — system shape: the pipeline, the realtime/orchestration split, the crate graph.
- `docs/UI.md` — the app frame, regions, and workspaces.

Read the relevant doc before changing code. If a change alters the architecture, the
signal pipeline, or the UI frame, update the matching doc in the **same** change. Docs
describe shape and decisions; code is the implementation. Keep them in sync.

## Commands (mise)

Everything runs through mise — the Rust toolchain is provisioned by `mise.toml`, so do
not assume a system `cargo`. Run cargo only via these tasks:

- `mise run build` — build the workspace
- `mise run test` — tests
- `mise run check` — fmt check + clippy (the CI gate; run before committing)
- `mise run fmt` — format
- `mise run cli -- <args>` — run the headless `sdr` CLI (e.g. `mise run cli -- device list`)

Do not run `mise run app` (it launches the GUI app) — that is for the user to run, not
the agent. To verify a change, build and let the user run it.

## Code style

- Minimum effective abstraction. No layer of indirection without a caller that pays for it.
- Comments explain **why** for a future senior engineer reading cold. No narration of the
  change, no restating what the code already says, nothing tied to this session.
- Comments should NEVER include em-dashes
- Match the style of the surrounding code.

## Workspace

Dependency direction points inward and no core crate may know about the UI:

```
core <- dsp / device / decode <- engine <- { app, cli }
```

## UI

The UI is GPUI + gpui-component. Before building or changing UI, use the `gpui-component`
skill (`.agents/skills/gpui-component/`) and the component gallery:
https://longbridge.github.io/gpui-component/docs/components/

Follow the look and feel in `docs/UI.md` (flat Ableton-dark: warm-gray surfaces, near-black
insets, orange accent + cyan data). The canonical visual reference is `docs/style-refs/ableton.html`.

- **Use the reusable component kit in `crates/app/src/ui/`** — `inset`, `value_box`, `segmented`,
  `tab_strip`, `icon_button`, `segmented_meter`, `device_header`, `field_row`/`kv_row`/`section_label`,
  `knob`, `dropdown`. Compose from these; don't hand-roll inline panels/buttons/rows. If a new pattern
  recurs, add it to the kit rather than duplicating it.
- **Never hardcode color.** Read it from the theme (`cx.theme()`) or the app palette (`ui::palette(cx)`
  for inset/data/meter colors). The palette is defined once in `ui/theme.rs` and written into the
  gpui-component theme; widen that, not call sites.
- Mono font (JetBrains Mono) is for numeric data only; labels use the UI font, sentence case.
