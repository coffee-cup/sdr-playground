//! Shared types and traits: the IQ sample type, the central `Source` trait, and the
//! crate-wide error type.
//!
//! This is the root of the dependency graph and stays IO/async/UI-free. Its only
//! dependency is `num-complex`, which provides the sample type that is consistent across
//! the whole stack (rustfft re-exports the same `Complex`). See `docs/ARCHITECTURE.md`.

pub mod error;
pub mod source;

pub use error::{Error, Result};
pub use num_complex::Complex;
pub use source::Source;

/// A single complex baseband sample. The unit of everything that flows through the pipeline.
pub type Iq = Complex<f32>;
