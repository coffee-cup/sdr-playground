//! Argument parsers and human-readable formatting for the CLI.

use sdr_engine::Gain;

/// Parse a number with an optional SI suffix (k/M/G, case-insensitive) into base units.
fn parse_si(s: &str) -> Result<f64, String> {
    let s = s.trim();
    let (digits, mult) = match s.chars().last() {
        Some('k' | 'K') => (&s[..s.len() - 1], 1e3),
        Some('m' | 'M') => (&s[..s.len() - 1], 1e6),
        Some('g' | 'G') => (&s[..s.len() - 1], 1e9),
        _ => (s, 1.0),
    };
    digits
        .trim()
        .parse::<f64>()
        .map(|n| n * mult)
        .map_err(|_| format!("invalid number: '{s}'"))
}

pub fn parse_freq(s: &str) -> Result<u64, String> {
    Ok(parse_si(s)?.round() as u64)
}

pub fn parse_rate(s: &str) -> Result<u32, String> {
    let v = parse_si(s)?.round();
    if !(1.0..=u32::MAX as f64).contains(&v) {
        return Err(format!("sample rate out of range: '{s}'"));
    }
    Ok(v as u32)
}

pub fn parse_gain(s: &str) -> Result<Gain, String> {
    if s.eq_ignore_ascii_case("auto") {
        return Ok(Gain::Auto);
    }
    let db = s
        .parse::<f32>()
        .map_err(|_| format!("gain must be 'auto' or a number in dB: '{s}'"))?;
    Ok(Gain::Manual((db * 10.0).round() as i32))
}

pub fn gain_label(gain: Gain) -> String {
    match gain {
        Gain::Auto => "auto".to_string(),
        Gain::Manual(tenths) => format!("{:.1} dB", tenths as f32 / 10.0),
    }
}

pub fn freq(hz: u64) -> String {
    let hz = hz as f64;
    if hz >= 1e9 {
        format!("{:.3} GHz", hz / 1e9)
    } else if hz >= 1e6 {
        format!("{:.3} MHz", hz / 1e6)
    } else if hz >= 1e3 {
        format!("{:.3} kHz", hz / 1e3)
    } else {
        format!("{hz:.0} Hz")
    }
}

pub fn rate(sps: u32) -> String {
    let sps = sps as f64;
    if sps >= 1e6 {
        format!("{:.3} MS/s", sps / 1e6)
    } else if sps >= 1e3 {
        format!("{:.1} kS/s", sps / 1e3)
    } else {
        format!("{sps:.0} S/s")
    }
}

pub fn count(n: u64) -> String {
    let n = n as f64;
    if n >= 1e6 {
        format!("{:.2}M", n / 1e6)
    } else if n >= 1e3 {
        format!("{:.1}k", n / 1e3)
    } else {
        format!("{n:.0}")
    }
}

pub fn db(value: f32) -> String {
    if value.is_finite() {
        format!("{value:.1}")
    } else {
        "-inf".to_string()
    }
}

/// Unicode block ramp for the spectrum sparkline, low → high (first is a space for the floor).
const BLOCKS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render a fftshifted dBFS spectrum as a `width`-column sparkline. Bins are max-pooled per
/// column (peaks survive downsampling). The dB scale auto-ranges per frame — floor at the
/// median (the noise sits at the bottom), ceil at the max — so signals pop at any gain.
pub fn sparkline(bins: &[f32], width: usize) -> String {
    let n = bins.len();
    if n == 0 || width == 0 {
        return String::new();
    }

    let mut finite: Vec<f32> = bins.iter().copied().filter(|d| d.is_finite()).collect();
    if finite.is_empty() {
        return " ".repeat(width);
    }
    finite.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let floor = finite[finite.len() / 2]; // median ≈ noise floor
    let ceil = *finite.last().unwrap();
    let span = (ceil - floor).max(1.0);

    (0..width)
        .map(|c| {
            let start = c * n / width;
            let end = ((c + 1) * n / width).clamp(start + 1, n);
            let db = bins[start..end]
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let t = ((db - floor) / span).clamp(0.0, 1.0);
            BLOCKS[(t * (BLOCKS.len() - 1) as f32).round() as usize]
        })
        .collect()
}

/// A human readout of the strongest bin: absolute frequency, signed offset from center, dB.
pub fn peak_readout(bins: &[f32], center_freq: u64, sample_rate: u32) -> String {
    let n = bins.len();
    let Some((bin, &peak_db)) = bins
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
    else {
        return String::new();
    };
    let offset_hz = (bin as i64 - n as i64 / 2) * sample_rate as i64 / n as i64;
    let abs = (center_freq as i64 + offset_hz).max(0) as u64;
    let sign = if offset_hz < 0 { "-" } else { "+" };
    format!(
        "{}  ({}{})  {} dBFS",
        freq(abs),
        sign,
        freq(offset_hz.unsigned_abs()),
        db(peak_db),
    )
}
