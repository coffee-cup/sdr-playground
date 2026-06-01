//! `sdr` — the headless front-end. A peer of the GUI `app` over the same `engine`, which is
//! what keeps the core free of any UI dependency. See `docs/ARCHITECTURE.md`.
//!
//! For now it does one useful thing: read raw IQ from the RTL-SDR (or a recorded file) and
//! show a live readout of what's flowing. No decoders yet.

mod format;

use std::io::Write;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use owo_colors::OwoColorize;

use sdr_engine::{
    Engine, EngineConfig, FileSource, Gain, RtlConfig, RtlSdrSource, Source, SpectrumConfig,
};

#[derive(Parser)]
#[command(
    name = "sdr",
    version,
    about = "Software-defined radio — headless front-end"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect connected devices.
    #[command(subcommand)]
    Device(DeviceCmd),
    /// Read IQ from a device (or file) and show a live readout.
    Listen(ListenArgs),
}

#[derive(Subcommand)]
enum DeviceCmd {
    /// List connected RTL-SDR devices.
    List,
    /// Show details for one device.
    Info(IndexArg),
}

#[derive(Args)]
struct IndexArg {
    /// Device index among detected RTL-SDRs.
    #[arg(long, default_value_t = 0)]
    index: usize,
}

#[derive(Args)]
struct ListenArgs {
    /// Device index among detected RTL-SDRs.
    #[arg(long, default_value_t = 0)]
    index: usize,
    /// Center frequency, e.g. 100M, 433.92M, 2.048G.
    #[arg(long, value_parser = format::parse_freq, default_value = "100M")]
    freq: u64,
    /// Sample rate, e.g. 2.048M.
    #[arg(long, value_parser = format::parse_rate, default_value = "2.048M")]
    rate: u32,
    /// Tuner gain: `auto` or a value in dB (e.g. 30).
    #[arg(long, value_parser = format::parse_gain, default_value = "auto")]
    gain: Gain,
    /// Replay a raw cu8 IQ file instead of opening a device.
    #[arg(long)]
    file: Option<String>,
    /// FFT size for the spectrum (samples).
    #[arg(long, default_value_t = 8192)]
    fft_size: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Device(DeviceCmd::List) => device_list(),
        Command::Device(DeviceCmd::Info(args)) => device_info(args.index),
        Command::Listen(args) => listen(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("{} {msg}", "error:".red().bold());
            ExitCode::FAILURE
        }
    }
}

fn device_list() -> Result<(), String> {
    let devices = RtlSdrSource::list().map_err(|e| e.to_string())?;
    if devices.is_empty() {
        println!("No RTL-SDR devices found.");
        return Ok(());
    }
    println!("{}", "Connected RTL-SDR devices:".bold());
    for d in devices {
        println!(
            "  {} {} {} {}",
            format!("[{}]", d.index).cyan().bold(),
            d.manufacturer,
            d.name.bold(),
            format!("(serial {})", d.serial).dimmed(),
        );
    }
    Ok(())
}

fn device_info(index: usize) -> Result<(), String> {
    let source = RtlSdrSource::open(index, RtlConfig::default()).map_err(|e| e.to_string())?;
    let info = source.info();
    println!("{}", info.name.bold());
    field("index", &info.index.to_string());
    field("manufacturer", &info.manufacturer);
    field("serial", &info.serial);
    field("tuner", info.tuner.as_deref().unwrap_or("unknown"));

    if let Ok(gains) = source.supported_gains() {
        let steps: Vec<String> = gains
            .iter()
            .map(|t| format!("{:.1}", *t as f32 / 10.0))
            .collect();
        field("gains (dB)", &steps.join(" "));
    }
    Ok(())
}

fn field(label: &str, value: &str) {
    println!("  {:<14} {}", format!("{label}:").dimmed(), value);
}

