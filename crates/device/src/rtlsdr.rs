//! The RTL-SDR [`Source`]: a thin wrapper over `rtl-sdr-rs` that delivers `Complex<f32>` IQ.
//!
//! libusb is statically linked in (see the `vendored` feature in `Cargo.toml`), so this
//! works with no system library install.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use rtl_sdr_rs::{DeviceId, RtlSdr, TunerGain};
use sdr_core::{Error, Iq, Result, Source};

use crate::convert;

/// Bytes per `read_sync` on the reader thread. The thread does nothing but read and enqueue, so
/// the gap between USB transfers stays tiny (the device FIFO never overflows the way it does
/// when demodulation work sits between synchronous reads).
const READ_BYTES: usize = 1 << 16;

/// RTL-SDR USB bulk transfer granularity, in bytes. Reads that are a multiple of it stream
/// most efficiently.
const USB_CHUNK: usize = 16384;

/// Default read size, in complex samples. At 2 bytes/sample this spans two USB transfers.
pub const DEFAULT_READ_SAMPLES: usize = USB_CHUNK;

/// Tuner gain setting.
#[derive(Debug, Clone, Copy)]
pub enum Gain {
    /// Hardware automatic gain control.
    Auto,
    /// Manual gain in tenths of a dB (one of the device's supported steps).
    Manual(i32),
}

impl From<Gain> for TunerGain {
    fn from(gain: Gain) -> Self {
        match gain {
            Gain::Auto => TunerGain::Auto,
            Gain::Manual(tenths_db) => TunerGain::Manual(tenths_db),
        }
    }
}

/// How to configure the device on open.
#[derive(Debug, Clone, Copy)]
pub struct RtlConfig {
    pub freq_hz: u64,
    pub sample_rate: u32,
    pub gain: Gain,
}

impl Default for RtlConfig {
    fn default() -> Self {
        Self {
            freq_hz: 100_000_000,
            sample_rate: 2_048_000,
            gain: Gain::Auto,
        }
    }
}

/// Identifying information about a connected device.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub index: usize,
    pub name: String,
    pub manufacturer: String,
    pub serial: String,
    /// The tuner chip (e.g. "R820T"). `None` until the device is opened.
    pub tuner: Option<String>,
}

pub struct RtlSdrSource {
    info: DeviceInfo,
    sample_rate: u32,
    /// Shared with the reader thread: the live center frequency and pending retune request.
    center_freq: Arc<AtomicU64>,
    tune_req: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    /// Raw cu8 chunks from the reader thread, plus a cursor into the current chunk.
    rx: Receiver<Vec<u8>>,
    chunk: Vec<u8>,
    pos: usize,
    /// Supported tuner gain steps, captured at open (the device now lives on the reader thread).
    gains: Vec<i32>,
    reader: Option<JoinHandle<()>>,
}

fn device_err(e: impl std::fmt::Display) -> Error {
    Error::Device(e.to_string())
}

impl RtlSdrSource {
    /// Enumerate connected RTL-SDR devices. `tuner` is left `None` (it requires opening).
    pub fn list() -> Result<Vec<DeviceInfo>> {
        let devices = RtlSdr::list_devices().map_err(device_err)?;
        Ok(devices
            .into_iter()
            .map(|d| DeviceInfo {
                index: d.index,
                name: d.product,
                manufacturer: d.manufacturer,
                serial: d.serial,
                tuner: None,
            })
            .collect())
    }

