//! Group assembly and parsing. The block sync emits 26-bit blocks in their A,B,C,D positions;
//! this collects each set of four into a group and extracts the fields we care about: PI, PTY,
//! the Program Service name (group 0), and RadioText (group 2).

use crate::rds::sync::{Block, C_PRIME, D};
use crate::rds::text::{PsBuffer, RtBuffer, SegmentText};
use crate::rds::RdsEvent;
use crate::Event;

/// Assembles blocks into groups and tracks the running PS/RT/PI/PTY state, emitting an event
/// whenever a field changes.
pub struct Groups {
    /// Info words for the current group by position; `None` if that block failed its check.
    blocks: [Option<u16>; 4],
    ps: PsBuffer,
    rt: RtBuffer,
    last_pi: Option<u16>,
    last_pty: Option<u8>,
    /// RT+ state: the 5-bit application group-type code that carries the tags (named by a 3A
    /// registration), the last item toggle, and the last title/artist emitted (for dedup).
    rtplus_code: Option<u8>,
    rtplus_toggle: Option<u8>,
    rtplus_title: Option<String>,
    rtplus_artist: Option<String>,
    /// Long PS (15A, UTF-8, 32 bytes), PTYN (10A, 8 bytes), and the last clock-time emitted.
    long_ps: SegmentText,
    ptyn: SegmentText,
    last_ct: Option<String>,
}

