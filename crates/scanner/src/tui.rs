//! Terminal UI for the band scan: a live table of stations and what they are playing, updated
//! as the scanner sweeps the band in the background.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::Stylize;
use ratatui::widgets::{Block, Paragraph, Row, Table};

use crate::Scanner;

/// Run the scanner with a live TUI. Blocks until the user quits (`q`/`Esc`).
pub fn run(scanner: Scanner) -> io::Result<()> {
    let table = scanner.table();
    let current = scanner.current();
    let windows = scanner.windows().len();

    let stop = Arc::new(AtomicBool::new(false));
    let scan_stop = Arc::clone(&stop);
    let handle = thread::spawn(move || scanner.run(scan_stop));

    let mut terminal = ratatui::init();
    let result = loop {
        if let Err(e) = terminal.draw(|frame| draw(frame, &table, &current, windows)) {
            break Err(e);
        }
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                // Raw mode suppresses SIGINT, so Ctrl+C arrives here as a key, not a signal.
                let ctrl_c =
                    k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL);
                if ctrl_c || matches!(k.code, KeyCode::Char('q') | KeyCode::Esc) {
                    break Ok(());
                }
            }
        }
    };

    ratatui::restore();
    stop.store(true, Ordering::SeqCst);
    let _ = handle.join();
    result
}

fn draw(
    frame: &mut ratatui::Frame,
    table: &crate::StationTable,
    current: &std::sync::atomic::AtomicU64,
    windows: usize,
) {
    let stations = table.stations();
    let tuned = current.load(Ordering::Relaxed);
    let layout = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(frame.area());

    let status = format!(
        " scanning {:.1} MHz   {} stations found   {} windows   (q to quit) ",
        tuned as f64 / 1e6,
        stations.len(),
        windows,
    );
    frame.render_widget(
        Paragraph::new(status).block(Block::bordered().title("FM RDS Scanner")),
        layout[0],
    );

    let rows = stations.iter().map(|s| {
        Row::new(vec![
            format!("{:.1}", s.freq as f64 / 1e6),
            s.program_service.clone().unwrap_or_default(),
            s.pty_name().unwrap_or("").to_string(),
            s.now_playing().unwrap_or_default(),
        ])
    });
    let widths = [
        Constraint::Length(8),
        Constraint::Length(12),
        Constraint::Length(16),
        Constraint::Min(20),
    ];
    let header = Row::new(vec!["Freq", "Station", "Type", "Now Playing / RadioText"]).bold();
    frame.render_widget(
        Table::new(rows, widths)
            .header(header)
            .block(Block::bordered()),
        layout[1],
    );
}
