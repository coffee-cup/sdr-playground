//! Scans the FM band for RDS station/song data. Rides the same `engine` as the GUI app and
//! headless CLI: it tunes windows, registers a decode channel per station, and folds the
//! engine's decoded events into a station table. See `docs/ARCHITECTURE.md`.

pub mod band;
pub mod scan;
pub mod store;
pub mod tui;

pub use band::{plan_windows, Region, Window};
pub use scan::Scanner;
pub use store::{Station, StationTable};
