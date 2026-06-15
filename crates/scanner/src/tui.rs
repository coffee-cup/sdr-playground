//! Terminal UI for the band scan: a live table of stations and what they are playing, updated
//! as the scanner sweeps the band in the background.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Line;
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};

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

const HEADERS: [&str; 4] = ["Freq", "Station", "Type", "Now Playing"];
/// Per-column minimum width, so the table starts at a comfortable size before stations fill in
/// rather than collapsing to the header text.
const COL_MINS: [usize; 4] = [7, 12, 14, 28];
/// Per-column maximum width; the column grows with its content up to here, then truncates.
const COL_CAPS: [usize; 4] = [7, 18, 18, 48];

fn draw(
    frame: &mut ratatui::Frame,
    table: &crate::StationTable,
    current: &std::sync::atomic::AtomicU64,
    windows: usize,
) {
    let area = frame.area();
    let stations = table.stations();
    let tuned = current.load(Ordering::Relaxed);

    let cell = |s: &crate::store::Station, i: usize| match i {
        0 => format!("{:.1}", s.freq as f64 / 1e6),
        1 => s.name().unwrap_or_default().to_string(),
        2 => s.type_label().unwrap_or_default().to_string(),
        _ => s.now_playing().unwrap_or_default(),
    };

    // Each column is only as wide as its widest value (or header), capped, so the table takes
    // just the space it needs rather than stretching across the terminal.
    let widths: Vec<u16> = (0..4)
        .map(|i| {
            let content = stations.iter().map(|s| cell(s, i).chars().count()).max();
            content
                .unwrap_or(0)
                .max(HEADERS[i].len())
                .max(COL_MINS[i])
                .min(COL_CAPS[i]) as u16
        })
        .collect();

    let status = Line::from(vec![
        "scanning ".into(),
        format!("{:.1} MHz", tuned as f64 / 1e6).yellow().bold(),
        "   ".into(),
        format!("{} stations", stations.len()).green(),
        "   ".into(),
        format!("{windows} windows").dark_gray(),
        "   ".into(),
        "q quits".dark_gray(),
    ]);

    // Box size = content size, clamped to the terminal. Width: columns + 3 spacings + 2 border,
    // at least the status line. Height: border + status + blank + header + one row per station.
    let table_w = widths.iter().sum::<u16>() + 3;
    let inner_w = table_w.max(status.width() as u16);
    let box_w = (inner_w + 2).min(area.width);
    let box_h = (stations.len() as u16 + 5).min(area.height);
    let rect = Rect::new(0, 0, box_w, box_h);

    let block = Block::bordered()
        .border_style(Style::new().fg(Color::DarkGray))
        .title("FM RDS Scanner".cyan().bold());
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let parts = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(inner);
    frame.render_widget(Paragraph::new(status), parts[0]);

    let header = Row::new(HEADERS.map(Cell::from))
        .style(Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let rows = stations.iter().map(|s| {
        // Highlight structured RT+ now-playing differently from raw RadioText.
        let np_style = if s.artist.is_some() || s.title.is_some() {
            Style::new().fg(Color::Yellow)
        } else {
            Style::new().fg(Color::Gray)
        };
        Row::new([
            Cell::from(cell(s, 0)).style(Style::new().fg(Color::White)),
            Cell::from(cell(s, 1))
                .style(Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Cell::from(cell(s, 2)).style(Style::new().fg(Color::Blue)),
            Cell::from(cell(s, 3)).style(np_style),
        ])
    });
    let constraints: Vec<Constraint> = widths.iter().map(|&w| Constraint::Length(w)).collect();
    frame.render_widget(
        Table::new(rows, constraints)
            .header(header)
            .column_spacing(1),
        parts[2],
    );
}
