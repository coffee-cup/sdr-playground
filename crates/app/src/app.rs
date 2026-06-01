use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::transport_bar;
use crate::nav::{self, Workspace};
use crate::workspaces;

/// Root view. Owns the active workspace and lays out the persistent frame:
/// nav rail on the left, the active workspace filling the center, and the
/// transport bar pinned along the bottom (present in every workspace).
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;

        let content = match self.active {
            Workspace::Listen => workspaces::listen::render(window, cx).into_any_element(),
            Workspace::Library => workspaces::library::render(cx).into_any_element(),
            Workspace::Recordings => workspaces::recordings::render(cx).into_any_element(),
            Workspace::Settings => workspaces::settings::render(cx).into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(background)
            .text_color(foreground)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .overflow_hidden()
                    .child(nav::render(self.active, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .overflow_hidden()
                            .child(content),
                    ),
            )
            .child(transport_bar::render(cx))
    }
}