    /// Open the device at `index` (its position among detected RTL-SDRs) and apply `cfg`.
    pub fn open(index: usize, cfg: RtlConfig) -> Result<Self> {
        // Read the descriptor strings before opening for real — both paths open the USB
        // device exclusively, so they must not overlap.
        let desc = RtlSdr::get_device_info(index).map_err(device_err)?;

        let mut sdr = RtlSdr::open(DeviceId::Index(index)).map_err(device_err)?;
        sdr.set_sample_rate(cfg.sample_rate).map_err(device_err)?;
        // The tuner quantizes the rate; the actual delivered rate (not the request) is what the
        // sample timing is based on, so downstream decoders must use it.
        let actual_rate = sdr.get_sample_rate();
        let freq = u32::try_from(cfg.freq_hz)
            .map_err(|_| Error::Config(format!("frequency {} Hz out of range", cfg.freq_hz)))?;
        sdr.set_center_freq(freq).map_err(device_err)?;
        sdr.set_tuner_gain(cfg.gain.into()).map_err(device_err)?;
        sdr.reset_buffer().map_err(device_err)?; // mandatory before the first read

        let tuner = sdr.get_tuner_id().ok().map(str::to_string);
        let info = DeviceInfo {
            index: desc.index,
            name: desc.product,
            manufacturer: desc.manufacturer,
            serial: desc.serial,
            tuner,
        };

        let gains = sdr.get_tuner_gains().unwrap_or_default();
        let center_freq = Arc::new(AtomicU64::new(cfg.freq_hz));
        let tune_req = Arc::new(AtomicU64::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<Vec<u8>>();

        // Reader thread: tight read_sync loop into the channel, applying retunes between reads.
        // Keeping the device drained continuously is what avoids dropped samples.
        let reader = {
            let (center_freq, tune_req, stop) = (
                Arc::clone(&center_freq),
                Arc::clone(&tune_req),
                Arc::clone(&stop),
            );
            thread::Builder::new()
                .name("rtl-read".into())
                .spawn(move || {
                    let mut sdr = sdr;
                    let mut buf = vec![0u8; READ_BYTES];
                    while !stop.load(Ordering::Relaxed) {
                        let req = tune_req.swap(0, Ordering::AcqRel);
                        if req != 0 {
                            if let Ok(f) = u32::try_from(req) {
                                if sdr.set_center_freq(f).is_ok() {
                                    center_freq.store(req, Ordering::Release);
                                }
                            }
                        }
                        match sdr.read_sync(&mut buf) {
                            Ok(0) => continue,
                            Ok(n) => {
                                if tx.send(buf[..n].to_vec()).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                })
                .expect("spawn rtl-read thread")
        };

        Ok(Self {
            info,
            sample_rate: actual_rate,
            center_freq,
            tune_req,
            stop,
            rx,
            chunk: Vec::new(),
            pos: 0,
            gains,
            reader: Some(reader),
        })
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// The gain steps the tuner supports, in tenths of a dB.
    pub fn supported_gains(&self) -> Result<Vec<i32>> {
        Ok(self.gains.clone())
    }
}

impl Drop for RtlSdrSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Source for RtlSdrSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn center_freq(&self) -> u64 {
        self.center_freq.load(Ordering::Acquire)
    }

    fn tune(&mut self, hz: u64) -> Result<()> {
        u32::try_from(hz).map_err(|_| Error::Config(format!("frequency {hz} Hz out of range")))?;
        // Hand the retune to the reader thread, which applies it between reads.
        self.tune_req.store(hz, Ordering::Release);
        Ok(())
    }

    fn tune_range(&self) -> (u64, u64) {
        // The R820T/R828D tuners on the RTL-SDR V3 cover ~24 MHz to 1.766 GHz. Direct-sampling
        // (which reaches HF below 24 MHz) is not enabled, so this is the usable range.
        (24_000_000, 1_766_000_000)
    }

    fn read(&mut self, out: &mut [Iq]) -> Result<usize> {
        // Refill from the reader thread when the current chunk is drained. Blocking here is fine:
        // the device keeps streaming into the channel meanwhile, so no samples are lost.
        if self.pos >= self.chunk.len() {
            match self.rx.recv() {
                Ok(chunk) => {
                    self.chunk = chunk;
                    self.pos = 0;
                }
                Err(_) => return Ok(0), // reader thread stopped: end of stream
            }
        }
        let n = convert::cu8_to_iq(&self.chunk[self.pos..], out);
        self.pos += n * 2;
        Ok(n)
    }
}
