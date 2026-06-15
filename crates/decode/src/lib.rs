//! Decoder tails. Reuses `dsp` filters and demodulators and turns demodulated signals into
//! structured [`Event`]s. Pure: no IO, hardware, or async. Tested against synthetic signals
//! and recorded fixtures. See `docs/ARCHITECTURE.md`.

pub mod rds;

pub use rds::{pty_name, RdsDecoder, RdsEvent};

/// Consumes a stream of real demodulated samples (FM multiplex or audio) and emits structured
/// events. One trait object per channel; the engine owns it and feeds it demodulated blocks.
pub trait Decoder: Send {
    /// Feed a block of demodulated samples; return events recognized within it.
    fn feed(&mut self, samples: &[f32]) -> Vec<Event>;
}

/// A decoded event, tagged by source decoder. New decoders add variants.
#[derive(Debug, Clone)]
pub enum Event {
    Rds(RdsEvent),
}
