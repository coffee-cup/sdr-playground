//! A flat status chip: a small control-fill box with a muted label and a value. Used for the
//! read-only status in the title bar (sample rate, bandwidth).

use gpui::{div, App, IntoElement, ParentElement, SharedString, Styled};
use gpui_component::ActiveTheme;

use crate::ui::{palette, tokens};

/// A flat labeled value chip.
pub fn value_box(label: &str, value: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_baseline()
        .gap_1()
        .px_2()
        .py(gpui::px(2.))
        .rounded(tokens::RADIUS)
        .bg(palette(cx).control)
        .border_1()
        .border_color(cx.theme().border)
        .text_size(tokens::TEXT_SM)
        .text_color(cx.theme().muted_foreground)
        .child(label.to_string())
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().foreground)
                .child(value.into()),
        )
}
