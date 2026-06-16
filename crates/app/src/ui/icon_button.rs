//! A flat icon button: a bordered control-fill square holding a line icon, accent-colored when
//! active. Used for transport controls.

use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName};

use crate::app::SdrApp;
use crate::ui::tokens;

/// A flat icon button. `active` tints the icon with the accent color (e.g. the play button while
/// live). `on_click` runs against the app.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: IconName,
    active: bool,
    cx: &mut gpui::Context<SdrApp>,
    on_click: impl Fn(&mut SdrApp, &mut Window, &mut gpui::Context<SdrApp>) + 'static,
) -> impl IntoElement {
    let fg = if active {
        cx.theme().primary
    } else {
        cx.theme().foreground
    };

    div()
        .id(id.into())
        .flex()
        .items_center()
        .justify_center()
        .size(px(24.))
        .rounded(tokens::RADIUS)
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .cursor_pointer()
        .child(Icon::new(icon).size_4().text_color(fg))
        .on_click(cx.listener(move |app, _, window, cx| on_click(app, window, cx)))
}
