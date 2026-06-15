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
    /// Hard cap on time spent per window.
    dwell: Duration,
    /// Leave a window early once it has gone this long with no new station info.
    quiet: Duration,
    /// Center frequency of the window currently being scanned (for the UI).
    current: Arc<AtomicU64>,
}

impl Scanner {
    pub fn new(
        engine: Engine,
        region: Region,
        sample_rate: u32,
        dwell: Duration,
        quiet: Duration,
    ) -> Self {
        let windows = plan_windows(&region.channels(), sample_rate);
        Self {
            engine,
            windows,
            table: StationTable::new(),
            dwell,
            quiet,
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

        // Drop the settle period, then discard whatever arrived during the retune.
        thread::sleep(SETTLE.min(self.dwell));
        let _ = self.engine.drain_events();

        // Adaptive dwell: keep listening while new station info arrives, but leave after `quiet`
        // with nothing new (an empty or fully-decoded window) and never exceed `dwell`. This is
        // what makes a full sweep fast: most windows are empty or settle in a few seconds, so we
        // do not burn the whole cap on dead air.
        let start = Instant::now();
        let mut last_progress = start;
        while !stop.load(Ordering::SeqCst) && start.elapsed() < self.dwell {
            let mut progressed = false;
            for ev in self.engine.drain_events() {
                progressed |= self.table.apply(w.center, &ev);
            }
            if progressed {
                last_progress = Instant::now();
            } else if last_progress.elapsed() >= self.quiet {
                break;
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
