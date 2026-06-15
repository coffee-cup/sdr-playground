//! Group assembly and parsing. The block sync emits 26-bit blocks in their A,B,C,D positions;
//! this collects each set of four into a group and extracts the fields we care about: PI, PTY,
//! the Program Service name (group 0), and RadioText (group 2).

use crate::rds::sync::{Block, C_PRIME, D};
use crate::rds::text::{PsBuffer, RtBuffer};
use crate::rds::RdsEvent;
use crate::Event;

/// Assembles blocks into groups and tracks the running PS/RT/PI/PTY state, emitting an event
/// whenever a field changes.
pub struct Groups {
    /// Info words for the current group by position; `None` if that block failed its check.
    blocks: [Option<u16>; 4],
    have: u8,
    ps: PsBuffer,
    rt: RtBuffer,
    last_pi: Option<u16>,
    last_pty: Option<u8>,
}

impl Groups {
    pub fn new() -> Self {
        Self {
            blocks: [None; 4],
            have: 0,
            ps: PsBuffer::new(),
            rt: RtBuffer::new(),
            last_pi: None,
            last_pty: None,
        }
    }

    /// Position 0..3 of a block from its offset (C and C' both sit at position 2).
    fn position(offset: usize) -> usize {
        match offset {
            o if o == D => 3,
            o if o == C_PRIME => 2,
            o => o.min(2),
        }
    }

    /// Feed one synced block. A completed group (delimited by its D block) is parsed and any
    /// resulting events pushed to `out`.
    pub fn push_block(&mut self, block: Block, out: &mut Vec<Event>) {
        let pos = Self::position(block.offset);
        self.blocks[pos] = block.ok.then_some(block.info);
        self.have |= 1 << pos;

        if pos == 3 {
            self.parse(out);
            self.blocks = [None; 4];
            self.have = 0;
        }
    }

    fn parse(&mut self, out: &mut Vec<Event>) {
        // Block B carries the group type and flags; without it the group is unusable.
        let Some(b) = self.blocks[1] else { return };
        let group_type = (b >> 12) & 0xF;
        let version_b = (b >> 11) & 1 == 1;
        let pty = ((b >> 5) & 0x1F) as u8;

        if let Some(pi) = self.blocks[0] {
            if self.last_pi != Some(pi) {
                self.last_pi = Some(pi);
                out.push(Event::Rds(RdsEvent::Pi(pi)));
            }
        }
        if self.last_pty != Some(pty) {
            self.last_pty = Some(pty);
            out.push(Event::Rds(RdsEvent::ProgramType(pty)));
        }

        match group_type {
            0 => self.parse_ps(b, out),
            2 => self.parse_rt(b, version_b, out),
            _ => {}
        }
    }

    /// Group 0A/0B: block D holds two PS characters at the segment in block B's low 2 bits.
    fn parse_ps(&mut self, b: u16, out: &mut Vec<Event>) {
        let seg = (b & 0x3) as usize;
        let Some(d) = self.blocks[3] else { return };
        if let Some(ps) = self.ps.apply(seg, (d >> 8) as u8, (d & 0xFF) as u8) {
            out.push(Event::Rds(RdsEvent::ProgramService(ps)));
        }
    }

