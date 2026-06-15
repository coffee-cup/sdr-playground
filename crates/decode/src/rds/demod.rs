//! RDS physical layer: FM multiplex in, data bits out.
//!
//! The chain is: lock a PLL to the 19 kHz pilot and triple its phase for a coherent 57 kHz
//! reference; mix the multiplex down to complex baseband and low-pass to the ~2.4 kHz RDS
//! band; matched-filter the biphase symbol; recover symbol timing (Gardner); then differential
//! decode. Working with complex symbols and differential decoding makes the result immune to
//! the carrier's unknown phase and the 180-degree ambiguity that biphase otherwise carries.

use std::f32::consts::{PI, TAU};

use sdr_core::Iq;
use sdr_dsp::{bandpass, lowpass, root_raised_cosine, FirDecimator, FirFilter, Pll};

/// RDS bit rate: 57000 / 48 = 1187.5 Hz, exactly the 19 kHz pilot divided by 16.
const SYMBOL_RATE: f32 = 1187.5;

/// RDS biphase (Manchester) chip rate: two chips per bit. Symbol timing recovers chips; a bit
/// is the transition between the two chips of a bit period (the biphase decode).
const CHIP_RATE: f32 = 2.0 * SYMBOL_RATE;

/// Gardner timing-loop gain (error is normalized by symbol energy first).
const TIMING_GAIN: f32 = -0.05;

/// Costas (carrier) loop bandwidth and pull-in range in Hz. The pilot-derived 57 kHz reference
/// is only approximate (the subcarrier is not always phase-locked to the pilot), so a Costas
/// loop tracks out the residual frequency and phase offset.
const COSTAS_BW_HZ: f32 = 100.0;
const COSTAS_PULL_HZ: f32 = 3_000.0;

/// Number of baseband samples used for the one-shot coarse carrier-frequency estimate before
/// the Costas loop takes over tracking.
const ACQ_LEN: u32 = 8192;

pub struct Demod {
    pll: Pll,
    sample_rate: u32,
    /// Isolates the 19 kHz pilot from the rest of the multiplex so the PLL sees a clean tone.
    pilot_bp: FirFilter,
    /// 57 kHz mix and low-pass to baseband, decimating to a few times the symbol rate.
    baseband: FirDecimator,
    /// Costas carrier-tracking loop over the complex baseband (BPSK).
    car_phase: f32,
    car_freq: f32,
    car_alpha: f32,
    car_beta: f32,
    car_freq_max: f32,
    /// One-shot coarse frequency acquisition state.
    acquired: bool,
    acq_n: u32,
    acq_sum: Iq,
    acq_prev_y2: Iq,
    /// Boxcar matched filter over one chip.
    mf: Vec<Iq>,
    mf_taps: Vec<f32>,
    mf_pos: usize,
    /// Gardner timing recovery at the chip rate. Strobes twice per chip (center + midpoint).
    sps: f32,
    mu: f32,
    half: bool,
    mid: Iq,
    last_center: Iq,
    hist: [Iq; 2],
    /// Biphase decode: a bit is the difference of consecutive chips. The even/odd chip parity
    /// that carries the bit transitions is chosen by magnitude, then differential-decoded.
    prev_chip: Iq,
    biphase_clock: u32,
    biphase_polarity: u32,
    even_sum: f32,
    odd_sum: f32,
    biphase_count: u32,
    prev_decoded: bool,
}

