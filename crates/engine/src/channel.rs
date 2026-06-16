//! A demodulation channel: the realized "tune -> filter + decimate -> demodulate -> decode"
//! tail from `docs/ARCHITECTURE.md`. One channel shifts a station's offset within the captured
//! window to baseband, FM-demodulates it, and feeds the multiplex to a decoder.

use sdr_core::Iq;
use sdr_decode::{Decoder, Event};
use sdr_dsp::{lowpass, FirDecimator, FmDemod, Nco};

/// Target multiplex rate after channel decimation. Comfortably above the 57 kHz RDS subcarrier.
const MPX_RATE: f32 = 240_000.0;

/// Specifies a channel to run: where the station sits relative to the window center, and how to
/// build its decoder once the multiplex rate is known.
pub struct ChannelSpec {
    /// Station offset from the tuned center frequency, in Hz (may be negative).
    pub offset_hz: f64,
    /// Builds the decoder for this channel given the multiplex sample rate it will receive.
    pub make_decoder: Box<dyn FnOnce(u32) -> Box<dyn Decoder> + Send>,
}

impl ChannelSpec {
    /// A convenience constructor for `RdsDecoder` channels.
    pub fn rds(offset_hz: f64) -> Self {
        Self {
            offset_hz,
            make_decoder: Box::new(|rate| Box::new(sdr_decode::RdsDecoder::new(rate))),
        }
    }
}

/// A live channel: NCO mixer + low-pass decimator + FM demodulator + decoder.
pub struct Channel {
    offset_hz: f64,
    nco: Nco,
    lpf: FirDecimator,
    fm: FmDemod,
    decoder: Box<dyn Decoder>,
    shifted: Vec<Iq>,
    baseband: Vec<Iq>,
    mpx: Vec<f32>,
    /// Running DC estimate, subtracted before demodulation: the RTL-SDR's DC spike lands wherever
    /// a station sits at the window center, and corrupts the phase discriminator if left in.
    dc: Iq,
}

impl Channel {
    pub fn new(spec: ChannelSpec, input_rate: u32) -> Self {
        let decim = (input_rate as f32 / MPX_RATE).round().max(1.0) as usize;
        let mpx_rate = input_rate / decim as u32;
        // Low-pass just below the decimated Nyquist so the full wideband-FM signal (and its
        // 57 kHz RDS sidebands) survives before the discriminator.
        let lpf = FirDecimator::new(lowpass(127, 0.45 / decim as f32), decim);
        Self {
            offset_hz: spec.offset_hz,
            nco: Nco::new(-spec.offset_hz, input_rate),
            lpf,
            fm: FmDemod::new(1.0),
            decoder: (spec.make_decoder)(mpx_rate),
            shifted: Vec::new(),
            baseband: Vec::new(),
            mpx: Vec::new(),
            dc: Iq::default(),
        }
    }

    pub fn offset_hz(&self) -> f64 {
        self.offset_hz
    }

    /// Feed a block of window IQ; return any decoded events.
    pub fn feed(&mut self, iq: &[Iq]) -> Vec<Event> {
        self.shifted.resize(iq.len(), Iq::default());
        self.nco.mix(iq, &mut self.shifted);
        self.baseband.clear();
        self.lpf.process(&self.shifted, &mut self.baseband);
        // One-pole DC blocker (~a few Hz corner, far below the 57 kHz RDS) before demodulation.
        for b in &mut self.baseband {
            self.dc += (*b - self.dc) * 1e-4;
            *b -= self.dc;
        }
        self.mpx.clear();
        self.fm.process(&self.baseband, &mut self.mpx);
        self.decoder.feed(&self.mpx)
    }
}
