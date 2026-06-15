//! Structured RDS outputs.

/// A decoded RDS event. Emitted incrementally as groups arrive; a front-end aggregates these
/// into per-station state. RDS data trickles in, so a full PS name or RadioText is the result
/// of several groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RdsEvent {
    /// Program Identification: a stable 16-bit station identity (maps to call sign in the US).
    Pi(u16),
    /// Program Service: the 8-character station label (e.g. "KEXP").
    ProgramService(String),
    /// Program Type: a genre code. Resolve with [`pty_name`].
    ProgramType(u8),
    /// RadioText: free-form now-playing text, frequently "Artist - Title".
    RadioText(String),
    /// RadioText Plus: structured now-playing tags carved out of the RadioText by an ODA
    /// (AID 0x4BD7). Carries the current item title and/or artist when the station provides them.
    RadioTextPlus {
        title: Option<String>,
        artist: Option<String>,
    },
}

/// RBDS (North American) program-type names, indexed by the 5-bit PTY code. The European RDS
/// table assigns some codes differently; this is the variant that matches US/Canada broadcasts.
const PTY_RBDS: [&str; 32] = [
    "None",
    "News",
    "Information",
    "Sports",
    "Talk",
    "Rock",
    "Classic Rock",
    "Adult Hits",
    "Soft Rock",
    "Top 40",
    "Country",
    "Oldies",
    "Soft",
    "Nostalgia",
    "Jazz",
    "Classical",
    "Rhythm and Blues",
    "Soft R&B",
    "Foreign Language",
    "Religious Music",
    "Religious Talk",
    "Personality",
    "Public",
    "College",
    "Spanish Talk",
    "Spanish Music",
    "Hip Hop",
    "Unassigned",
    "Unassigned",
    "Weather",
    "Emergency Test",
    "Emergency",
];

/// Human-readable name for a 5-bit RBDS program-type code.
pub fn pty_name(pty: u8) -> &'static str {
    PTY_RBDS.get(pty as usize).copied().unwrap_or("Unknown")
}