impl Demod {
    pub fn new(sample_rate: u32) -> Self {
        // Decimate the 57 kHz-baseband to roughly 24 kHz (about 10 samples per chip).
        let decim = (sample_rate as f32 / 24_000.0).round().max(1.0) as usize;
        let fs_b = sample_rate as f32 / decim as f32;
        let sps = fs_b / CHIP_RATE;
        // Root-raised-cosine matched filter for the biphase chip pulse (matches fmradio).
        let mf_taps = root_raised_cosine(sps, 3, 0.8);

        // Second-order Costas loop coefficients from its loop bandwidth (damping 0.707).
        let w = TAU * COSTAS_BW_HZ / fs_b;
        let d = 1.0 + 2.0 * 0.707 * w + w * w;

        Self {
            pll: Pll::new(19_000.0, sample_rate, 200.0, 0.707),
            sample_rate,
            // Pilot band-pass: 19 kHz ± 2 kHz, away from audio (< 15 kHz) and the 38 kHz stereo.
            pilot_bp: FirFilter::new(bandpass(
                201,
                19_000.0 / sample_rate as f32,
                2_000.0 / sample_rate as f32,
            )),
            // ~3 kHz low-pass on the complex 57 kHz baseband before decimation.
            baseband: FirDecimator::new(lowpass(127, 3_000.0 / sample_rate as f32), decim),
            car_phase: 0.0,
            car_freq: 0.0,
            car_alpha: 4.0 * 0.707 * w / d,
            car_beta: 4.0 * w * w / d,
            car_freq_max: TAU * COSTAS_PULL_HZ / fs_b,
            acquired: false,
            acq_n: 0,
            acq_sum: Iq::default(),
            acq_prev_y2: Iq::default(),
            mf: vec![Iq::default(); mf_taps.len()],
            mf_pos: 0,
            mf_taps,
            sps,
            mu: 0.0,
            half: false,
            mid: Iq::default(),
            last_center: Iq::default(),
            hist: [Iq::default(); 2],
            prev_chip: Iq::default(),
            biphase_clock: 0,
            biphase_polarity: 0,
            even_sum: 0.0,
            odd_sum: 0.0,
            biphase_count: 0,
            prev_decoded: false,
        }
    }

    /// Feed multiplex samples; append recovered data bits (0/1) to `bits`.
    pub fn process(&mut self, mpx: &[f32], bits: &mut Vec<u8>) {
        let mut bb = Vec::new();
        for &x in mpx {
            // Lock the PLL to the isolated pilot; the band-pass group delay is constant, so the
            // resulting carrier-phase and symbol-timing offsets are absorbed downstream.
            let pilot = self.pilot_bp.process_sample(x);
            let theta = self.pll.process_sample(pilot);
            // Coherent 57 kHz reference is the tripled pilot phase; mix to baseband.
            let c = (3.0 * theta).cos();
            let s = (3.0 * theta).sin();
            let z = Iq::new(x * c, -x * s);
            self.baseband.process(&[z], &mut bb);
        }
        for &b in &bb {
            self.push_baseband(b, bits);
        }
    }

    /// Track and remove the residual carrier offset on the complex baseband (BPSK Costas loop),
    /// rotating the data back onto the real axis.
    fn carrier(&mut self, z: Iq) -> Iq {
        self.car_phase = wrap_pi(self.car_phase + self.car_freq);
        let y = z * Iq::from_polar(1.0, -self.car_phase);

        if !self.acquired {
            // Coarse frequency estimate: squaring removes the BPSK data, and summing consecutive
            // products coherently (magnitude-weighted) yields twice the per-sample offset with no
            // bias from the biphase amplitude. Unambiguous to a quarter of the baseband rate.
            let y2 = y * y;
            self.acq_sum += y2 * self.acq_prev_y2.conj();
            self.acq_prev_y2 = y2;
            self.acq_n += 1;
            if self.acq_n >= ACQ_LEN {
                self.car_freq =
                    (self.acq_sum.arg() * 0.5).clamp(-self.car_freq_max, self.car_freq_max);
                self.acquired = true;
            }
            return y;
        }

        // Costas phase loop tracks the residual after acquisition.
        let costas = y.re * y.im / y.norm_sqr().max(1e-9);
        self.car_freq =
            (self.car_freq + self.car_beta * costas).clamp(-self.car_freq_max, self.car_freq_max);
        self.car_phase = wrap_pi(self.car_phase + self.car_alpha * costas);
        y
    }

