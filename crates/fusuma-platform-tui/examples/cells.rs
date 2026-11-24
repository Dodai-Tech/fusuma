//! A hand-drawn screen in the terminal: a frame, a few widget looks, the last input as Fusuma sees it. Quit with `q`
//! or Escape.
//!
//! The widgets are drawn by hand here, to show the cells, the colours and the diffed output; drawing them from a
//! Fusuma widget tree is the toolkit's job once it can paint into cells (see the roadmap).

use std::io;

use crossterm::event::{self, Event, KeyCode};
use fusuma_platform_tui::cells::{Attrs, Cell, Color, Style};
use fusuma_platform_tui::{Frame, Terminal, map_event};

fn draw(f: &mut Frame, last: &str, events: u32) {
    let (w, h) = (f.width(), f.height());
    let border = Style::PLAIN.fg(Color::Rgb(90, 120, 160));
    let title = Style::PLAIN.attrs(Attrs::BOLD);
    let accent = Style::PLAIN.fg(Color::Rgb(24, 162, 165)).attrs(Attrs::BOLD);
    let muted = Style::PLAIN.attrs(Attrs::DIM);
    if w < 2 || h < 2 {
        return;
    }
    // A rounded frame round the screen.
    for x in 1..w - 1 {
        f.set(x, 0, Cell::new('─', border));
        f.set(x, h - 1, Cell::new('─', border));
    }
    for y in 1..h - 1 {
        f.set(0, y, Cell::new('│', border));
        f.set(w - 1, y, Cell::new('│', border));
    }
    for (x, y, c) in [
        (0, 0, '╭'),
        (w - 1, 0, '╮'),
        (0, h - 1, '╰'),
        (w - 1, h - 1, '╯'),
    ] {
        f.set(x, y, Cell::new(c, border));
    }
    f.put_str(2, 0, " Fusuma cells ", title);
    f.put_str(2, 2, "[x] Notifications", Style::PLAIN);
    f.put_str(2, 3, "[ ] Location", Style::PLAIN);
    f.put_str(2, 5, "( ) Small  (•) Medium  ( ) Large", Style::PLAIN);
    f.put_str(2, 7, "[███████░░░] 70%", accent);
    f.put_str(2, 9, "[ OK ]", Style::PLAIN.attrs(Attrs::REVERSE));
    f.put_str(9, 9, "[ Cancel ]", Style::PLAIN);
    f.put_str(2, 11, &format!("last input ({events}): {last}"), muted);
    f.put_str(2, h.saturating_sub(2), "q or Escape quits", muted);
}

fn main() -> io::Result<()> {
    let mut term = Terminal::open()?;
    let (mut last, mut events) = (String::from("none yet"), 0u32);
    loop {
        let mut frame = term.frame();
        draw(&mut frame, &last, events);
        term.present(&frame)?;
        let e = event::read()?;
        if let Event::Key(k) = &e
            && matches!(k.code, KeyCode::Char('q') | KeyCode::Esc)
        {
            return Ok(());
        }
        if let Event::Resize(w, h) = e {
            term.resize(w, h);
        }
        if let Some(input) = map_event(&e, false) {
            events += 1;
            last = format!("{input:?}");
        }
    }
}
