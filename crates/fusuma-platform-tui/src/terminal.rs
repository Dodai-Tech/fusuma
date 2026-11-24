//! The terminal: brought up, written to a frame at a time, and put back.

use std::io::{self, Write};

use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
use crossterm::{cursor, execute, terminal};
use fusuma_cells::{Cell, Depth, Encoder, Grid};

/// A frame of cells the size of the terminal.
pub type Frame = Grid<Vec<Cell>>;

/// A frame of `width x height` blank cells.
#[must_use]
pub fn blank(width: u16, height: u16) -> Frame {
    let cells = vec![Cell::BLANK; usize::from(width) * usize::from(height)];
    Grid::new(width, height, cells).expect("the storage is made for the size")
}

/// The terminal, in raw mode on the alternate screen with the cursor hidden and the mouse reported, for as long as
/// this lives. Dropping it (a panic included) puts the terminal back.
#[derive(Debug)]
pub struct Terminal {
    out: io::Stdout,
    /// What the screen shows now.
    shown: Frame,
    encoder: Encoder,
    /// The bytes of a frame, written at once.
    buf: String,
    /// The screen is not known to match `shown`: the next frame clears it and draws everything.
    fresh: bool,
}

impl Terminal {
    /// Takes the terminal over, with the colour depth the environment describes ([`crate::detect_depth`]).
    ///
    /// # Errors
    /// When the terminal cannot be put in raw mode or written to (output is not a terminal).
    pub fn open() -> io::Result<Self> {
        Self::with_depth(crate::detect_depth())
    }

    /// Takes the terminal over, writing colours for `depth`.
    ///
    /// # Errors
    /// When the terminal cannot be put in raw mode or written to.
    pub fn with_depth(depth: Depth) -> io::Result<Self> {
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        if let Err(e) = execute!(
            out,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            EnableMouseCapture
        ) {
            let _ = terminal::disable_raw_mode();
            return Err(e);
        }
        let (w, h) = terminal::size()?;
        Ok(Self {
            out,
            shown: blank(w, h),
            encoder: Encoder::new(depth),
            buf: String::new(),
            fresh: true,
        })
    }

    /// Columns and rows.
    #[must_use]
    pub const fn size(&self) -> (u16, u16) {
        (self.shown.width(), self.shown.height())
    }

    /// The colour depth it writes for.
    #[must_use]
    pub const fn depth(&self) -> Depth {
        self.encoder.depth()
    }

    /// A blank frame of the terminal's size, to draw the next frame into.
    #[must_use]
    pub fn frame(&self) -> Frame {
        let (w, h) = self.size();
        blank(w, h)
    }

    /// The terminal is `width x height` now (from a resize event): the next frame repaints everything.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.shown = blank(width, height);
        self.fresh = true;
    }

    /// Shows `frame`, writing only the cells that differ from the last one. A frame of another size than the last
    /// is drawn whole.
    ///
    /// # Errors
    /// When the terminal cannot be written to.
    pub fn present(&mut self, frame: &Frame) -> io::Result<()> {
        self.buf.clear();
        let to_io = |_| io::Error::other("formatting a frame failed");
        if self.fresh || (frame.width(), frame.height()) != self.size() {
            self.shown = blank(frame.width(), frame.height());
            self.encoder.clear(&mut self.buf).map_err(to_io)?;
            self.fresh = false;
        }
        self.encoder
            .write_changes(&mut self.buf, &self.shown, frame)
            .map_err(to_io)?;
        self.encoder.finish(&mut self.buf).map_err(to_io)?;
        self.out.write_all(self.buf.as_bytes())?;
        self.out.flush()?;
        self.shown.copy_from(frame);
        Ok(())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = execute!(
            self.out,
            DisableMouseCapture,
            cursor::Show,
            terminal::LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_frame_has_the_size_and_only_blanks() {
        let f = blank(5, 2);
        assert_eq!((f.width(), f.height()), (5, 2));
        assert!(f.cells().iter().all(|c| *c == Cell::BLANK));
        assert_eq!(blank(0, 0).cells().len(), 0);
    }
}
