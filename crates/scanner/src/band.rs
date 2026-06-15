//! FM broadcast channel grids and window planning.
//!
//! A single RTL-SDR captures one ~2 MHz window, but the FM band is ~20 MHz, so the scanner
//! covers it as a sequence of windows. This computes the station grid for a region and packs
//! the stations into the fewest windows the device bandwidth allows.

/// FM band plan for a region: where stations can sit on the dial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// North America: 87.9–107.9 MHz on a 200 kHz grid.
    Us,
    /// ITU/Europe: 87.5–108.0 MHz on a 100 kHz grid.
    Eu,
}

impl Region {
    fn grid(self) -> (u64, u64, u64) {
        match self {
            Region::Us => (87_900_000, 107_900_000, 200_000),
            Region::Eu => (87_500_000, 108_000_000, 100_000),
        }
    }

    /// Every candidate station center frequency (Hz) on the dial.
    pub fn channels(self) -> Vec<u64> {
        let (lo, hi, step) = self.grid();
        (0..=(hi - lo) / step).map(|i| lo + i * step).collect()
    }
}

/// One device tuning: a center frequency and the stations that fall within its window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub center: u64,
    pub stations: Vec<u64>,
}

impl Window {
    /// Station offsets from the window center, in Hz (signed), for `Engine::set_channels`.
    pub fn offsets(&self) -> Vec<f64> {
        self.stations
            .iter()
            .map(|&s| s as f64 - self.center as f64)
            .collect()
    }
}

/// Pack `channels` (ascending) into windows no wider than the usable device bandwidth, which is
/// taken as 80% of the sample rate to stay clear of the anti-alias filter skirts.
pub fn plan_windows(channels: &[u64], sample_rate: u32) -> Vec<Window> {
    let usable = (sample_rate as f64 * 0.8) as u64;
    let half = usable / 2;
    let mut windows = Vec::new();
    let mut i = 0;
    while i < channels.len() {
        // Start a window at the first uncovered station; gather everything within `usable`.
        let start = channels[i];
        let mut j = i;
        while j < channels.len() && channels[j] - start < usable {
            j += 1;
        }
        let stations = channels[i..j].to_vec();
        // Center the window between the first and last station it covers.
        let center = (stations[0] + stations[stations.len() - 1]) / 2;
        // Guard: the span must fit the window (it does by construction since span < usable).
        debug_assert!(stations[stations.len() - 1] - stations[0] <= 2 * half);
        windows.push(Window { center, stations });
        i = j;
    }
    windows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn us_grid_spans_the_band() {
        let ch = Region::Us.channels();
        assert_eq!(ch[0], 87_900_000);
        assert_eq!(*ch.last().unwrap(), 107_900_000);
        assert_eq!(ch.len(), 101); // (107.9 - 87.9) / 0.2 + 1
                                   // All on the 200 kHz grid.
        assert!(ch.windows(2).all(|w| w[1] - w[0] == 200_000));
    }

    #[test]
    fn windows_cover_every_station_within_bandwidth() {
        let ch = Region::Us.channels();
        let windows = plan_windows(&ch, 2_400_000);
        // Every station appears exactly once, in order.
        let flat: Vec<u64> = windows.iter().flat_map(|w| w.stations.clone()).collect();
        assert_eq!(flat, ch);
        // No window exceeds the usable bandwidth, and offsets stay within half of it.
        let half = 2_400_000.0 * 0.8 / 2.0;
        for w in &windows {
            for off in w.offsets() {
                assert!(off.abs() <= half, "offset {off} exceeds half-bandwidth");
            }
        }
        // ~20 MHz / ~1.9 MHz usable => about a dozen windows.
        assert!(
            (8..=14).contains(&windows.len()),
            "got {} windows",
            windows.len()
        );
    }
}
