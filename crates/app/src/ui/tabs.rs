//! A flat tab strip: the active tab is the accent color on the app background; cells are separated
//! by a hairline. Display-only for now (the bottom-pane tabs don't switch yet); add an `on_select`
//! when tab-switching is wired.

use gpui::{div, App, IntoElement, ParentElement, Styled};
use gpui_component::ActiveTheme;

use crate::ui::tokens;

/// A row of tabs with `active` highlighted in the accent color.
pub fn tab_strip(labels: &[&'static str], active: usize, cx: &App) -> impl IntoElement {
    let accent = cx.theme().primary;
    let muted = cx.theme().muted_foreground;
    let line = cx.theme().border;

    div()
        .flex()
        .flex_row()
        .h(tokens::HEADER_H)
        .border_b_1()
        .border_color(line)
        .children(labels.iter().enumerate().map(|(i, label)| {
            div()
                .flex()
                .items_center()
                .h_full()
                .px_3()
                .border_r_1()
                .border_color(line)
                .text_size(tokens::TEXT_MD)
                .text_color(if i == active { accent } else { muted })
                .child(*label)
        }))
}
