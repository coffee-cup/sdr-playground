//! Signal processing: FFT, filters, decimation, demodulators. Pure — no IO, hardware,
//! or async. The most heavily unit-tested crate. See `docs/ARCHITECTURE.md`.

mod fft;
mod spectrum;
mod window;

pub use spectrum::{SpectrumAnalyzer, SpectrumConfig};
pub use window::WindowKind;
