//! RDS (Radio Data System) decoder: the FM multiplex carries a 57 kHz data subcarrier with
//! the station name, program type, and free-form RadioText. This turns that subcarrier into
//! structured [`RdsEvent`]s. See the submodules for the physical layer (`demod`), block sync
//! and error detection (`sync`), and group parsing (`group`, `text`).

mod demod;
mod group;
mod sync;
mod text;
mod types;

pub use types::{pty_name, RdsEvent};

use crate::{Decoder, Event};
use demod::Demod;
use group::Groups;
use sync::BlockSync;

/// Decodes RDS from a stream of FM multiplex samples (the real output of FM demodulation).
pub struct RdsDecoder {
    demod: Demod,
    sync: BlockSync,
    groups: Groups,
    bits: Vec<u8>,
}

impl RdsDecoder {
    /// `sample_rate` is the rate of the multiplex samples fed to [`feed`](Decoder::feed).
    pub fn new(sample_rate: u32) -> Self {
        Self {
            demod: Demod::new(sample_rate),
            sync: BlockSync::new(),
            groups: Groups::new(),
            bits: Vec::new(),
        }
    }

    /// Whether the 19 kHz pilot is locked, a precondition for decoding.
    pub fn pilot_locked(&self) -> bool {
        self.demod.pilot_locked()
    }

    /// Whether block synchronization has been acquired.
    pub fn synced(&self) -> bool {
        self.sync.synced()
    }
}

impl Decoder for RdsDecoder {
    fn feed(&mut self, samples: &[f32]) -> Vec<Event> {
        let mut events = Vec::new();
        self.bits.clear();
        self.demod.process(samples, &mut self.bits);
        for &bit in &self.bits {
            if let Some(block) = self.sync.push(bit) {
                self.groups.push_block(block, &mut events);
            }
        }
        events
    }
}
