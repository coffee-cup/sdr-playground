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
    /// Structured now-playing from RT+ (when the station sends it), carved out of the RadioText.
    pub title: Option<String>,
    pub artist: Option<String>,
    /// Long PS (RDS2): a station name longer than the 8-char PS, when broadcast.
    pub long_ps: Option<String>,
    /// PTYN: a free-form refinement of the program-type genre.
    pub ptyn: Option<String>,
    /// The station's clock (ISO-8601), when broadcast.
    pub clock: Option<String>,
}

impl Station {
    fn new(freq: u64) -> Self {
        Self {
            freq,
            pi: None,
            pty: None,
            program_service: None,
            radiotext: None,
            title: None,
            artist: None,
            long_ps: None,
            ptyn: None,
            clock: None,
        }
    }

    /// Now-playing line: the RT+ "Artist - Title" when available, else the raw RadioText.
    pub fn now_playing(&self) -> Option<String> {
        match (&self.artist, &self.title) {
            (Some(a), Some(t)) => Some(format!("{a} - {t}")),
            (Some(a), None) => Some(a.clone()),
            (None, Some(t)) => Some(t.clone()),
            (None, None) => self.radiotext.clone(),
        }
    }

    /// Station name to display: the Long PS when broadcast, else the 8-char PS.
    pub fn name(&self) -> Option<&str> {
        self.long_ps.as_deref().or(self.program_service.as_deref())
    }

    /// Program-type label: the station's own PTYN when broadcast, else the standard PTY name.
    pub fn type_label(&self) -> Option<&str> {
        self.ptyn.as_deref().or_else(|| self.pty_name())
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
            Event::Rds(RdsEvent::RadioTextPlus { title, artist }) => {
                let t = title
                    .as_ref()
                    .is_some_and(|t| set_changed(&mut s.title, t.clone()));
                let a = artist
                    .as_ref()
                    .is_some_and(|a| set_changed(&mut s.artist, a.clone()));
                t || a
            }
            Event::Rds(RdsEvent::LongProgramService(name)) => {
                set_changed(&mut s.long_ps, name.clone())
            }
            Event::Rds(RdsEvent::ProgramTypeName(name)) => set_changed(&mut s.ptyn, name.clone()),
            Event::Rds(RdsEvent::ClockTime(ct)) => set_changed(&mut s.clock, ct.clone()),
        }
    }

    /// Snapshot of all known stations, ordered by frequency.
    pub fn stations(&self) -> Vec<Station> {
        self.inner.lock().unwrap().values().cloned().collect()
    }
}
