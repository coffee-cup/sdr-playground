//! The band-scan scheduler: drive the engine across windows, dwelling on each long enough for
//! RDS to trickle in, folding decoded events into the shared station table.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sdr_engine::{ChannelSpec, Engine};

use crate::band::{plan_windows, Region, Window};
use crate::store::StationTable;

/// Time discarded after a retune for the tuner to settle and channel filters to fill.
const SETTLE: Duration = Duration::from_millis(300);

pub struct Scanner {
    engine: Engine,
    windows: Vec<Window>,
    table: StationTable,
    dwell: Duration,
    /// Center frequency of the window currently being scanned (for the UI).
    current: Arc<AtomicU64>,
}

impl Scanner {
    pub fn new(engine: Engine, region: Region, sample_rate: u32, dwell: Duration) -> Self {
        let windows = plan_windows(&region.channels(), sample_rate);
        Self {
            engine,
            windows,
            table: StationTable::new(),
            dwell,
            current: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn table(&self) -> StationTable {
        self.table.clone()
    }

    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// Center frequency of the window being scanned right now (0 before the first).
    pub fn current(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.current)
    }

    /// Tune to a window and decode every station in it for `dwell`, folding events into the table.
    pub fn dwell_window(&self, w: &Window, stop: &AtomicBool) {
        self.current.store(w.center, Ordering::Relaxed);
        self.engine.tune(w.center);
        let specs: Vec<ChannelSpec> = w.offsets().into_iter().map(ChannelSpec::rds).collect();
        self.engine.set_channels(specs);

        // Drop the settle period, then collect for the rest of the dwell.
        thread::sleep(SETTLE.min(self.dwell));
        let _ = self.engine.drain_events();
        let deadline = Instant::now() + self.dwell;
        while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
            for ev in self.engine.drain_events() {
                self.table.apply(w.center, &ev);
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// Sweep the whole band repeatedly until `stop` is set.
    pub fn run(&self, stop: Arc<AtomicBool>) {
        while !stop.load(Ordering::SeqCst) {
            for w in &self.windows {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                self.dwell_window(w, &stop);
            }
        }
    }
}
