//! Flat surfaces. An `inset` is the near-black well used for sunken areas (the inspect waveform,
//! the transport scrubber track): flat, with a hairline border and the theme's small radius.

use gpui::{div, App, Div, Styled};
use gpui_component::ActiveTheme;

use crate::ui::{palette, tokens};

/// A near-black inset well with a hairline border, for signal displays and value wells.
pub fn inset(cx: &App) -> Div {
    div()
        .bg(palette(cx).inset)
        .border_1()
        .border_color(cx.theme().border)
        .rounded(tokens::RADIUS)
}
