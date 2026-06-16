//! A small in-house component library: themed, reusable building blocks layered on GPUI and
//! gpui-component so the app's look stays consistent. Colors and fonts come from the active
//! theme; this module owns the design tokens (sizing, typography) and the shared widgets.

pub mod device_header;
pub mod dropdown;
pub mod field;
pub mod icon_button;
pub mod knob;
pub mod meter;
pub mod segmented;
pub mod surface;
pub mod tabs;
pub mod theme;
pub mod tokens;
pub mod value_box;

pub use device_header::device_header;
pub use dropdown::{dropdown, DropdownState};
pub use field::{field_row, kv_row, section_label};
pub use icon_button::icon_button;
pub use knob::knob;
pub use meter::{segmented_meter, MeterDir};
pub use segmented::segmented;
pub use surface::inset;
pub use tabs::tab_strip;
pub use theme::palette;
pub use value_box::value_box;
