//! Block synchronization and error detection for the RDS data link.
//!
//! The bit stream is a continuous repetition of 104-bit groups, each four 26-bit blocks. A
//! block is 16 information bits plus a 10-bit checkword formed from a shortened cyclic code
//! (generator `0x5B9`) with a per-position "offset word" added. Computing the syndrome of a
//! 26-bit window and matching it against the five known offset syndromes both detects block
//! boundaries and identifies which block (A, B, C, C', D) we are looking at. Algorithm and
//! constants follow the IEC 62106 standard (as implemented in GNU Radio's gr-rds).

/// Offset indices A, B, C, D, C'. C' (used by version-B groups in block 3) shares block
/// position 2 with C.
pub const A: usize = 0;
pub const B: usize = 1;
pub const C: usize = 2;
pub const D: usize = 3;
pub const C_PRIME: usize = 4;

/// Syndrome produced by [`calc_syndrome`] for an error-free block carrying each offset word.
const SYNDROME: [u16; 5] = [383, 14, 303, 663, 748];
/// The offset word added to each block's checkword, indexed as above. Used only to synthesize
/// RDS for tests; the decoder identifies blocks by syndrome, not by reconstructing the word.
#[cfg(test)]
pub const OFFSET_WORD: [u16; 5] = [252, 408, 360, 436, 848];
/// Block position (0..4) within the group for each offset.
const OFFSET_POS: [usize; 5] = [0, 1, 2, 3, 2];

const BLOCK_BITS: u32 = 26;
const POLY: u32 = 0x5B9;
/// Drop sync after this many consecutive blocks fail their syndrome check.
const MAX_BAD_BLOCKS: u32 = 6;

/// One recovered 26-bit block: its 16 information bits, which offset it carries, and whether
/// the syndrome check passed.
#[derive(Debug, Clone, Copy)]
pub struct Block {
    pub info: u16,
    pub offset: usize,
    pub ok: bool,
}

/// Syndrome of the low `mlen` bits of `message` under the RDS code. Linear over GF(2), so the
/// syndrome of an error-free block equals the syndrome of its offset word alone.
pub fn calc_syndrome(message: u32, mlen: u32) -> u16 {
    let mut reg: u32 = 0;
    for i in (0..mlen).rev() {
        reg = (reg << 1) | ((message >> i) & 1);
        if reg & (1 << 10) != 0 {
            reg ^= POLY;
        }
    }
    for _ in 0..10 {
        reg <<= 1;
        if reg & (1 << 10) != 0 {
            reg ^= POLY;
        }
    }
    (reg & 0x3FF) as u16
}

/// Build a transmittable 26-bit block from 16 info bits and an offset index. Inverse of the
/// syndrome check, used to synthesize RDS for tests.
#[cfg(test)]
pub fn make_block(info: u16, offset: usize) -> u32 {
    let check = (calc_syndrome(info as u32, 16) ^ OFFSET_WORD[offset]) & 0x3FF;
    ((info as u32) << 10) | check as u32
}

/// Slides a 26-bit window over the bit stream, acquiring block sync and then tracking it.
#[derive(Default)]
pub struct BlockSync {
    reg: u32,
    total_bits: u64,
    synced: bool,
    bad: u32,
    // Acquisition: remember the last syndrome match to confirm a second one a whole number of
    // blocks away and in a consistent position.
    last_match_bit: Option<u64>,
    last_match_pos: usize,
    // Tracking: count bits within the current block and which position we expect next.
    block_bit: u32,
    expected_pos: usize,
}

impl BlockSync {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn synced(&self) -> bool {
        self.synced
    }

    /// The offset index expected at block position `pos`, given the previously seen offset's
    /// match (C and C' both live at position 2).
    fn offset_for_pos(pos: usize) -> usize {
        match pos {
            0 => A,
            1 => B,
            2 => C,
            _ => D,
        }
    }

