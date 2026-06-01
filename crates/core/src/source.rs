//! The `Source` trait: anything that produces raw IQ.
//!
//! Live hardware and recorded files implement the same trait, so the pipeline is identical
//! whether samples come from the antenna or from disk. See `docs/ARCHITECTURE.md`.

use crate::{Iq, Result};

/// A producer of raw IQ samples.
///
/// `Send` because the engine owns a source on a dedicated reader thread.
pub trait Source: Send {
    /// Samples per second the source is configured to deliver.
    fn sample_rate(&self) -> u32;

    /// Current center (tuned) frequency in Hz.
    fn center_freq(&self) -> u64;

    /// Retune to `hz`. For a file source this updates the reported frequency only.
    fn tune(&mut self, hz: u64) -> Result<()>;

    /// Fill `out` with up to `out.len()` samples, returning how many were written.
    ///
    /// `Ok(0)` signals end of stream (e.g. a file source reaching EOF), following the
    /// `std::io::Read` convention; real failures are returned as [`Err`](crate::Error).
    fn read(&mut self, out: &mut [Iq]) -> Result<usize>;
}
