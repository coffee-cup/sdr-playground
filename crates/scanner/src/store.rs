//! Per-station aggregation of decoded RDS events. The scan thread writes; the UI reads.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use sdr_engine::{pty_name, ChannelEvent, Event, RdsEvent};

/// Set `field` to `val`, returning whether that changed its value.
fn set_changed<T: PartialEq>(field: &mut Option<T>, val: T) -> bool {
    let changed = field.as_ref() != Some(&val);
    *field = Some(val);
    changed
}

/// Accumulated knowledge about one station.
#[derive(Debug, Clone)]
pub struct Station {
    pub freq: u64,
    pub pi: Option<u16>,
    pub pty: Option<u8>,
    pub program_service: Option<String>,
    pub radiotext: Option<String>,
}

impl Station {
    fn new(freq: u64) -> Self {
        Self {
            freq,
            pi: None,
            pty: None,
            program_service: None,
            radiotext: None,
        }
    }

    /// Program-type name, if known.
    pub fn pty_name(&self) -> Option<&'static str> {
        self.pty.map(pty_name)
    }
}

/// A frequency-keyed table of stations, shared between the scan thread and the UI.
#[derive(Clone, Default)]
pub struct StationTable {
    inner: Arc<Mutex<BTreeMap<u64, Station>>>,
}

impl StationTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold a tagged event into the table. `center` is the tuned frequency the event's channel
    /// offset is relative to. Returns whether this added genuinely new information (a new field
    /// or a changed value), which the scan loop uses to decide when a window has gone quiet.
    pub fn apply(&self, center: u64, ev: &ChannelEvent) -> bool {
        let freq = (center as i64 + ev.offset_hz as i64).max(0) as u64;
        let mut table = self.inner.lock().unwrap();
        let s = table.entry(freq).or_insert_with(|| Station::new(freq));
        match &ev.event {
            Event::Rds(RdsEvent::Pi(v)) => set_changed(&mut s.pi, *v),
            Event::Rds(RdsEvent::ProgramType(p)) => set_changed(&mut s.pty, *p),
            Event::Rds(RdsEvent::ProgramService(ps)) => {
                set_changed(&mut s.program_service, ps.clone())
            }
            Event::Rds(RdsEvent::RadioText(rt)) => set_changed(&mut s.radiotext, rt.clone()),
        }
    }

    /// Snapshot of all known stations, ordered by frequency.
    pub fn stations(&self) -> Vec<Station> {
        self.inner.lock().unwrap().values().cloned().collect()
    }
}
