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
