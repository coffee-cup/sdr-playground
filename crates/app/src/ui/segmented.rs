//! A flat segmented control: a row of cells where the selected one is an orange fill with dark
//! text and the rest are control-fill with muted text. For small mutually-exclusive choices.

use gpui::*;
use gpui_component::ActiveTheme;

use crate::app::SdrApp;
use crate::ui::{palette, tokens};

/// A segmented control over `options` (label, value). Clicking a cell runs `on_select` with its
/// value. `id` namespaces the per-cell element ids.
pub fn segmented<T: PartialEq + Copy + 'static>(
    id: &'static str,
    options: &[(&'static str, T)],
    selected: T,
    cx: &mut Context<SdrApp>,
    on_select: impl Fn(&mut SdrApp, T, &mut Window, &mut Context<SdrApp>) + 'static + Copy,
) -> impl IntoElement {
    let primary = cx.theme().primary;
    let on_primary = cx.theme().primary_foreground;
    let control = palette(cx).control;
    let muted = cx.theme().muted_foreground;
    let line = cx.theme().border;

    div()
        .flex()
        .flex_row()
        .gap(px(2.))
        .children(options.iter().enumerate().map(|(i, &(label, value))| {
            let active = value == selected;
            div()
                .id((id, i))
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .px_2()
                .py(px(4.))
                .rounded(tokens::RADIUS)
                .border_1()
                .border_color(line)
                .text_size(tokens::TEXT_SM)
                .bg(if active { primary } else { control })
                .text_color(if active { on_primary } else { muted })
                .cursor_pointer()
                .child(label)
                .on_click(cx.listener(move |app, _, window, cx| on_select(app, value, window, cx)))
        }))
}
