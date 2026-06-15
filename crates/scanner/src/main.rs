//! `sdr-scan` — scan the FM band for RDS station/song data.
//!
//! `probe` decodes one station headlessly (for validation against the live device); `scan`
//! sweeps the whole band and prints the station table. Both ride the shared `engine`.

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use clap::{Args, Parser, Subcommand};
use sdr_engine::{
    ChannelSpec, Engine, EngineConfig, Event, FileSource, Gain, RdsEvent, RtlConfig, RtlSdrSource,
    Source,
};
use sdr_scanner::{Region, Scanner};

#[derive(Parser)]
#[command(
    name = "sdr-scan",
    version,
    about = "Scan FM stations for RDS song/artist data"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Decode RDS from a single station and print events (headless, for validation).
    Probe(ProbeArgs),
    /// Sweep the whole FM band, printing the station table as it fills in.
    Scan(ScanArgs),
}

#[derive(Args)]
struct ProbeArgs {
    /// Station frequency, e.g. 92.5M.
    #[arg(long, value_parser = parse_hz)]
    freq: u64,
    /// How long to listen, seconds.
    #[arg(long, default_value_t = 20.0)]
    secs: f64,
    /// Device index.
    #[arg(long, default_value_t = 0)]
    index: usize,
    /// Sample rate.
    #[arg(long, value_parser = parse_rate, default_value = "1.024M")]
    rate: u32,
    /// Tuner gain: `auto` or dB.
    #[arg(long, value_parser = parse_gain, default_value = "40")]
    gain: Gain,
    /// Replay a cu8 file instead of the device (centered on `--freq`).
    #[arg(long)]
    file: Option<String>,
}

#[derive(Args)]
struct ScanArgs {
    /// Device index.
    #[arg(long, default_value_t = 0)]
    index: usize,
    /// Sample rate.
    #[arg(long, value_parser = parse_rate, default_value = "1.024M")]
    rate: u32,
    /// Tuner gain: `auto` or dB.
    #[arg(long, value_parser = parse_gain, default_value = "40")]
    gain: Gain,
    /// Dwell per window, seconds (RDS needs several seconds to deliver PS/RadioText).
    #[arg(long, default_value_t = 12.0)]
    dwell: f64,
    /// Print the station table to stdout instead of the TUI (headless logging).
    #[arg(long)]
    print: bool,
    /// Start the sweep at this frequency instead of the bottom of the band.
    #[arg(long, value_parser = parse_hz)]
    start: Option<u64>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Probe(args) => probe(args),
        Command::Scan(args) => scan(args),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn probe(args: ProbeArgs) -> Result<(), String> {
    // Offset-tune: place the station off the window center so it avoids the RTL-SDR DC spike,
    // and decode it via the channel mixer (the same path the band scan uses for every station).
    let offset = 250_000.0;
    let center = (args.freq as f64 - offset).max(0.0) as u64;
    let source = open_source(
        args.file.as_deref(),
        args.index,
        center,
        args.rate,
        args.gain,
    )?;
    let engine = Engine::start(source, EngineConfig::default());
    engine.tune(center);
    engine.set_channels(vec![ChannelSpec::rds(offset)]);

    println!(
        "probing {:.3} MHz for {}s ...",
        args.freq as f64 / 1e6,
        args.secs
    );
    let stop = install_ctrlc();
    let deadline = Instant::now() + Duration::from_secs_f64(args.secs);
    let (mut pi, mut pty, mut ps, mut rt) = (None, None, None, None);
    while Instant::now() < deadline && !stop.load(Ordering::SeqCst) {
        for ce in engine.drain_events() {
            match ce.event {
                Event::Rds(RdsEvent::Pi(v)) => pi = Some(v),
                Event::Rds(RdsEvent::ProgramType(p)) => pty = Some(p),
                Event::Rds(RdsEvent::ProgramService(s)) => {
                    println!("  PS: {s:?}");
                    ps = Some(s);
                }
                Event::Rds(RdsEvent::RadioText(s)) => {
                    println!("  RT: {s:?}");
                    rt = Some(s);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    println!("---");
    println!(
        "PI        {}",
        pi.map(|v| format!("0x{v:04X}"))
            .unwrap_or_else(|| "(none)".into())
    );
    println!("PTY       {:?}", pty.map(sdr_engine::pty_name));
    println!("PS        {ps:?}");
    println!("RadioText {rt:?}");
    if pi.is_none() {
        return Err("no RDS decoded (signal too weak, no RDS, or decoder needs work)".into());
    }
    Ok(())
}

fn scan(args: ScanArgs) -> Result<(), String> {
    let source = open_source(None, args.index, 100_000_000, args.rate, args.gain)?;
    let engine = Engine::start(source, EngineConfig::default());
    let scanner = Scanner::new(
        engine,
        Region::Us,
        args.rate,
        Duration::from_secs_f64(args.dwell),
    );

    if !args.print {
        return sdr_scanner::tui::run(scanner).map_err(|e| e.to_string());
    }

    // Headless: sweep windows (optionally from `start`) and print decoded stations after each.
    let stop = install_ctrlc();
    let table = scanner.table();
    let windows: Vec<_> = scanner
        .windows()
        .iter()
        .filter(|w| args.start.is_none_or(|s| w.center >= s))
        .cloned()
        .collect();
    for w in &windows {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        eprintln!(
            "tuning {:.1} MHz ({} stations) ...",
            w.center as f64 / 1e6,
            w.stations.len()
        );
        scanner.dwell_window(w, &stop);
        for s in table
            .stations()
            .iter()
            .filter(|s| s.program_service.is_some())
        {
            println!(
                "{:7.1} MHz  {:8}  {}",
                s.freq as f64 / 1e6,
                s.program_service.as_deref().unwrap_or(""),
                s.radiotext.as_deref().unwrap_or(""),
            );
        }
    }
    Ok(())
}

fn open_source(
    file: Option<&str>,
    index: usize,
    freq: u64,
    rate: u32,
    gain: Gain,
) -> Result<Box<dyn Source>, String> {
    if let Some(path) = file {
        let src = FileSource::open_cu8(path, rate, freq).map_err(|e| e.to_string())?;
        Ok(Box::new(src))
    } else {
        let cfg = RtlConfig {
            freq_hz: freq,
            sample_rate: rate,
            gain,
        };
        let src = RtlSdrSource::open(index, cfg).map_err(|e| e.to_string())?;
        Ok(Box::new(src))
    }
}

fn install_ctrlc() -> Arc<AtomicBool> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let _ = ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst));
    stop
}

fn parse_hz(s: &str) -> Result<u64, String> {
    Ok(parse_si(s)?.round() as u64)
}

fn parse_rate(s: &str) -> Result<u32, String> {
    let v = parse_si(s)?.round();
    if !(1.0..=u32::MAX as f64).contains(&v) {
        return Err(format!("rate out of range: {s}"));
    }
    Ok(v as u32)
}

fn parse_gain(s: &str) -> Result<Gain, String> {
    if s.eq_ignore_ascii_case("auto") {
        return Ok(Gain::Auto);
    }
    let db: f32 = s
        .parse()
        .map_err(|_| format!("gain must be 'auto' or dB: {s}"))?;
    Ok(Gain::Manual((db * 10.0).round() as i32))
}

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
        .map_err(|_| format!("invalid number: {s}"))
}
