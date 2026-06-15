//! A small in-house component library: themed, reusable building blocks layered on GPUI and
//! gpui-component so the app's look stays consistent. Colors and fonts come from the active
//! theme; this module owns the design tokens (sizing, typography) and the shared widgets.

pub mod dropdown;
pub mod tokens;

pub use dropdown::{dropdown, DropdownState};