    /// One decimated complex baseband sample: carrier-correct it, matched-filter it over a chip,
    /// recover chip timing (Gardner), and biphase-decode chips into data bits.
    fn push_baseband(&mut self, sample: Iq, bits: &mut Vec<u8>) {
        let sample = self.carrier(sample);
        // Boxcar matched filter over one chip.
        self.mf[self.mf_pos] = sample;
        self.mf_pos = (self.mf_pos + 1) % self.mf.len();
        let mut acc = Iq::default();
        for (k, &t) in self.mf_taps.iter().enumerate() {
            let idx = (self.mf_pos + k) % self.mf.len();
            acc += self.mf[idx] * t;
        }

        // Gardner timing: step mu down each input sample; strobe twice per chip.
        self.hist[1] = self.hist[0];
        self.hist[0] = acc;
        self.mu -= 1.0;
        if self.mu > 0.0 {
            return;
        }
        let frac = self.mu + 1.0;
        let strobe = self.hist[1] * (1.0 - frac) + self.hist[0] * frac;
        self.mu += self.sps / 2.0;

        if self.half {
            // Mid-chip strobe, used only by the timing error detector.
            self.mid = strobe;
            self.half = false;
            return;
        }
        self.half = true;

        // Chip-center strobe. Update timing, then biphase-decode this chip.
        let err = gardner_error(self.last_center, self.mid, strobe);
        let norm = (self.last_center.norm_sqr() + strobe.norm_sqr()).max(1e-6);
        self.mu += TIMING_GAIN * err / norm;
        self.last_center = strobe;
        self.decode_chip(strobe, bits);
    }

    /// Biphase decode: a bit is the signed difference between consecutive chips. Two chips make a
    /// bit, so only one chip parity carries the bit transitions; pick it by magnitude over a
    /// window, then differential-decode (RDS differentially encodes before biphase).
    fn decode_chip(&mut self, chip: Iq, bits: &mut Vec<u8>) {
        let biphase = (chip.re - self.prev_chip.re) * 0.5;
        self.prev_chip = chip;

        if self.biphase_clock.is_multiple_of(2) {
            self.even_sum += biphase.abs();
        } else {
            self.odd_sum += biphase.abs();
        }
        self.biphase_count += 1;
        if self.biphase_count >= 128 {
            self.biphase_polarity = if self.even_sum >= self.odd_sum { 0 } else { 1 };
            self.even_sum = 0.0;
            self.odd_sum = 0.0;
            self.biphase_count = 0;
        }

        if self.biphase_clock % 2 == self.biphase_polarity {
            let biphase_bit = biphase >= 0.0;
            bits.push((biphase_bit != self.prev_decoded) as u8);
            self.prev_decoded = biphase_bit;
        }
        self.biphase_clock = self.biphase_clock.wrapping_add(1);
    }

    /// Whether the pilot PLL is tracking near 19 kHz (a precondition for any RDS).
    pub fn pilot_locked(&self) -> bool {
        (self.pll.freq_hz(self.sample_rate) - 19_000.0).abs() < 100.0
    }
}

/// Gardner timing error detector for complex symbols: uses the midpoint sample and the
/// difference of adjacent symbol-center samples. Zero at correct timing.
fn gardner_error(prev: Iq, mid: Iq, cur: Iq) -> f32 {
    ((cur - prev) * mid.conj()).re
}

