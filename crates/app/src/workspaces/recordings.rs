use gpui::*;

use crate::app::SdrApp;
use crate::workspaces::placeholder;

/// Saved IQ captures (DVR snapshots and manual recordings), each openable as a file
/// `Source` to replay through the live pipeline (see `docs/UI.md`).
pub fn render(cx: &mut Context<SdrApp>) -> impl IntoElement {
    placeholder("Recordings", "Saved IQ captures — coming soon", cx)
}
