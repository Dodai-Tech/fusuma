#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

use core::fmt::{self, Write};

use fusuma_cells::{Cell, CellKey, Decoder, Depth, Encoder, Grid, MouseKind};
use fusuma_render::Point;
use fusuma_ui::{Edit, Input, InputText, Key, Pointer, wheel_pixels};

/// Rows a wheel notch scrolls: a layout in cells is one row a line.
pub const WHEEL_ROWS: i32 = 3;

/// A serial console: frames out, input in.
#[derive(Debug)]
pub struct Console<W, S> {
    out: W,
    /// What the terminal shows now.
    shown: Grid<S>,
    encoder: Encoder,
    decoder: Decoder,
    /// The terminal is not known to match `shown`: the next frame clears it and writes everything.
    fresh: bool,
}

impl<W: Write, S: AsRef<[Cell]> + AsMut<[Cell]>> Console<W, S> {
    /// A console writing to `out` for a terminal of `depth` colours. `shown` is storage for what the terminal
    /// shows (the size of the frames to come, blank); the frames are `Grid`s of the same size.
    pub const fn new(out: W, shown: Grid<S>, depth: Depth) -> Self {
        Self {
            out,
            shown,
            encoder: Encoder::new(depth),
            decoder: Decoder::new(),
            fresh: true,
        }
    }

    /// Switches the terminal to the alternate screen with the cursor hidden and mouse reporting on.
    ///
    /// # Errors
    /// The writer's error.
    pub fn start(&mut self) -> fmt::Result {
        // Alternate screen, hide the cursor, report the mouse (clicks and drags, in the SGR form).
        self.out
            .write_str("\x1b[?1049h\x1b[?25l\x1b[?1000h\x1b[?1002h\x1b[?1006h")?;
        self.fresh = true;
        self.encoder.forget();
        Ok(())
    }

    /// Puts the terminal back as it was found.
    ///
    /// # Errors
    /// The writer's error.
    pub fn stop(&mut self) -> fmt::Result {
        self.out
            .write_str("\x1b[0m\x1b[?1006l\x1b[?1002l\x1b[?1000l\x1b[?25h\x1b[?1049l")
    }

    /// The writer, to send something else through.
    pub fn writer(&mut self) -> &mut W {
        &mut self.out
    }

    /// The terminal is another size now (or something else wrote on it): the next frame clears the screen and
    /// writes everything.
    pub fn invalidate(&mut self) {
        self.fresh = true;
        self.encoder.forget();
    }

    /// Shows `frame`, writing only the cells that differ from the last one. A frame of another size than `shown`
    /// is written whole, and `shown` is left for the next one of the right size.
    ///
    /// # Errors
    /// The writer's error.
    pub fn present<T: AsRef<[Cell]>>(&mut self, frame: &Grid<T>) -> fmt::Result {
        if self.fresh {
            self.encoder.clear(&mut self.out)?;
            self.shown.clear(Cell::BLANK);
            self.fresh = false;
        }
        self.encoder
            .write_changes(&mut self.out, &self.shown, frame)?;
        self.encoder.finish(&mut self.out)?;
        self.shown.copy_from(frame);
        Ok(())
    }

    /// Feeds a byte the terminal sent; the input it completes, if any. `wants_text` is the app's
    /// `App::wants_text`: with a text field focused the arrows and Backspace edit, otherwise they move the focus.
    pub fn feed(&mut self, byte: u8, wants_text: bool) -> Option<Input> {
        let key = self.decoder.push(byte)?;
        input_of(key, wants_text)
    }

    /// Input has paused: a lone Escape that was waiting is the Escape key.
    pub fn flush(&mut self, wants_text: bool) -> Option<Input> {
        let key = self.decoder.flush()?;
        input_of(key, wants_text)
    }
}