impl Groups {
    pub fn new() -> Self {
        Self {
            blocks: [None; 4],
            ps: PsBuffer::new(),
            rt: RtBuffer::new(),
            last_pi: None,
            last_pty: None,
            rtplus_code: None,
            rtplus_toggle: None,
            rtplus_title: None,
            rtplus_artist: None,
            long_ps: SegmentText::new(32, true),
            ptyn: SegmentText::new(8, false),
            last_ct: None,
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

        if pos == 3 {
            self.parse(out);
            self.blocks = [None; 4];
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

        // RT+ ODA registration (group 3A): block D is the AID, block B's low 5 bits name the
        // group type that will carry the RT+ tags.
        if group_type == 3 && !version_b && self.blocks[3] == Some(0x4BD7) {
            self.rtplus_code = Some((b & 0x1F) as u8);
        }
        // If this group is the registered RT+ carrier, pull title/artist out of the RadioText.
        if self.rtplus_code == Some(((group_type as u8) << 1) | u8::from(version_b)) {
            self.parse_rtplus(b, out);
        }

        match (group_type, version_b) {
            (0, _) => self.parse_ps(b, out),
            (2, _) => self.parse_rt(b, version_b, out),
            (4, false) => self.parse_ct(b, out),
            (10, false) => self.parse_ptyn(b, out),
            (15, false) => self.parse_long_ps(b, out),
            _ => {}
        }
    }

    /// Group 15A: Long PS name (RDS2). A 3-bit segment in block B; four UTF-8 bytes per group,
    /// two in block C and two in block D, each half written independently.
    fn parse_long_ps(&mut self, b: u16, out: &mut Vec<Event>) {
        let base = (b & 0x7) as usize * 4;
        if let Some(c) = self.blocks[2] {
            self.long_ps.set(base, (c >> 8) as u8);
            self.long_ps.set(base + 1, (c & 0xFF) as u8);
        }
        if let Some(d) = self.blocks[3] {
            self.long_ps.set(base + 2, (d >> 8) as u8);
            self.long_ps.set(base + 3, (d & 0xFF) as u8);
        }
        if let Some(name) = self.long_ps.value() {
            out.push(Event::Rds(RdsEvent::LongProgramService(name)));
        }
    }

    /// Group 10A: Program Type Name, eight characters across two 1-bit-addressed segments, reset
    /// on the block-B A/B flag. Needs both C and D.
    fn parse_ptyn(&mut self, b: u16, out: &mut Vec<Event>) {
        self.ptyn.set_flag(((b >> 4) & 1) as u8);
        let (Some(c), Some(d)) = (self.blocks[2], self.blocks[3]) else {
            return;
        };
        let base = (b & 0x1) as usize * 4;
        self.ptyn.set(base, (c >> 8) as u8);
        self.ptyn.set(base + 1, (c & 0xFF) as u8);
        self.ptyn.set(base + 2, (d >> 8) as u8);
        self.ptyn.set(base + 3, (d & 0xFF) as u8);
        if let Some(name) = self.ptyn.value() {
            out.push(Event::Rds(RdsEvent::ProgramTypeName(name)));
        }
    }

    /// Group 4A: clock-time and date. The Modified Julian Day spans block B's low bit and block C;
    /// the UTC time and a local offset (in half-hours) sit in C and D. Emitted as ISO-8601 with
    /// the offset (the MJD->Gregorian conversion is EN 50067 Annex G).
    fn parse_ct(&mut self, b: u16, out: &mut Vec<Event>) {
        let (Some(c), Some(d)) = (self.blocks[2], self.blocks[3]) else {
            return;
        };
        let mjd = (((u32::from(b) << 16) | u32::from(c)) >> 1) & 0x1_FFFF;
        if mjd < 15079 {
            return;
        }
        let hour = (((u32::from(c) << 16) | u32::from(d)) >> 12) & 0x1F;
        let minute = (d >> 6) & 0x3F;
        let off_half = (d & 0x1F) as i64;
        if hour > 23 || minute > 59 || off_half > 28 {
            return;
        }

        let mjdf = mjd as f64;
        let y = ((mjdf - 15078.2) / 365.25) as i64;
        let m = ((mjdf - 14956.1 - (y as f64 * 365.25).trunc()) / 30.6001) as i64;
        let day =
            (mjdf - 14956.0 - (y as f64 * 365.25).trunc() - (m as f64 * 30.6001).trunc()) as i64;
        let (mut year, mut month) = (y, m);
        if month == 14 || month == 15 {
            year += 1;
            month -= 12;
        }
        year += 1900;
        month -= 1;

        let offset = if off_half == 0 {
            "Z".to_string()
        } else {
            let sign = if (d >> 5) & 1 == 1 { '-' } else { '+' };
            format!("{sign}{:02}:{:02}", off_half / 2, (off_half % 2) * 30)
        };
        let ct = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:00{offset}");
        if self.last_ct.as_deref() != Some(ct.as_str()) {
            self.last_ct = Some(ct.clone());
            out.push(Event::Rds(RdsEvent::ClockTime(ct)));
        }
    }

    /// Parse an RT+ data group: up to two tags, each a (content type, start, length) triple that
    /// slices the current RadioText. Content type 1 is the item title, 4 the artist. The fields
    /// straddle blocks; layout transcribed from redsea (tag 2's length is only 5 bits).
    fn parse_rtplus(&mut self, b: u16, out: &mut Vec<Event>) {
        let toggle = ((b >> 4) & 1) as u8;
        // A toggle flip signals a new item; forget the old tags so the new ones re-emit.
        if self.rtplus_toggle != Some(toggle) {
            self.rtplus_toggle = Some(toggle);
            self.rtplus_title = None;
            self.rtplus_artist = None;
        }

        let Some(c) = self.blocks[2] else { return };
        let mut tags = [(0u32, 0usize, 0usize); 2];
        tags[0] = (
            ((u32::from(b) << 16 | u32::from(c)) >> 13) & 0x3F,
            ((c >> 7) & 0x3F) as usize,
            ((c >> 1) & 0x3F) as usize + 1,
        );
        let n = if let Some(d) = self.blocks[3] {
            tags[1] = (
                ((u32::from(c) << 16 | u32::from(d)) >> 11) & 0x3F,
                ((d >> 5) & 0x3F) as usize,
                (d & 0x1F) as usize + 1,
            );
            2
        } else {
            1
        };

        let (mut title, mut artist) = (None, None);
        for &(ct, start, len) in &tags[..n] {
            let Some(text) = (ct != 0).then(|| self.rt.substring(start, len)).flatten() else {
                continue;
            };
            match ct {
                1 => title = Some(text),
                4 => artist = Some(text),
                _ => {}
            }
        }

        let changed = (title.is_some() && title != self.rtplus_title)
            || (artist.is_some() && artist != self.rtplus_artist);
        if title.is_some() {
            self.rtplus_title = title;
        }
        if artist.is_some() {
            self.rtplus_artist = artist;
        }
        if changed {
            out.push(Event::Rds(RdsEvent::RadioTextPlus {
                title: self.rtplus_title.clone(),
                artist: self.rtplus_artist.clone(),
            }));
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
                _ => {}
            }
        }

        assert_eq!(got_pi, Some(pi));
        assert_eq!(got_pty, Some(5));
        assert_eq!(got_ps.as_deref(), Some("KEXP-FM"));
        assert_eq!(got_rt.as_deref(), Some("Now Playing: Radiohead - Creep"));
    }

    /// A group 3A registering RT+ (AID 0x4BD7) on group type 11A (application code 22).
    fn group_3a_rtplus(bits: &mut Vec<u8>, pi: u16) {
        let b = (3u16 << 12) | (5 << 5) | 22;
        push_block(bits, make_block(pi, A));
        push_block(bits, make_block(b, OFF_B));
        push_block(bits, make_block(0x0000, C));
        push_block(bits, make_block(0x4BD7, OFF_D));
    }

    /// An 11A group carrying two RT+ tags in blocks C and D (toggle 0, running 1).
    fn group_11a_rtplus(bits: &mut Vec<u8>, pi: u16, c: u16, d: u16) {
        let b = (11u16 << 12) | (5 << 5) | 0b0_1000;
        push_block(bits, make_block(pi, A));
        push_block(bits, make_block(b, OFF_B));
        push_block(bits, make_block(c, C));
        push_block(bits, make_block(d, OFF_D));
    }

    #[test]
    fn recovers_rtplus_title_and_artist() {
        let pi = 0x4D54;
        let rt = b"MANIC MONDAY by THE BANGLES     "; // 32 chars
                                                      // tag 1: title (ct 1) start 0, length field 11 -> 12 chars "MANIC MONDAY".
                                                      // tag 2: artist (ct 4) start 16, length field 10 -> 11 chars "THE BANGLES".
        let (c, d) = (0x2016u16, 0x220Au16);

        let mut bits = Vec::new();
        for _ in 0..8 {
            group_3a_rtplus(&mut bits, pi);
            for seg in 0..8u16 {
                let i = seg as usize * 4;
                group_2a(
                    &mut bits,
                    pi,
                    5,
                    0,
                    seg,
                    &[rt[i], rt[i + 1], rt[i + 2], rt[i + 3]],
                );
            }
            group_11a_rtplus(&mut bits, pi, c, d);
        }

        let mut sync = BlockSync::new();
        let mut groups = Groups::new();
        let mut events = Vec::new();
        for &bit in &bits {
            if let Some(block) = sync.push(bit) {
                groups.push_block(block, &mut events);
            }
        }

        let (mut title, mut artist) = (None, None);
        for e in &events {
            if let Event::Rds(RdsEvent::RadioTextPlus {
                title: t,
                artist: a,
            }) = e
            {
                if t.is_some() {
                    title = t.clone();
                }
                if a.is_some() {
                    artist = a.clone();
                }
            }
        }
        assert_eq!(title.as_deref(), Some("MANIC MONDAY"));
        assert_eq!(artist.as_deref(), Some("THE BANGLES"));
    }

    fn group_abcd(bits: &mut Vec<u8>, pi: u16, b: u16, c: u16, d: u16) {
        push_block(bits, make_block(pi, A));
        push_block(bits, make_block(b, OFF_B));
        push_block(bits, make_block(c, C));
        push_block(bits, make_block(d, OFF_D));
    }

    /// Decode a stream of identical groups and return all events (after sync acquires).
    fn decode_groups(make: impl Fn(&mut Vec<u8>), reps: usize) -> Vec<Event> {
        let mut bits = Vec::new();
        for _ in 0..reps {
            make(&mut bits);
        }
        let mut sync = BlockSync::new();
        let mut groups = Groups::new();
        let mut events = Vec::new();
        for &bit in &bits {
            if let Some(block) = sync.push(bit) {
                groups.push_block(block, &mut events);
            }
        }
        events
    }

    #[test]
    fn recovers_long_ps_15a() {
        let pi = 0x4D54;
        let name = b"The Beat - Montreal Top 40 Hits!"; // 32 chars, no terminator
        let events = decode_groups(
            |bits| {
                for seg in 0..8u16 {
                    let i = seg as usize * 4;
                    let (c, d) = (
                        ((name[i] as u16) << 8) | name[i + 1] as u16,
                        ((name[i + 2] as u16) << 8) | name[i + 3] as u16,
                    );
                    group_abcd(bits, pi, (15 << 12) | (5 << 5) | seg, c, d);
                }
            },
            6,
        );
        let long_ps = events.iter().find_map(|e| match e {
            Event::Rds(RdsEvent::LongProgramService(s)) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(long_ps.as_deref(), Some("The Beat - Montreal Top 40 Hits!"));
    }

    #[test]
    fn recovers_ptyn_10a() {
        let pi = 0x4D54;
        let name = b"Top Hits"; // 8 chars across two segments
        let events = decode_groups(
            |bits| {
                for seg in 0..2u16 {
                    let i = seg as usize * 4;
                    let (c, d) = (
                        ((name[i] as u16) << 8) | name[i + 1] as u16,
                        ((name[i + 2] as u16) << 8) | name[i + 3] as u16,
                    );
                    group_abcd(bits, pi, (10 << 12) | (5 << 5) | seg, c, d);
                }
            },
            8,
        );
        let ptyn = events.iter().find_map(|e| match e {
            Event::Rds(RdsEvent::ProgramTypeName(s)) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(ptyn.as_deref(), Some("Top Hits"));
    }

    #[test]
    fn recovers_clock_time_4a() {
        // MJD 58849 = 2020-01-01, 12:34 UTC, zero offset. Block words built to match the layout.
        let pi = 0x4D54;
        let events = decode_groups(|bits| group_abcd(bits, pi, 0x40A1, 0xCBC2, 0xC880), 6);
        let ct = events.iter().find_map(|e| match e {
            Event::Rds(RdsEvent::ClockTime(s)) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(ct.as_deref(), Some("2020-01-01T12:34:00Z"));
    }
}
