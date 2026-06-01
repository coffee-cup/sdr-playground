use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::{title_bar, transport_bar};
use crate::nav::{self, Workspace};
use crate::workspaces;

/// Root view. Owns the active workspace and lays out the persistent frame:
/// the integrated title bar, then a body of [nav rail | active workspace], then
/// the transport bar pinned along the bottom (present in every workspace).
///
/// The frame arrangement is described in `docs/UI.md`.
pub struct SdrApp {
    active: Workspace,
}

impl SdrApp {
    pub fn new(_window: &mut Window, _cx: &mut Context<Self>) -> Self {
        Self {
            active: Workspace::Listen,
        }
    }

    pub fn activate(&mut self, workspace: Workspace, cx: &mut Context<Self>) {
        if self.active != workspace {
            self.active = workspace;
            cx.notify();
        }
    }
}

impl Render for SdrApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;

        let content = match self.active {
            Workspace::Listen => workspaces::listen::render(cx).into_any_element(),
            Workspace::Library => workspaces::library::render(cx).into_any_element(),
            Workspace::Recordings => workspaces::recordings::render(cx).into_any_element(),
            Workspace::Settings => workspaces::settings::render(cx).into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .text_size(px(13.))
            .bg(background)
            .text_color(foreground)
            .child(title_bar::render(self.active, cx))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(nav::render(self.active, cx))
                    .child(div().flex().flex_1().overflow_hidden().child(content)),
            )
            .child(transport_bar::render(cx))
    }
}
