use gpui::*;

use crate::app::SdrApp;
use crate::workspaces::placeholder;

pub fn render(cx: &mut Context<SdrApp>) -> impl IntoElement {
    placeholder(
        "Settings",
        "Device, audio, and appearance — coming soon",
        cx,
    )
}
