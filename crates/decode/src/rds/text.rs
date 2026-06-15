//! Accumulators for the two text fields RDS delivers piecemeal: the 8-character Program
//! Service name and the up-to-64-character RadioText. Each group carries only a couple of
//! characters at a segment address, so a field is assembled across many groups.

/// Map an RDS character byte to display text. The RDS G0 table matches ASCII over the
/// printable range, which covers ordinary station names and RadioText; anything else (control
/// codes, extended glyphs) becomes a space.
fn rds_char(byte: u8) -> char {
    match byte {
        0x20..=0x7E => byte as char,
        _ => ' ',
    }
}

/// The 8-character Program Service name, filled by segment.
pub struct PsBuffer {
    chars: [u8; 8],
    seen: u8,
    last: Option<String>,
}

impl PsBuffer {
    pub fn new() -> Self {
        Self {
            chars: [b' '; 8],
            seen: 0,
            last: None,
        }
    }

    fn set(&mut self, idx: usize, byte: u8) {
        if idx < 8 {
            self.chars[idx] = byte;
            self.seen |= 1 << idx;
        }
    }

    /// Apply the two characters from a 0A/0B group's block D at segment `seg` (0..3). Returns
    /// the finished name once all eight positions have been seen and the text has changed.
    pub fn apply(&mut self, seg: usize, hi: u8, lo: u8) -> Option<String> {
        self.set(seg * 2, hi);
        self.set(seg * 2 + 1, lo);
        if self.seen != 0xFF {
            return None;
        }
        let text: String = self.chars.iter().map(|&b| rds_char(b)).collect();
        let text = text.trim_end().to_string();
        if self.last.as_deref() == Some(text.as_str()) {
            None
        } else {
            self.last = Some(text.clone());
            Some(text)
        }
    }
}

/// RadioText, up to 64 characters, filled by segment and reset when the text A/B flag toggles.
pub struct RtBuffer {
    chars: [u8; 64],
    seen: u64,
    ab: Option<u8>,
    last: Option<String>,
}

impl RtBuffer {
    pub fn new() -> Self {
        Self {
            chars: [b' '; 64],
            seen: 0,
            ab: None,
            last: None,
        }
    }

    /// A new A/B flag value means a new message: clear the buffer so stale characters do not
    /// bleed into the next text.
    pub fn set_flag(&mut self, ab: u8) {
        if self.ab != Some(ab) {
            self.ab = Some(ab);
            self.chars = [b' '; 64];
            self.seen = 0;
        }
    }

    fn set(&mut self, idx: usize, byte: u8) {
        if idx < 64 {
            self.chars[idx] = byte;
            self.seen |= 1 << idx;
        }
    }

    /// Extract the `start..start+len` slice an RT+ tag points at, but only if every referenced
    /// position has actually been received (so a tag can't slice into not-yet-filled RadioText).
    /// Trailing spaces are trimmed; a blank slice yields `None`.
    pub fn substring(&self, start: usize, len: usize) -> Option<String> {
        let end = start.checked_add(len)?;
        if end > 64 {
            return None;
        }
        let upto = if end == 64 {
            u64::MAX
        } else {
            (1u64 << end) - 1
        };
        let want = upto & !((1u64 << start) - 1);
        if self.seen & want != want {
            return None;
        }
        let text: String = self.chars[start..end]
            .iter()
            .map(|&b| rds_char(b))
            .collect();
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    }

    /// Place characters starting at `start`. Returns the current text when it changes. A 0x0D
    /// carriage return marks the end of the message and truncates it.
    pub fn apply(&mut self, start: usize, bytes: &[u8]) -> Option<String> {
        let mut terminator = None;
        for (i, &byte) in bytes.iter().enumerate() {
            let idx = start + i;
            if byte == 0x0D {
                terminator = Some(idx);
            }
            self.set(idx, byte);
        }

        let end = terminator.unwrap_or(64).min(64);
        // Only report text we have actually received contiguously from the start.
        let filled = (self.seen.trailing_ones() as usize).min(64);
        let len = end.min(filled);
        if len == 0 {
            return None;
        }
        let text: String = self.chars[..len].iter().map(|&b| rds_char(b)).collect();
        let text = text.trim_end().to_string();
        if text.is_empty() || self.last.as_deref() == Some(text.as_str()) {
            None
        } else {
            self.last = Some(text.clone());
            Some(text)
        }
    }
}