/// Wrap a phase to (-pi, pi].
fn wrap_pi(mut p: f32) -> f32 {
    while p > PI {
        p -= TAU;
    }
    while p < -PI {
        p += TAU;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rds::sync::{make_block, A, B as OFF_B, C, D as OFF_D};
    use crate::{Decoder, Event, RdsDecoder, RdsEvent};
    use std::f32::consts::TAU;

    fn push_block(bits: &mut Vec<u8>, word: u32) {
        for i in (0..26).rev() {
            bits.push(((word >> i) & 1) as u8);
        }
    }

    /// Group 0A delivering two PS characters at `seg`.
    fn group_0a(bits: &mut Vec<u8>, pi: u16, seg: u16, hi: u8, lo: u8) {
        let b = (5u16 << 5) | seg; // type 0, version A, PTY 5
        push_block(bits, make_block(pi, A));
        push_block(bits, make_block(b, OFF_B));
        push_block(bits, make_block(0xCDCD, C));
        push_block(bits, make_block(((hi as u16) << 8) | lo as u16, OFF_D));
    }

    /// Modulate a data-bit stream into an FM multiplex: differential-encode, biphase-shape,
    /// put it on a 57 kHz subcarrier, and add a coherent 19 kHz pilot. The inverse of `Demod`.
    /// `sym_rate` simulates transmitter/receiver clock offset; `subcarrier_hz` simulates a
    /// subcarrier that is not perfectly locked to 3x the pilot (as seen on real stations).
    fn synth_mpx(
        data_bits: &[u8],
        fs: u32,
        repeats: usize,
        sym_rate: f32,
        subcarrier_hz: f32,
    ) -> Vec<f32> {
        synth_mpx_noisy(data_bits, fs, repeats, sym_rate, subcarrier_hz, 0.0)
    }

    fn synth_mpx_noisy(
        data_bits: &[u8],
        fs: u32,
        repeats: usize,
        sym_rate: f32,
        subcarrier_hz: f32,
        noise: f32,
    ) -> Vec<f32> {
        let mut levels = Vec::new();
        let mut t = 0u8;
        for _ in 0..repeats {
            for &d in data_bits {
                t ^= d & 1;
                levels.push(t);
            }
        }
        let sps = fs as f32 / sym_rate;
        let nsamp = (levels.len() as f32 * sps) as usize;
        let mut mpx = vec![0.0f32; nsamp];
        let mut rng = 0x1234_5678u32; // deterministic LCG for reproducible noise
        for (n, out) in mpx.iter_mut().enumerate() {
            let sym_pos = n as f32 / sps;
            let k = sym_pos.floor() as usize;
            if k >= levels.len() {
                break;
            }
            let first_half = (sym_pos - k as f32) < 0.5;
            // Biphase: level 1 is high-then-low, level 0 is low-then-high.
            let data_val = match (levels[k] == 1, first_half) {
                (true, true) | (false, false) => 1.0,
                _ => -1.0,
            };
            let time = n as f32 / fs as f32;
            let pilot = (TAU * 19_000.0 * time).cos();
            let subcarrier = (TAU * subcarrier_hz * time).cos();
            // Approximate Gaussian noise via summed LCG uniforms (central limit).
            let mut g = 0.0f32;
            for _ in 0..6 {
                rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                g += (rng >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            }
            *out = pilot + 0.4 * data_val * subcarrier + noise * g;
        }
        mpx
    }

    /// Decode a multiplex and return the recovered PI, PS, and whether sync held to the end.
    fn decode(mpx: &[f32], fs: u32) -> (Option<u16>, Option<String>, bool) {
        let mut dec = RdsDecoder::new(fs);
        let (mut pi, mut ps) = (None, None);
        for chunk in mpx.chunks(8192) {
            for e in dec.feed(chunk) {
                match e {
                    Event::Rds(RdsEvent::Pi(v)) => pi = Some(v),
                    Event::Rds(RdsEvent::ProgramService(s)) => ps = Some(s),
                    _ => {}
                }
            }
        }
        (pi, ps, dec.synced())
    }

    fn ps_groups(pi: u16) -> Vec<u8> {
        let ps = b"KEXP-FM ";
        let mut data = Vec::new();
        for seg in 0..4u16 {
            let i = seg as usize * 2;
            group_0a(&mut data, pi, seg, ps[i], ps[i + 1]);
        }
        data
    }

    #[test]
    fn decodes_synthetic_multiplex() {
        let pi = 0x4D54;
        let fs = 240_000;
        let mpx = synth_mpx(&ps_groups(pi), fs, 40, SYMBOL_RATE, 57_000.0);
        let (got_pi, got_ps, synced) = decode(&mpx, fs);
        assert_eq!(got_pi, Some(pi), "PI not recovered");
        assert_eq!(got_ps.as_deref(), Some("KEXP-FM"), "PS not recovered");
        assert!(synced, "sync should hold");
    }

    #[test]
    fn timing_loop_tracks_clock_offset() {
        // Simulate transmitter/receiver clock mismatch in both directions (RTL-SDR crystals run
        // tens to hundreds of ppm off). Free-running timing would slip and lose sync; the
        // Gardner loop must pull it back.
        let pi = 0x1234;
        let fs = 240_000;
        for ppm in [-400.0, -150.0, 150.0, 400.0] {
            let rate = SYMBOL_RATE * (1.0 + ppm * 1e-6);
            let mpx = synth_mpx(&ps_groups(pi), fs, 80, rate, 57_000.0);
            let (got_pi, got_ps, synced) = decode(&mpx, fs);
            assert_eq!(got_pi, Some(pi), "PI lost at {ppm} ppm");
            assert_eq!(got_ps.as_deref(), Some("KEXP-FM"), "PS lost at {ppm} ppm");
            // Sync surviving to the end proves the loop tracked the offset rather than just
            // catching the field before drift accumulated.
            assert!(synced, "sync lost by end at {ppm} ppm");
        }
    }

    /// FM-modulate a multiplex into IQ (the inverse of the FM demodulator), to test the full
    /// IQ -> FM demod -> RDS path the real signal takes.
    fn fm_modulate(mpx: &[f32], k: f32) -> Vec<Iq> {
        let mut phase = 0.0f32;
        mpx.iter()
            .map(|&m| {
                phase = (phase + k * m).rem_euclid(TAU);
                Iq::new(phase.cos(), phase.sin())
            })
            .collect()
    }

    #[test]
    fn decodes_through_fm_demod() {
        // The real path is IQ -> FM demod -> RDS, never tested before (synthetic fed MPX directly).
        let pi = 0x4D54;
        let fs = 240_000;
        let mpx = synth_mpx(&ps_groups(pi), fs, 50, SYMBOL_RATE, 57_000.0);
        let iq = fm_modulate(&mpx, 0.3);
        let mut fm = sdr_dsp::FmDemod::new(1.0);
        let mut demod = Vec::new();
        fm.process(&iq, &mut demod);
        let (got_pi, got_ps, _) = decode(&demod, fs);
        assert_eq!(got_pi, Some(pi), "PI lost through FM demod");
        assert_eq!(
            got_ps.as_deref(),
            Some("KEXP-FM"),
            "PS lost through FM demod"
        );
    }

    #[test]
    fn decodes_through_noisy_fm_demod() {
        // IQ noise becomes parabolic MPX noise, worst at 57 kHz where RDS lives. The decoder
        // should still lock at a moderate level (representative of a solid real station).
        let pi = 0x21B0;
        let fs = 240_000;
        let clean = synth_mpx(&ps_groups(pi), fs, 60, SYMBOL_RATE, 57_000.0);
        let iq = fm_modulate(&clean, 0.3);
        let mut rng = 0x9E37_79B9u32;
        let noisy: Vec<Iq> = iq
            .iter()
            .map(|&s| {
                let mut g = [0.0f32; 2];
                for x in &mut g {
                    for _ in 0..4 {
                        rng = rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        *x += (rng >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                    }
                }
                s + Iq::new(0.1 * g[0], 0.1 * g[1])
            })
            .collect();
        let mut fm = sdr_dsp::FmDemod::new(1.0);
        let mut mpx = Vec::new();
        fm.process(&noisy, &mut mpx);
        let (got_pi, got_ps, _) = decode(&mpx, fs);
        assert_eq!(got_pi, Some(pi), "PI lost through noisy FM demod");
        assert_eq!(
            got_ps.as_deref(),
            Some("KEXP-FM"),
            "PS lost through noisy FM demod"
        );
    }

    #[test]
    fn carrier_loop_tracks_subcarrier_offset() {
        // Real stations do not always lock the 57 kHz subcarrier to exactly 3x the pilot; a few
        // hundred Hz offset is common. The pilot-derived reference is then wrong by that much and
        // the FLL/Costas loop must track it out, or every symbol rotates and decoding fails.
        let pi = 0x21B0;
        let fs = 240_000;
        for offset in [-500.0, -200.0, 200.0, 500.0] {
            let mpx = synth_mpx(&ps_groups(pi), fs, 80, SYMBOL_RATE, 57_000.0 + offset);
            let (got_pi, got_ps, _) = decode(&mpx, fs);
            assert_eq!(got_pi, Some(pi), "PI lost at {offset} Hz subcarrier offset");
            assert_eq!(
                got_ps.as_deref(),
                Some("KEXP-FM"),
                "PS lost at {offset} Hz subcarrier offset"
            );
        }
    }

    /// Manual throughput probe (ignored), not a correctness gate. Mirrors `engine::Channel`
    /// (NCO -> FIR decimate -> DC block -> FM demod -> RDS decode) at the scanner's capture rate,
    /// so the printed "Nx realtime" is roughly how many stations one core can decode live.
    /// Run: `cargo test -p sdr-decode --release -- --ignored --nocapture channel_throughput`
    #[test]
    #[ignore]
    fn channel_throughput() {
        use sdr_core::Iq;
        use sdr_dsp::{lowpass, FirDecimator, FmDemod, Nco};

        let fs = 1_024_000u32; // RTL-SDR window rate the scanner uses
        let offset = 250_000.0f64; // station offset within the window

        // RDS-bearing multiplex at the capture rate, FM-modulated onto a carrier at `offset`.
        let mpx = synth_mpx(&ps_groups(0x4D54), fs, 12, SYMBOL_RATE, 57_000.0);
        let mut iq = vec![Iq::default(); mpx.len()];
        let (mut phase, kf) = (0.0f32, 0.25f32);
        let w = TAU * offset as f32 / fs as f32;
        for (n, (s, m)) in iq.iter_mut().zip(&mpx).enumerate() {
            phase += kf * *m;
            let a = w * n as f32 + phase;
            *s = Iq::new(a.cos(), a.sin());
        }

        // Channel front-end, identical to engine::Channel::new.
        let decim = (fs as f32 / 240_000.0).round().max(1.0) as usize;
        let mut nco = Nco::new(-offset, fs);
        let mut lpf = FirDecimator::new(lowpass(127, 0.45 / decim as f32), decim);
        let mut fm = FmDemod::new(1.0);
        let mut dec = RdsDecoder::new(fs / decim as u32);
        let mut dc = Iq::default();
        let (mut shifted, mut baseband, mut demod) = (Vec::new(), Vec::new(), Vec::new());

        let start = std::time::Instant::now();
        for block in iq.chunks(16_384) {
            shifted.resize(block.len(), Iq::default());
            nco.mix(block, &mut shifted);
            baseband.clear();
            lpf.process(&shifted, &mut baseband);
            for b in &mut baseband {
                dc += (*b - dc) * 1e-4;
                *b -= dc;
            }
            demod.clear();
            fm.process(&baseband, &mut demod);
            dec.feed(&demod);
        }
        let elapsed = start.elapsed().as_secs_f32();
        let sig = iq.len() as f32 / fs as f32;
        println!(
            "CHANNEL_THROUGHPUT: {sig:.2}s signal in {elapsed:.3}s => {:.1}x realtime (~{:.0} stations/core)",
            sig / elapsed,
            sig / elapsed
        );
    }
}
