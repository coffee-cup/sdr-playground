//! A thin, reusable wrapper over rustfft's planned forward FFT for a fixed transform size.
//! rustfft operates on `num_complex::Complex<f32>`, which is exactly `core::Iq`, so IQ flows
//! through without conversion.

use std::sync::Arc;

use rustfft::{Fft, FftPlanner};
use sdr_core::Iq;

/// A forward FFT planned once for a fixed size, reused across blocks. Owns its scratch buffer
/// so `process` allocates nothing.
pub struct Forward {
    fft: Arc<dyn Fft<f32>>,
    scratch: Vec<Iq>,
}

impl Forward {
    pub fn new(size: usize) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(size);
        let scratch = vec![Iq::default(); fft.get_inplace_scratch_len()];
        Self { fft, scratch }
    }

    /// In-place forward FFT. `buf` must match the planned size.
    pub fn process(&mut self, buf: &mut [Iq]) {
        self.fft.process_with_scratch(buf, &mut self.scratch);
    }
}