    /// Push one received bit; return a completed block when one is delimited.
    pub fn push(&mut self, bit: u8) -> Option<Block> {
        self.reg = ((self.reg << 1) | (bit as u32 & 1)) & 0x3FF_FFFF;
        self.total_bits += 1;

        if self.synced {
            self.block_bit += 1;
            if self.block_bit < BLOCK_BITS {
                return None;
            }
            self.block_bit = 0;

            let synd = calc_syndrome(self.reg, BLOCK_BITS);
            let expected = self.expected_pos;
            // At position 2 either C or C' is valid; elsewhere a single offset.
            let ok = if expected == C {
                synd == SYNDROME[C] || synd == SYNDROME[C_PRIME]
            } else {
                synd == SYNDROME[Self::offset_for_pos(expected)]
            };
            let offset = if expected == C && synd == SYNDROME[C_PRIME] {
                C_PRIME
            } else {
                Self::offset_for_pos(expected)
            };

            if ok {
                self.bad = 0;
            } else {
                self.bad += 1;
                if self.bad >= MAX_BAD_BLOCKS {
                    self.synced = false;
                    self.last_match_bit = None;
                }
            }
            self.expected_pos = (self.expected_pos + 1) % 4;
            return Some(Block {
                info: (self.reg >> 10) as u16,
                offset,
                ok,
            });
        }

        // Acquisition: look for a syndrome match, then confirm a second one a consistent
        // number of blocks later.
        let synd = calc_syndrome(self.reg, BLOCK_BITS);
        if let Some(idx) = SYNDROME.iter().position(|&s| s == synd) {
            let pos = OFFSET_POS[idx];
            if let Some(prev_bit) = self.last_match_bit {
                let dist = self.total_bits - prev_bit;
                if dist.is_multiple_of(BLOCK_BITS as u64) {
                    let blocks = (dist / BLOCK_BITS as u64) as usize;
                    if (self.last_match_pos + blocks) % 4 == pos {
                        // Confirmed: we are sitting exactly at the end of a block at `pos`.
                        self.synced = true;
                        self.bad = 0;
                        self.block_bit = 0;
                        self.expected_pos = (pos + 1) % 4;
                        return Some(Block {
                            info: (self.reg >> 10) as u16,
                            offset: idx,
                            ok: true,
                        });
                    }
                }
            }
            self.last_match_bit = Some(self.total_bits);
            self.last_match_pos = pos;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syndromes_match_offset_words() {
        // The defining property: an error-free block carrying offset O has syndrome SYNDROME[O].
        for i in 0..5 {
            assert_eq!(
                calc_syndrome(OFFSET_WORD[i] as u32, BLOCK_BITS),
                SYNDROME[i],
                "offset index {i}"
            );
        }
    }

    #[test]
    fn made_blocks_pass_their_own_syndrome() {
        for offset in [A, B, C, D, C_PRIME] {
            let blk = make_block(0xABCD, offset);
            let synd = calc_syndrome(blk, BLOCK_BITS);
            assert_eq!(synd, SYNDROME[offset], "offset {offset}");
            assert_eq!((blk >> 10) as u16, 0xABCD);
        }
    }

    #[test]
    fn single_bit_error_breaks_syndrome() {
        let blk = make_block(0x1234, A);
        let corrupted = blk ^ (1 << 5);
        assert_ne!(calc_syndrome(corrupted, BLOCK_BITS), SYNDROME[A]);
    }

    /// Feed a stream of groups (each four blocks A,B,C,D) and confirm the sync recovers every
    /// block in order with no errors, after a short acquisition delay.
    #[test]
    fn acquires_and_tracks_a_clean_stream() {
        let pi = 0x4D54u16;
        let mut bits = Vec::new();
        let push_block = |bits: &mut Vec<u8>, word: u32| {
            for i in (0..26).rev() {
                bits.push(((word >> i) & 1) as u8);
            }
        };
        // 20 identical groups: A=PI, B=0x0123, C=0x4567, D=0x89AB.
        for _ in 0..20 {
            push_block(&mut bits, make_block(pi, A));
            push_block(&mut bits, make_block(0x0123, B));
            push_block(&mut bits, make_block(0x4567, C));
            push_block(&mut bits, make_block(0x89AB, D));
        }

        let mut sync = BlockSync::new();
        let mut blocks = Vec::new();
        for &b in &bits {
            if let Some(blk) = sync.push(b) {
                blocks.push(blk);
            }
        }

        assert!(sync.synced(), "should have acquired sync");
        // Once tracking, blocks cycle A,B,C,D and all pass.
        let tail = &blocks[blocks.len() - 8..];
        let offsets: Vec<usize> = tail.iter().map(|b| b.offset).collect();
        assert_eq!(offsets, vec![A, B, C, D, A, B, C, D]);
        assert!(tail.iter().all(|b| b.ok));
        assert_eq!(tail[0].info, pi);
    }
}
