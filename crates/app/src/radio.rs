//! Connection state for the radio. The app opens a device asynchronously on launch — USB
//! enumeration and `open` block, so they run off the foreground — and this enum tracks where
//! that process is, so the Listen view can show a live display, a "searching" state, a
//! no-device prompt, or an error with a Retry. See `docs/UI.md`.

use std::sync::Arc;

use sdr_engine::Engine;

pub enum RadioState {
    /// Enumerating or opening a device.
    Connecting,
    /// A device is open and the engine is running.
    Running(Arc<Engine>),
    /// No RTL-SDR was found.
    NoDevice,
    /// Enumeration or open failed.
    Failed(String),
}

impl RadioState {
    pub fn engine(&self) -> Option<&Arc<Engine>> {
        match self {
            RadioState::Running(engine) => Some(engine),
            _ => None,
        }
    }
}
