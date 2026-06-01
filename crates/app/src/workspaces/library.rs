use gpui::*;

use crate::app::SdrApp;
use crate::workspaces::placeholder;

/// Frequency database (bookmarks): a searchable, filterable table where a row click
/// tunes to it (see `docs/UI.md`). Backed by SQLite via `engine` once that lands.
pub fn render(cx: &mut Context<SdrApp>) -> impl IntoElement {
    placeholder("Library", "Frequency database — coming soon", cx)
}