fn listen(args: ListenArgs) -> Result<(), String> {
    let source: Box<dyn Source> = if let Some(path) = &args.file {
        let source = FileSource::open_cu8(path, args.rate, args.freq).map_err(|e| e.to_string())?;
        println!("{} {}", "replaying".bold(), path.bold());
        Box::new(source)
    } else {
        let cfg = RtlConfig {
            freq_hz: args.freq,
            sample_rate: args.rate,
            gain: args.gain,
        };
        let source = RtlSdrSource::open(args.index, cfg).map_err(|e| e.to_string())?;
        let info = source.info();
        println!(
            "{}  tuner={}  {}",
            info.name.bold(),
            info.tuner.as_deref().unwrap_or("unknown").bold(),
            format!("serial {}", info.serial).dimmed(),
        );
        Box::new(source)
    };

    header(args.freq, args.rate, &format::gain_label(args.gain));
    let config = EngineConfig {
        spectrum: SpectrumConfig {
            fft_size: args.fft_size,
            ..Default::default()
        },
        ..Default::default()
    };
    run_live(Engine::start(source, config), args.rate);
    Ok(())
}

fn header(freq: u64, rate: u32, gain: &str) {
    println!(
        "center={}  rate={}  gain={}",
        format::freq(freq).bold(),
        format::rate(rate).bold(),
        gain.bold(),
    );
    println!("{}", "Ctrl-C to stop.".dimmed());
}

/// Columns in the ASCII spectrum sparkline.
const SPECTRUM_WIDTH: usize = 80;

fn run_live(engine: Engine, rate: u32) {
    let stop = Arc::new(AtomicBool::new(false));
    let handler_flag = Arc::clone(&stop);
    let _ = ctrlc::set_handler(move || handler_flag.store(true, Ordering::SeqCst));

    let mut stdout = std::io::stdout();
    while !stop.load(Ordering::SeqCst) {
        let s = engine.snapshot();
        let spec = engine.spectrum();

        // Throughput within 2% of the configured rate reads as healthy.
        let healthy = s.throughput_sps >= rate as f64 * 0.98;
        let tput = format::rate(s.throughput_sps as u32);
        let tput = if healthy {
            tput.green().to_string()
        } else {
            tput.yellow().to_string()
        };
        let stats = format!(
            "{} {}  rx={}  pwr={}  peak={}",
            "[live]".dimmed(),
            tput,
            format::count(s.total_samples).bold(),
            format!("{} dBFS", format::db(s.mean_dbfs)).cyan(),
            format!("{} dBFS", format::db(s.peak_dbfs)).cyan(),
        );

        let (spark, peak) = if spec.seq > 0 {
            let bar = format::sparkline(&spec.bins_db, SPECTRUM_WIDTH);
            (
                format!(" {} {}", "spec".dimmed(), bar),
                format::peak_readout(&spec.bins_db, spec.center_freq, spec.sample_rate),
            )
        } else {
            (
                format!(" {} {}", "spec".dimmed(), "acquiring…".dimmed()),
                String::new(),
            )
        };
        let peak = format!(" {} {}", "peak".dimmed(), peak.yellow());

        redraw(&mut stdout, &[stats, spark, peak]);

        if !s.running {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let final_snapshot = engine.snapshot();
    engine.stop();
    println!("\n");
    println!(
        "{} {} samples read.",
        "done:".bold(),
        format::count(final_snapshot.total_samples),
    );
}

/// Redraw a block of lines in place: clear each line, then park the cursor back on the first
/// so the next call overwrites the same block.
fn redraw(stdout: &mut impl Write, lines: &[String]) {
    let mut out = String::from("\r");
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str("\x1b[2K"); // clear entire line
        out.push_str(line);
    }
    if lines.len() > 1 {
        out.push_str(&format!("\x1b[{}A", lines.len() - 1)); // cursor up to the first line
    }
    out.push('\r');
    let _ = stdout.write_all(out.as_bytes());
    let _ = stdout.flush();
}
