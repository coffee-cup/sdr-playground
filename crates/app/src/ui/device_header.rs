//! An Ableton-style device header: a raised strip with a small power dot (orange when on) and a
//! name, optionally with a muted sub-label on the right. Used as the top strip of the inspect panel.

use gpui::{div, px, App, FontWeight, IntoElement, ParentElement, Styled};
use gpui_component::ActiveTheme;

use crate::ui::{palette, tokens};

/// A device/section header. `powered` lights the dot in the accent color.
pub fn device_header(name: &str, sub: Option<&str>, powered: bool, cx: &App) -> impl IntoElement {
    let dot = if powered {
        cx.theme().primary
    } else {
        cx.theme().muted_foreground
    };

    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .h(tokens::HEADER_H)
        .px_2()
        .bg(palette(cx).surface_2)
        .border_b_1()
        .border_color(cx.theme().border)
        .child(div().size(px(9.)).rounded_full().bg(dot))
        .child(
            div()
                .text_size(tokens::TEXT_MD)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(name.to_string()),
        );

    if let Some(sub) = sub {
        row = row.child(
            div()
                .ml_auto()
                .text_size(tokens::TEXT_SM)
                .text_color(cx.theme().muted_foreground)
                .child(sub.to_string()),
        );
    }
    row
}
