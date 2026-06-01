//! The RTL-SDR [`Source`]: a thin wrapper over `rtl-sdr-rs` that delivers `Complex<f32>` IQ.
//!
//! libusb is statically linked in (see the `vendored` feature in `Cargo.toml`), so this
//! works with no system library install.

use rtl_sdr_rs::{DeviceId, RtlSdr, TunerGain};
use sdr_core::{Error, Iq, Result, Source};

use crate::convert;

/// USB bulk transfer granularity. Reads sized to a multiple of this stream most efficiently.
pub const USB_CHUNK: usize = 16384;

/// A sensible default read size: two USB chunks worth of complex samples.
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
    sdr: RtlSdr,
    info: DeviceInfo,
    sample_rate: u32,
    center_freq: u64,
    byte_buf: Vec<u8>,
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

        Ok(Self {
            sdr,
            info,
            sample_rate: cfg.sample_rate,
            center_freq: cfg.freq_hz,
            byte_buf: Vec::new(),
        })
    }

    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// The gain steps the tuner supports, in tenths of a dB.
    pub fn supported_gains(&self) -> Result<Vec<i32>> {
        self.sdr.get_tuner_gains().map_err(device_err)
    }
}

impl Source for RtlSdrSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn center_freq(&self) -> u64 {
        self.center_freq
    }

    fn tune(&mut self, hz: u64) -> Result<()> {
        let freq = u32::try_from(hz)
            .map_err(|_| Error::Config(format!("frequency {hz} Hz out of range")))?;
        self.sdr.set_center_freq(freq).map_err(device_err)?;
        self.center_freq = hz;
        Ok(())
    }

    fn read(&mut self, out: &mut [Iq]) -> Result<usize> {
        let want = out.len() * 2;
        self.byte_buf.resize(want, 0);
        let got = self.sdr.read_sync(&mut self.byte_buf).map_err(device_err)?;
        Ok(convert::cu8_to_iq(&self.byte_buf[..got], out))
    }
}
