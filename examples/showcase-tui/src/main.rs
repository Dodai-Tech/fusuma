//! The Fusuma components showcase in a terminal: the same app as the desktop window and the watch, laid out in
//! character cells and drawn as text. Keys, the mouse and resizes work; Ctrl+C or Ctrl+Q quits.
//!
//! `showcase-tui --dump 80x24 [Page]` prints one frame as plain text and exits: no terminal needed, so it also
//! serves for a quick look and for tests. `--ascii` draws with ASCII only, `--mono` with no colour.

use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use fusuma::cells::{Cell, Depth, Grid};
use fusuma::{App, CellTheme, Input};
use fusuma_platform_tui::{Terminal, map_event};
use fusuma_showcase::{Page, Showcase};

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let ascii = has("--ascii") || !utf8_locale();
    if let Some(at) = args.iter().position(|a| a == "--dump") {
        return dump(&args[at + 1..], ascii);
    }
    let mut term = Terminal::open()?;
    let (cols, rows) = term.size();
    let mut app = Showcase::new_cells(u32::from(cols), u32::from(rows));
    let look = look(ascii, has("--mono") || term.depth() == Depth::Mono);
    let start = Instant::now();
    let mut last = 0u64;
    loop {
        let waiting = app.tree().needs_frame();
        let timeout = Duration::from_millis(if waiting { 16 } else { 250 });
        let mut quit = false;
        if event::poll(timeout)? {
            loop {
                let ev = event::read()?;
                if let Event::Key(k) = &ev
                    && k.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(k.code, KeyCode::Char('c' | 'q'))
                {
                    quit = true;
                }
                if let Event::Resize(w, h) = ev {
                    term.resize(w, h);
                }
                if let Some(input) = map_event(&ev, app.wants_text()) {
                    deliver(&mut app, input);
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        if quit {
            return Ok(());
        }
        let now = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        let dt = u32::try_from(now.saturating_sub(last)).unwrap_or(u32::MAX);
        last = now;
        App::tick(&mut app, dt);
        let mut frame = term.frame();
        app.frame_cells(&mut frame, &look);
        term.present(&frame)?;
    }
}

fn deliver(app: &mut Showcase, input: Input) {
    fusuma::deliver(app, input);
}

/// The look for this terminal: ASCII where Unicode may not draw, no colour where there is none.
fn look(ascii: bool, mono: bool) -> CellTheme {
    match (ascii, mono) {
        (true, _) => CellTheme::ASCII,
        (false, true) => CellTheme::MONO,
        (false, false) => CellTheme::UNICODE,
    }
}

/// Whether the locale says UTF-8 (so box drawing is safe).
fn utf8_locale() -> bool {
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.is_empty())
        .is_some_and(|v| v.to_ascii_lowercase().contains("utf"))
}

/// `COLSxROWS [Page]`: one frame, as text, on standard output.
fn dump(args: &[String], ascii: bool) -> io::Result<()> {
    let size = args
        .first()
        .and_then(|s| s.split_once('x'))
        .and_then(|(c, r)| Some((c.parse::<u16>().ok()?, r.parse::<u16>().ok()?)))
        .unwrap_or((80, 24));
    let (cols, rows) = size;
    let mut app = Showcase::new_cells(u32::from(cols), u32::from(rows));
    if let Some(name) = args.get(1)
        && let Some(page) = Page::ALL
            .iter()
            .find(|p| p.name().eq_ignore_ascii_case(name))
    {
        app.choose_page(*page);
        App::tick(&mut app, 1000);
    }
    let mut grid = Grid::new(
        cols,
        rows,
        vec![Cell::BLANK; usize::from(cols) * usize::from(rows)],
    )
    .ok_or_else(|| io::Error::other("bad size"))?;
    app.frame_cells(&mut grid, &look(ascii, false));
    for y in 0..rows {
        let line: String = grid
            .row(y)
            .into_iter()
            .flatten()
            .filter(|c| !c.is_tail())
            .map(|c| c.ch)
            .collect();
        println!("{}", line.trim_end());
    }
    Ok(())
}