/// `key` as Fusuma input, or `None` for what Fusuma does not use.
#[must_use]
pub fn input_of(key: CellKey, wants_text: bool) -> Option<Input> {
    let edit = |e: Edit| wants_text.then_some(Input::Edit(e));
    match key {
        CellKey::Char(c) => {
            let mut buf = [0u8; 4];
            Some(Input::Text(InputText::new(c.encode_utf8(&mut buf))))
        }
        CellKey::Enter => Some(Input::Key(Key::Activate)),
        CellKey::Tab => Some(Input::Key(Key::Next)),
        CellKey::BackTab => Some(Input::Key(Key::Prev)),
        CellKey::Escape => Some(Input::Key(Key::Cancel)),
        CellKey::Up => Some(Input::Key(Key::Up)),
        CellKey::Down => Some(Input::Key(Key::Down)),
        CellKey::Left if wants_text => edit(Edit::Left(false)),
        CellKey::Right if wants_text => edit(Edit::Right(false)),
        CellKey::Left => Some(Input::Key(Key::Left)),
        CellKey::Right => Some(Input::Key(Key::Right)),
        CellKey::Backspace => edit(Edit::Backspace),
        CellKey::Delete => edit(Edit::Delete),
        CellKey::Home => edit(Edit::Home(false)),
        CellKey::End => edit(Edit::End(false)),
        // Alt and a letter: the word keys of a text field (Alt+b, Alt+f), as an editor knows them.
        CellKey::Alt('b') => edit(Edit::WordLeft(false)),
        CellKey::Alt('f') => edit(Edit::WordRight(false)),
        CellKey::Alt(_) => None,
        CellKey::Mouse(m) => {
            let at = Point::new(i32::from(m.x), i32::from(m.y));
            Some(Input::Pointer(match m.kind {
                MouseKind::Down => Pointer::Down(at),
                MouseKind::Up => Pointer::Up(at),
                MouseKind::Drag | MouseKind::Move => Pointer::Move(at),
                MouseKind::ScrollDown => Pointer::Wheel(at, wheel_pixels(WHEEL_ROWS, 1)),
                MouseKind::ScrollUp => Pointer::Wheel(at, wheel_pixels(-WHEEL_ROWS, 1)),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::String;
    use std::vec;
    use std::vec::Vec;

    use fusuma_cells::{Cell, Style};
    use fusuma_showcase::Showcase;
    use fusuma_ui::{App, CellTheme, deliver};

    use super::*;

    fn grid(w: u16, h: u16) -> Grid<Vec<Cell>> {
        Grid::new(w, h, vec![Cell::BLANK; usize::from(w) * usize::from(h)]).unwrap()
    }

    fn console(w: u16, h: u16) -> Console<String, Vec<Cell>> {
        Console::new(String::new(), grid(w, h), Depth::TrueColor)
    }

    fn drain(c: &mut Console<String, Vec<Cell>>) -> String {
        core::mem::take(c.writer())
    }

    #[test]
    fn start_and_stop_bring_the_terminal_up_and_back() {
        let mut c = console(4, 2);
        c.start().unwrap();
        let up = drain(&mut c);
        for want in ["\x1b[?1049h", "\x1b[?25l", "\x1b[?1000h", "\x1b[?1006h"] {
            assert!(up.contains(want), "{up:?} lacks {want:?}");
        }
        c.stop().unwrap();
        let down = drain(&mut c);
        for want in ["\x1b[0m", "\x1b[?25h", "\x1b[?1049l", "\x1b[?1000l"] {
            assert!(down.contains(want), "{down:?} lacks {want:?}");
        }
    }

    #[test]
    fn the_first_frame_clears_and_writes_all_and_the_next_only_what_changed() {
        let mut c = console(6, 2);
        let mut f = grid(6, 2);
        f.put_str(0, 0, "hello", Style::PLAIN);
        c.present(&f).unwrap();
        let first = drain(&mut c);
        assert!(first.starts_with("\x1b[0m\x1b[2J\x1b[H"), "{first:?}");
        assert!(first.contains("hello"));
        // The same frame again: nothing but the style reset the encoder keeps tidy.
        c.present(&f).unwrap();
        assert!(!drain(&mut c).contains("hello"));
        f.put_str(0, 1, "x", Style::PLAIN);
        c.present(&f).unwrap();
        let third = drain(&mut c);
        assert!(third.contains('x') && !third.contains("hello"), "{third:?}");
        // A resize or something else on the line: everything again.
        c.invalidate();
        c.present(&f).unwrap();
        let again = drain(&mut c);
        assert!(again.contains("\x1b[2J") && again.contains("hello"));
    }

    fn feed(c: &mut Console<String, Vec<Cell>>, bytes: &[u8], text: bool) -> Vec<Input> {
        let mut v: Vec<Input> = bytes.iter().filter_map(|&b| c.feed(b, text)).collect();
        v.extend(c.flush(text));
        v
    }

    #[test]
    fn bytes_become_input() {
        let mut c = console(2, 2);
        assert_eq!(
            feed(&mut c, b"\t\x1b[Z\r\x1b", false),
            [
                Input::Key(Key::Next),
                Input::Key(Key::Prev),
                Input::Key(Key::Activate),
                Input::Key(Key::Cancel),
            ]
        );
        assert_eq!(feed(&mut c, b"\x1b[D", false), [Input::Key(Key::Left)]);
        assert_eq!(
            feed(&mut c, b"\x1b[D", true),
            [Input::Edit(Edit::Left(false))]
        );
        assert_eq!(
            feed(&mut c, b"\x7f", false),
            [],
            "no field, nothing to edit"
        );
        assert_eq!(feed(&mut c, b"\x7f", true), [Input::Edit(Edit::Backspace)]);
        assert_eq!(
            feed(&mut c, b"\x1bb\x1bf\x1bx", true),
            [
                Input::Edit(Edit::WordLeft(false)),
                Input::Edit(Edit::WordRight(false)),
            ]
        );
        assert!(matches!(
            feed(&mut c, "é".as_bytes(), true)[..],
            [Input::Text(_)]
        ));
        let p = |x, y| fusuma_render::Point::new(x, y);
        assert_eq!(
            feed(&mut c, b"\x1b[<0;5;3M\x1b[<0;5;3m", false),
            [
                Input::Pointer(Pointer::Down(p(4, 2))),
                Input::Pointer(Pointer::Up(p(4, 2))),
            ]
        );
        assert_eq!(
            feed(&mut c, b"\x1b[<65;1;1M", false),
            [Input::Pointer(Pointer::Wheel(p(0, 0), WHEEL_ROWS))]
        );
    }

    /// The whole loop over a console: the showcase draws, a click comes in as bytes, the next frame shows it.
    #[test]
    fn the_showcase_runs_over_a_console() {
        let (cols, rows) = (100u16, 30u16);
        let mut app = Showcase::new_cells(u32::from(cols), u32::from(rows));
        app.choose_page(fusuma_showcase::Page::Buttons);
        let mut c = console(cols, rows);
        c.start().unwrap();
        let frame = |app: &mut Showcase, c: &mut Console<String, Vec<Cell>>| {
            App::tick(app, 16);
            let mut g = grid(cols, rows);
            app.frame_cells(&mut g, &CellTheme::UNICODE);
            c.present(&g).unwrap();
            drain(c)
        };
        let first = frame(&mut app, &mut c);
        assert!(first.contains("Buttons") && first.contains("Clicked 0 times"));
        // Click the button: its position is in the layout, the terminal reports 1-based columns and rows.
        let id = app.widgets().click_me.unwrap();
        let r = app.rect_of(id).unwrap();
        let (x, y) = (r.x + (r.w / 2) as i32 + 1, r.y + 1);
        let report = std::format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m");
        for input in feed(&mut c, report.as_bytes(), false) {
            deliver(&mut app, input);
        }
        let second = frame(&mut app, &mut c);
        assert!(second.contains("1 times"), "{second:?}");
        assert!(
            !second.contains("Buttons"),
            "only what changed is written: {} bytes",
            second.len()
        );
        assert!(
            second.len() * 4 < first.len(),
            "{} against {}",
            second.len(),
            first.len()
        );
    }
}
