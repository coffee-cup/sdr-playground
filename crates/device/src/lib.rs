//! `Source` implementations: the RTL-SDR driver and the file replay source. The only crate
//! that touches hardware. See `docs/ARCHITECTURE.md`.

pub mod convert;
pub mod file;
pub mod rtlsdr;

pub use file::FileSource;
pub use rtlsdr::{DeviceInfo, Gain, RtlConfig, RtlSdrSource};
