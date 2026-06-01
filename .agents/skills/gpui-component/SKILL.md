---
name: gpui-component
description: Build and edit UI in this repo with GPUI + gpui-component. Use whenever writing or changing anything under crates/app, working with views/elements/layout/theming, or wiring components (buttons, inputs, tables, docks, dialogs).
---

# Building UI with GPUI + gpui-component

The app (`crates/app`) renders with [GPUI](https://github.com/zed-industries/zed) (Zed's
Rust UI framework) and [gpui-component](https://github.com/longbridge/gpui-component) (a
component library on top of it). This skill is the fast path to writing correct UI.

## References (open these first)

- **Component gallery (what exists, what it looks like):** https://longbridge.github.io/gpui-component/docs/components/
- **Source + examples (how to call it):** https://github.com/longbridge/gpui-component
  - Real, compilable examples: `examples/` (e.g. `hello_world`, `sidebar`, `system_monitor`)
    and `crates/story/examples/` (e.g. `dock.rs` for the dock/panel layout). When unsure of
    an API, read the example that uses it at the pinned rev — see below.
- **GPUI itself** (elements, `div()`, styling, `Context`, entities): the `gpui` crate in
  the zed repo. The styling API mirrors Tailwind (`.flex()`, `.gap_2()`, `.px_4()`, etc.).

## Versions

Pinned in the root `Cargo.toml` `[workspace.dependencies]`. `gpui`/`gpui_platform` track a
specific zed commit; `gpui-component`/`gpui-component-assets` track a gpui-component commit
that builds against it. **Bump both together** — gpui-component follows zed's `main` and the
two drift quickly. To find the matching zed rev for a gpui-component rev, read `gpui` in
gpui-component's `Cargo.lock` at that rev.

## The shape of a GPUI app

- **Entry point** (`main.rs`): `gpui_platform::application().with_assets(Assets).run(|cx| { … })`.
  Call `gpui_component::init(cx)` before using any component. The window's top-level view
  must be a `Root` (`Root::new(view, window, cx)`) — it hosts the dialog/sheet/notification
  overlay layers.
- **Views** are entities implementing `Render`:
  ```rust
  impl Render for MyView {
      fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement { … }
  }
  ```
- **Elements** are built fluently from `div()`. Layout is flexbox: `.flex().flex_col()`,
  `.flex_1()`, `.gap_2()`, `.px_4()`, `.size_full()`, `.w(px(240.))`, `.items_center()`.
- **Theme:** `use gpui_component::ActiveTheme;` then `cx.theme().background`, `.foreground`,
  `.border`, `.muted`, `.muted_foreground`, `.secondary`, `.accent`, etc. Always pull colors
  from the theme — never hardcode — so light/dark and theming work. Color values are `Hsla`
  (Copy): grab the ones you need into locals at the top of a render fn, then build elements
  (this also sidesteps borrow conflicts with `cx.listener`).
- **Interaction:** an element needs `.id(...)` to be clickable. Mutate view state from a
  handler via `cx.listener(|this, event, window, cx| { … cx.notify(); })`.

## Conventions in this repo

- Regions render as free functions taking `&mut Context<SdrApp>` (see `crates/app/src/nav.rs`,
  `components/transport_bar.rs`, `workspaces/`). Keep one region per file.
- Layout and region responsibilities are specified in `docs/UI.md` — match it.
- Prefer gpui-component widgets (Button, Input, Table, Dock, …) over hand-rolled elements
  once a region needs real behavior; the current placeholders use plain `div()` on purpose.
- Icons: gpui-component-assets bundles Lucide icons referenced by the `IconName` enum. The
  scaffold uses unicode glyphs as placeholders; switch to `IconName` when wiring real icons.

## When an API doesn't compile

The libraries move fast and these notes can lag. Trust the compiler and the pinned source:
`git ls-remote` is not needed — read the example/source at the rev in `Cargo.toml`. Fix
against the actual signature rather than guessing repeatedly.
