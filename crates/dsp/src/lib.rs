//! Signal processing: FFT, filters, decimation, demodulators. Pure — no IO, hardware,
//! or async. The most heavily unit-tested crate. See `docs/ARCHITECTURE.md`.

mod fft;
mod fir;
mod fm;
mod mixer;
mod pll;
mod spectrum;
mod window;

pub use fir::{bandpass, lowpass, root_raised_cosine, FirDecimator, FirFilter};
pub use fm::FmDemod;
pub use mixer::Nco;
pub use pll::Pll;
pub use spectrum::{SpectrumAnalyzer, SpectrumConfig};
pub use window::WindowKind;
