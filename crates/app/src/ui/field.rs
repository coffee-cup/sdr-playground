//! Label/value building blocks for panels: a section label, a label+control row, and a key/value
//! row (muted key, mono value).

use gpui::{div, AnyElement, App, IntoElement, ParentElement, Styled};
use gpui_component::ActiveTheme;

use crate::ui::tokens;

/// An inline section divider/label inside a panel.
pub fn section_label(label: &str, cx: &App) -> impl IntoElement {
    div()
        .pt_2()
        .text_size(tokens::TEXT_SM)
        .text_color(cx.theme().muted_foreground)
        .child(label.to_string())
}

/// A label/control row: muted label on the left, the control on the right.
pub fn field_row(label: &str, control: AnyElement, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .h(tokens::HEADER_H)
        .child(
            div()
                .text_size(tokens::TEXT_SM)
                .text_color(cx.theme().muted_foreground)
                .child(label.to_string()),
        )
        .child(control)
}

/// A key/value row: muted key on the left, mono value on the right.
pub fn kv_row(key: &str, value: &str, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .justify_between()
        .text_size(tokens::TEXT_SM)
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(key.to_string()),
        )
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_color(cx.theme().foreground)
                .child(value.to_string()),
        )
}
