# sdr-playground

[![CI](https://github.com/coffee-cup/sdr-playground/actions/workflows/ci.yml/badge.svg)](https://github.com/coffee-cup/sdr-playground/actions/workflows/ci.yml)

A software-defined radio for macOS, built around the RTL-SDR V3. A modern,
keyboard-first SDR with first-class decoders and a scrubbable IQ buffer.

See [`docs/`](docs/) for the why ([VISION](docs/VISION.md)), the system shape
([ARCHITECTURE](docs/ARCHITECTURE.md)), and the layout ([UI](docs/UI.md)).

## Develop

The toolchain is provisioned by [mise](https://mise.jdx.dev); all commands run
through it.

```sh
mise run run     # launch the app
mise run build   # build the workspace
mise run test    # tests
mise run check   # fmt check + clippy (the CI gate)
```

## Workspace

Dependencies point inward; no core crate knows about the UI.

```
core <- dsp / device / decode <- engine <- { app, cli }
```

- `core` — shared types and traits
- `dsp` — FFT, filters, demodulators (pure)
- `device` — `Source` implementations (hardware, file replay)
- `decode` — decoder tails
- `engine` — the runtime; UI-agnostic, IO injected
- `app` — GPUI front-end
- `cli` — headless front-end and decoder test harness

The UI uses [GPUI](https://github.com/zed-industries/zed) +
[gpui-component](https://github.com/longbridge/gpui-component); see the
`gpui-component` skill before building UI.
