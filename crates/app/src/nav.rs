use gpui::*;
use gpui_component::ActiveTheme;

use crate::app::SdrApp;

/// Top-level workspaces switched by the nav rail. Each is a full-frame arrangement
/// (see `docs/UI.md`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Workspace {
    Listen,
    Library,
    Recordings,
    Settings,
}

impl Workspace {
    const ALL: [Workspace; 4] = [
        Workspace::Listen,
        Workspace::Library,
        Workspace::Recordings,
        Workspace::Settings,
    ];

    /// Stable element id / unique label for the nav item.
    fn id(self) -> &'static str {
        match self {
            Workspace::Listen => "listen",
            Workspace::Library => "library",
            Workspace::Recordings => "recordings",
            Workspace::Settings => "settings",
        }
    }

    /// Placeholder glyph until real icons are wired (see `docs/UI.md` nav rail).
    fn glyph(self) -> &'static str {
        match self {
            Workspace::Listen => "◉",
            Workspace::Library => "▤",
            Workspace::Recordings => "⧉",
            Workspace::Settings => "⊚",
        }
    }
}

/// The nav rail: a narrow always-visible icon column that switches the top-level
/// workspace. It never holds content itself.
pub fn render(active: Workspace, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let selected_bg = cx.theme().secondary;

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_2()
        .w(px(56.))
        .h_full()
        .py_3()
        .border_r_1()
        .border_color(border)
        .children(Workspace::ALL.map(|ws| {
            let is_active = ws == active;
            let mut item = div()
                .id(ws.id())
                .flex()
                .items_center()
                .justify_center()
                .size(px(36.))
                .rounded_md()
                .cursor_pointer()
                .text_color(if is_active { foreground } else { muted })
                .child(ws.glyph());
            if is_active {
                item = item.bg(selected_bg);
            }
            item.on_click(cx.listener(move |this, _, _, cx| this.activate(ws, cx)))
        }))
}