    /// Group 2A/2B: RadioText. 2A carries four characters (blocks C and D) per segment; 2B
    /// carries two (block D), with block C reused for PI. Block B bit 4 is the A/B flag.
    fn parse_rt(&mut self, b: u16, version_b: bool, out: &mut Vec<Event>) {
        self.rt.set_flag(((b >> 4) & 1) as u8);
        let seg = (b & 0xF) as usize;

        let text = if version_b {
            let Some(d) = self.blocks[3] else { return };
            self.rt.apply(seg * 2, &[(d >> 8) as u8, (d & 0xFF) as u8])
        } else {
            let mut chars = [b' '; 4];
            let mut got = false;
            if let Some(c) = self.blocks[2] {
                chars[0] = (c >> 8) as u8;
                chars[1] = (c & 0xFF) as u8;
                got = true;
            }
            if let Some(d) = self.blocks[3] {
                chars[2] = (d >> 8) as u8;
                chars[3] = (d & 0xFF) as u8;
                got = true;
            }
            if got {
                self.rt.apply(seg * 4, &chars)
            } else {
                None
            }
        };

        if let Some(text) = text {
            out.push(Event::Rds(RdsEvent::RadioText(text)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rds::sync::{make_block, BlockSync, A, B as OFF_B, C, D as OFF_D};

    /// Append a 26-bit block, MSB first.
    fn push_block(bits: &mut Vec<u8>, word: u32) {
        for i in (0..26).rev() {
            bits.push(((word >> i) & 1) as u8);
        }
    }

    /// A group 0A (basic tuning + PS): block D carries two PS chars at `seg`.
    fn group_0a(bits: &mut Vec<u8>, pi: u16, pty: u16, seg: u16, hi: u8, lo: u8) {
        let b = (pty << 5) | seg; // group type 0, version A, flags 0
        push_block(bits, make_block(pi, A));
        push_block(bits, make_block(b, OFF_B));
        push_block(bits, make_block(0xE0E0, C)); // AF, unused here
        push_block(bits, make_block(((hi as u16) << 8) | lo as u16, OFF_D));
    }

    /// A group 2A (RadioText): blocks C and D carry four chars at `seg`.
    fn group_2a(bits: &mut Vec<u8>, pi: u16, pty: u16, ab: u16, seg: u16, c: &[u8; 4]) {
        let b = (2 << 12) | (pty << 5) | (ab << 4) | seg; // type 2, version A
        push_block(bits, make_block(pi, A));
        push_block(bits, make_block(b, OFF_B));
        push_block(bits, make_block(((c[0] as u16) << 8) | c[1] as u16, C));
        push_block(bits, make_block(((c[2] as u16) << 8) | c[3] as u16, OFF_D));
    }

    #[test]
    fn recovers_pi_ps_and_radiotext_from_a_group_stream() {
        let pi = 0x4D54;
        let pty = 5; // Rock
        let ps = b"KEXP-FM ";
        let rt = b"Now Playing: Radiohead - Creep  "; // 32 chars, multiple of 4

        let mut bits = Vec::new();
        // Repeat the full cycle enough times to acquire sync and fill both fields.
        for _ in 0..6 {
            for seg in 0..4u16 {
                let i = seg as usize * 2;
                group_0a(&mut bits, pi, pty, seg, ps[i], ps[i + 1]);
            }
            for seg in 0..8u16 {
                let i = seg as usize * 4;
                let chunk = [rt[i], rt[i + 1], rt[i + 2], rt[i + 3]];
                group_2a(&mut bits, pi, pty, 0, seg, &chunk);
            }
        }

        let mut sync = BlockSync::new();
        let mut groups = Groups::new();
        let mut events = Vec::new();
        for &bit in &bits {
            if let Some(block) = sync.push(bit) {
                groups.push_block(block, &mut events);
            }
        }

        let mut got_pi = None;
        let mut got_ps = None;
        let mut got_rt = None;
        let mut got_pty = None;
        for e in &events {
            match e {
                Event::Rds(RdsEvent::Pi(v)) => got_pi = Some(*v),
                Event::Rds(RdsEvent::ProgramService(s)) => got_ps = Some(s.clone()),
                Event::Rds(RdsEvent::RadioText(s)) => got_rt = Some(s.clone()),
                Event::Rds(RdsEvent::ProgramType(p)) => got_pty = Some(*p),
            }
        }

        assert_eq!(got_pi, Some(pi));
        assert_eq!(got_pty, Some(5));
        assert_eq!(got_ps.as_deref(), Some("KEXP-FM"));
        assert_eq!(got_rt.as_deref(), Some("Now Playing: Radiohead - Creep"));
    }
}
