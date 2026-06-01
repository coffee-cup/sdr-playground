//! The tap: a snapshot of what the reader thread last saw, read at frame rate.

/// A point-in-time view of the running pipeline. Published by the reader thread and read
/// (lock-free) by consumers. `Copy` and small — overwriting the slot is cheap and a missed
/// read has no consequence.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub center_freq: u64,
    pub sample_rate: u32,
    /// Total samples read since the source started.
    pub total_samples: u64,
    /// Measured delivery rate over the last publish window, in samples/sec.
    pub throughput_sps: f64,
    /// Mean instantaneous power (|s|²) over the last window.
    pub mean_power: f32,
    /// Peak instantaneous power (|s|²) over the last window.
    pub peak_power: f32,
    /// `mean_power` as dB relative to full scale.
    pub mean_dbfs: f32,
    /// `peak_power` as dB relative to full scale.
    pub peak_dbfs: f32,
    /// `false` once the source reaches EOF, is stopped, or errors.
    pub running: bool,
}

impl Snapshot {
    pub(crate) fn initial(center_freq: u64, sample_rate: u32) -> Self {
        Self {
            center_freq,
            sample_rate,
            total_samples: 0,
            throughput_sps: 0.0,
            mean_power: 0.0,
            peak_power: 0.0,
            mean_dbfs: f32::NEG_INFINITY,
            peak_dbfs: f32::NEG_INFINITY,
            running: true,
        }
    }
}

/// Power (|s|², full scale = 1.0) as dB relative to full scale.
pub(crate) fn to_dbfs(power: f32) -> f32 {
    if power > 0.0 {
        10.0 * power.log10()
    } else {
        f32::NEG_INFINITY
    }
}
