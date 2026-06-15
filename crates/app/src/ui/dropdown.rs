//! A themed single-select dropdown wrapping gpui-component's `Select`. It keeps each option's
//! typed value alongside its label, so a confirmed selection maps straight back to a value the
//! caller can act on. The selection state lives in an entity (held by the parent view); the
//! parent subscribes to `SelectEvent::Confirm` to react.

use gpui::*;
use gpui_component::select::{SearchableVec, Select, SelectState};
use gpui_component::{IndexPath, Sizable};

use crate::ui::tokens;

/// Persistent state for one dropdown: the `Select` entity plus the label→value table.
pub struct DropdownState<T: Clone> {
    pub state: Entity<SelectState<SearchableVec<SharedString>>>,
    options: Vec<(SharedString, T)>,
}

impl<T: Clone> DropdownState<T> {
    /// Build a dropdown over `options` (label, value), with `selected` pre-selected.
    pub fn new(
        options: Vec<(SharedString, T)>,
        selected: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let labels: Vec<SharedString> = options.iter().map(|(l, _)| l.clone()).collect();
        let idx = selected.min(labels.len().saturating_sub(1));
        let state = cx.new(|cx| {
            SelectState::new(
                SearchableVec::new(labels),
                Some(IndexPath::new(idx)),
                window,
                cx,
            )
        });
        Self { state, options }
    }

    /// The value paired with a confirmed label.
    pub fn value_for(&self, label: &SharedString) -> Option<T> {
        self.options
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, v)| v.clone())
    }
}

/// The dropdown element for a `DropdownState`, sized for the settings panel.
pub fn dropdown<T: Clone>(state: &DropdownState<T>) -> impl IntoElement {
    div()
        .w(px(130.))
        .text_size(tokens::TEXT_MD)
        .child(Select::new(&state.state).small().menu_width(px(150.)))
}
