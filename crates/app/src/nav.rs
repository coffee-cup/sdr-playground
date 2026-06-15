use gpui::*;
use gpui_component::{ActiveTheme, Icon, IconName};
use serde::{Deserialize, Serialize};

use crate::app::SdrApp;

/// Top-level workspaces switched by the nav rail. Each is a full-frame arrangement
/// (see `docs/UI.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    pub fn title(self) -> &'static str {
        match self {
            Workspace::Listen => "Listen",
            Workspace::Library => "Library",
            Workspace::Recordings => "Recordings",
            Workspace::Settings => "Settings",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Workspace::Listen => "listen",
            Workspace::Library => "library",
            Workspace::Recordings => "recordings",
            Workspace::Settings => "settings",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Workspace::Listen => IconName::Frame,
            Workspace::Library => IconName::BookOpen,
            Workspace::Recordings => IconName::HardDrive,
            Workspace::Settings => IconName::Settings,
        }
    }
}

/// The nav rail: a narrow always-visible icon column that switches the top-level
/// workspace. The active item gets a subtle raised fill and a left accent edge.
pub fn render(active: Workspace, cx: &mut Context<SdrApp>) -> impl IntoElement {
    let border = cx.theme().border;
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let raised = cx.theme().secondary;
    let accent = cx.theme().primary;

    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .w(px(48.))
        .h_full()
        .py_2()
        .border_r_1()
        .border_color(border)
        .children(Workspace::ALL.map(|ws| {
            let is_active = ws == active;
            let mut cell = div()
                .id(ws.id())
                .relative()
                .flex()
                .items_center()
                .justify_center()
                .size(px(36.))
                .cursor_pointer()
                .text_color(if is_active { foreground } else { muted })
                .child(Icon::new(ws.icon()).size_4());
            if is_active {
                cell = cell.bg(raised).child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(6.))
                        .bottom(px(6.))
                        .w(px(2.))
                        .bg(accent),
                );
            }
            cell.on_click(cx.listener(move |this, _, _, cx| this.activate(ws, cx)))
        }))
}
