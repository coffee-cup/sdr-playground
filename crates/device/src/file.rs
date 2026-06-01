//! A [`Source`] that replays raw interleaved `cu8` IQ from a file.
//!
//! This is the device-free path: it shares the exact conversion of the hardware source, so
//! the pipeline above it is identical whether samples come from the antenna or from disk.
//! Pacing (replaying at the original sample rate) is the engine's job, not the source's.
//!
//! The file is headerless raw `cu8` for now; a container/metadata format (SigMF) is a
//! separate decision — see `docs/ARCHITECTURE.md`.

use std::fs::File;
use std::io::{BufReader, ErrorKind, Read};
use std::path::Path;

use sdr_core::{Iq, Result, Source};

use crate::convert;

pub struct FileSource {
    reader: BufReader<File>,
    sample_rate: u32,
    center_freq: u64,
    byte_buf: Vec<u8>,
}

impl FileSource {
    /// Open a raw `cu8` file, reporting the given sample rate and center frequency (which the
    /// file itself does not record).
    pub fn open_cu8(path: impl AsRef<Path>, sample_rate: u32, center_freq: u64) -> Result<Self> {
        let file = File::open(path)?;
        Ok(Self {
            reader: BufReader::new(file),
            sample_rate,
            center_freq,
            byte_buf: Vec::new(),
        })
    }
}

impl Source for FileSource {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn center_freq(&self) -> u64 {
        self.center_freq
    }

    fn tune(&mut self, hz: u64) -> Result<()> {
        self.center_freq = hz;
        Ok(())
    }

    fn read(&mut self, out: &mut [Iq]) -> Result<usize> {
        let want = out.len() * 2;
        self.byte_buf.resize(want, 0);

        let mut filled = 0;
        while filled < want {
            match self.reader.read(&mut self.byte_buf[filled..]) {
                Ok(0) => break, // EOF
                Ok(n) => filled += n,
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(convert::cu8_to_iq(&self.byte_buf[..filled], out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tone.cu8")
    }

    fn fixture_byte_len() -> usize {
        std::fs::metadata(fixture()).unwrap().len() as usize
    }

    #[test]
    fn replays_all_samples_then_eof() {
        let mut src = FileSource::open_cu8(fixture(), 2_048_000, 100_000_000).unwrap();
        assert_eq!(src.sample_rate(), 2_048_000);
        assert_eq!(src.center_freq(), 100_000_000);

        let mut out = vec![Iq::default(); 4096];
        let mut total = 0;
        loop {
            let n = src.read(&mut out).unwrap();
            if n == 0 {
                break;
            }
            total += n;
        }
        assert_eq!(total, fixture_byte_len() / 2);
        // Reading again past EOF stays at 0.
        assert_eq!(src.read(&mut out).unwrap(), 0);
    }

    #[test]
    fn tune_updates_reported_center_freq() {
        let mut src = FileSource::open_cu8(fixture(), 2_048_000, 100_000_000).unwrap();
        src.tune(99_500_000).unwrap();
        assert_eq!(src.center_freq(), 99_500_000);
    }
}
