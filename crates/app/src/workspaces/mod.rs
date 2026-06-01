//! Each workspace is a full-frame arrangement selected by the nav rail (see `docs/UI.md`).
//! Only `listen` carries real layout today; the rest are scaffolded placeholders.

pub mod library;
pub mod listen;
pub mod recordings;
pub mod settings;

use gpui::*;
use gpui_component::ActiveTheme;

use crate::app::SdrApp;

/// A centered title + subtitle, used by workspaces that are not built out yet.
pub(crate) fn placeholder(
    title: &'static str,
    subtitle: &'static str,
    cx: &mut Context<SdrApp>,
) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;

    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .size_full()
        .child(div().text_lg().child(title))
        .child(div().text_sm().text_color(muted).child(subtitle))
}
