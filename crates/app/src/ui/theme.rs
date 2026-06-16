//! The Ableton-dark theme: the app's palette as a single source of truth. `apply` writes it into
//! the gpui-component theme so every component and gpui-component widget inherits it. App-specific
//! colors that the theme has no slot for live in `AppPalette`. See `docs/UI.md`.

use gpui::{px, App, Global, Hsla};
use gpui_component::{Colorize, Theme};

// Palette (hex). Matches the table in `docs/UI.md` and `docs/style-refs/ableton.html`.
pub const BG: &str = "#1c1c1c";
pub const SURFACE: &str = "#2d2d2d";
pub const SURFACE_2: &str = "#383838";
pub const CONTROL: &str = "#3c3c3c";
pub const INSET: &str = "#161616";
pub const LINE: &str = "#131313";
pub const LINE_HI: &str = "#404040";
pub const TXT: &str = "#d2d2d2";
pub const TXT_2: &str = "#8c8c8c";
pub const ORANGE: &str = "#ff8c2b";
pub const CYAN: &str = "#4fd0e6";
pub const GREEN: &str = "#7ec64a";
pub const YELLOW: &str = "#dcc24a";
pub const RED: &str = "#d85a4e";
/// Text drawn on an orange fill.
pub const DARK_TEXT: &str = "#1a1a1a";
const KNOB_TRACK: &str = "#4a4a4a";

fn h(hex: &str) -> Hsla {
    Hsla::parse_hex(hex).expect("valid hex literal")
}

/// App-specific colors gpui-component's `ThemeColor` has no slot for. Stored as a global so any
/// component can read it the way it reads `cx.theme()`.
#[derive(Clone, Copy)]
pub struct AppPalette {
    /// Near-black background for signal displays, meters, waveforms, wells.
    pub inset: Hsla,
    /// The brighter hairline, for the occasional divider that needs contrast.
    pub line_hi: Hsla,
    /// Raised header strips.
    pub surface_2: Hsla,
    /// Raised control fills and value boxes.
    pub control: Hsla,
    /// Live-data color: spectrum trace, readout values.
    pub data: Hsla,
    pub meter_green: Hsla,
    pub meter_yellow: Hsla,
    pub meter_red: Hsla,
    /// The inactive portion of a knob's value arc.
    pub knob_track: Hsla,
    /// A knob's indicator line.
    pub pointer: Hsla,
}

impl Global for AppPalette {}

/// The app palette. Available once `apply` has run.
pub fn palette(cx: &App) -> &AppPalette {
    cx.global::<AppPalette>()
}

/// Recolor the whole app to the Ableton-dark palette. Must run AFTER `Theme::change(Dark)`, which
/// resets the theme colors to gpui-component's built-in dark defaults.
pub fn apply(cx: &mut App) {
    let orange = h(ORANGE);
    let dark_text = h(DARK_TEXT);

    let t = Theme::global_mut(cx);

    // Flat geometry, no shadows (set on Theme, not its colors).
    t.radius = px(2.);
    t.radius_lg = px(3.);
    t.shadow = false;

    // All color fields go through `t.colors`: some color names (e.g. `list`) collide with Theme's
    // own non-color fields, so the explicit path avoids ambiguity.
    let c = &mut t.colors;

    // Core surfaces + text.
    c.background = h(BG);
    c.foreground = h(TXT);
    c.muted = h(SURFACE);
    c.muted_foreground = h(TXT_2);
    c.border = h(LINE);

    // Primary accent (orange): active states, primary buttons, focus, caret, selection.
    c.primary = orange;
    c.primary_hover = orange.darken(0.06);
    c.primary_active = orange.darken(0.12);
    c.primary_foreground = dark_text;
    c.button_primary = orange;
    c.button_primary_hover = orange.darken(0.06);
    c.button_primary_active = orange.darken(0.12);
    c.button_primary_foreground = dark_text;
    c.ring = orange;
    c.caret = orange;
    c.selection = orange.opacity(0.30);

    // Accent (hover bg for menus/lists) + raised secondary controls.
    c.accent = h(SURFACE_2);
    c.accent_foreground = h(TXT);
    c.secondary = h(CONTROL);
    c.secondary_hover = h(SURFACE_2);
    c.secondary_active = h(SURFACE_2);
    c.secondary_foreground = h(TXT);

    // Status colors.
    c.danger = h(RED);
    c.danger_hover = h(RED).darken(0.06);
    c.danger_active = h(RED).darken(0.12);
    c.danger_foreground = h(TXT);
    c.success = h(GREEN);
    c.success_foreground = dark_text;
    c.warning = h(YELLOW);
    c.warning_foreground = dark_text;
    c.info = h(CYAN);
    c.info_foreground = dark_text;

    // Inputs / surfaces / overlays.
    c.input = h(LINE);
    c.title_bar = h(SURFACE);
    c.title_bar_border = h(LINE);
    c.popover = h(SURFACE);
    c.popover_foreground = h(TXT);
    c.list = h(SURFACE);
    c.list_active = orange.opacity(0.18);
    c.list_hover = h(SURFACE_2);
    c.scrollbar = h(BG);
    c.scrollbar_thumb = h(CONTROL);
    c.drag_border = orange;

    // Tabs.
    c.tab = h(SURFACE);
    c.tab_foreground = h(TXT_2);
    c.tab_active = h(BG);
    c.tab_active_foreground = orange;
    c.tab_bar = h(SURFACE);
    c.tab_bar_segmented = h(CONTROL);

    // Slider / progress (the transport scrubber).
    c.slider_bar = h(INSET);
    c.slider_thumb = orange;
    c.progress_bar = orange;

    // Base hue anchors, so any component reading a base color stays on-palette.
    c.cyan = h(CYAN);
    c.green = h(GREEN);
    c.yellow = h(YELLOW);
    c.red = h(RED);

    cx.set_global(AppPalette {
        inset: h(INSET),
        line_hi: h(LINE_HI),
        surface_2: h(SURFACE_2),
        control: h(CONTROL),
        data: h(CYAN),
        meter_green: h(GREEN),
        meter_yellow: h(YELLOW),
        meter_red: h(RED),
        knob_track: h(KNOB_TRACK),
        pointer: h(TXT),
    });
}
