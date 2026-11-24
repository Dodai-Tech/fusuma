//! Cells as ANSI escape sequences, written to any `core::fmt::Write`.

use core::fmt::{self, Write};

use crate::cell::{Attrs, Cell, Color, Depth, Style};
use crate::diff::{Run, changes};
use crate::grid::Grid;

/// Writes cells to a terminal as ANSI escape sequences, remembering where the cursor is and which style is on so it
/// sends only what changes. Colours are shown at the output's [`Depth`].
#[derive(Clone, Copy, Debug)]
pub struct Encoder {
    depth: Depth,
    /// The style the terminal has now; `None` when unknown.
    style: Option<Style>,
    /// Where the cursor is now; `None` when unknown.
    cursor: Option<(u16, u16)>,
}

impl Encoder {
    /// An encoder for an output of `depth`, knowing nothing of the terminal's state.
    #[must_use]
    pub const fn new(depth: Depth) -> Self {
        Self {
            depth,
            style: None,
            cursor: None,
        }
    }

    /// The colour depth it writes for.
    #[must_use]
    pub const fn depth(&self) -> Depth {
        self.depth
    }

    /// Forgets what it knew of the terminal (after something else wrote to it), so the next write sets everything.
    pub fn forget(&mut self) {
        self.style = None;
        self.cursor = None;
    }

    /// Clears the screen to the plain style and puts the cursor home.
    ///
    /// # Errors
    /// The writer's error.
    pub fn clear<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        out.write_str("\x1b[0m\x1b[2J\x1b[H")?;
        self.style = Some(Style::PLAIN);
        self.cursor = Some((0, 0));
        Ok(())
    }

    /// Writes the cells of `run` from `grid`.
    ///
    /// # Errors
    /// The writer's error.
    pub fn write_run<W: Write, S: AsRef<[Cell]>>(
        &mut self,
        out: &mut W,
        grid: &Grid<S>,
        run: Run,
    ) -> fmt::Result {
        let Some(row) = grid.row(run.y) else {
            return Ok(());
        };
        let end = usize::from(run.x.saturating_add(run.len)).min(row.len());
        let start = usize::from(run.x).min(end);
        if start == end {
            return Ok(());
        }
        if self.cursor != Some((run.x, run.y)) {
            write!(
                out,
                "\x1b[{};{}H",
                u32::from(run.y) + 1,
                u32::from(run.x) + 1
            )?;
        }
        for cell in &row[start..end] {
            if self.style != Some(cell.style) {
                self.write_style(out, cell.style)?;
                self.style = Some(cell.style);
            }
            // A control character would move the cursor or worse: it is shown as a space.
            out.write_char(if cell.ch.is_control() { ' ' } else { cell.ch })?;
        }
        self.cursor = Some((run.x.saturating_add((end - start) as u16), run.y));
        Ok(())
    }

    /// Writes what differs between `old` and `new` (everything when the sizes differ).
    ///
    /// # Errors
    /// The writer's error.
    pub fn write_changes<W: Write, A: AsRef<[Cell]>, B: AsRef<[Cell]>>(
        &mut self,
        out: &mut W,
        old: &Grid<A>,
        new: &Grid<B>,
    ) -> fmt::Result {
        for run in changes(old, new) {
            self.write_run(out, new, run)?;
        }
        Ok(())
    }

    /// Turns the style back to the terminal's plain one, to leave it as it was found.
    ///
    /// # Errors
    /// The writer's error.
    pub fn finish<W: Write>(&mut self, out: &mut W) -> fmt::Result {
        if self.style != Some(Style::PLAIN) {
            out.write_str("\x1b[0m")?;
            self.style = Some(Style::PLAIN);
        }
        Ok(())
    }

    /// One SGR sequence that sets the whole style, starting from a reset.
    fn write_style<W: Write>(&self, out: &mut W, style: Style) -> fmt::Result {
        out.write_str("\x1b[0")?;
        for (attr, code) in [
            (Attrs::BOLD, "1"),
            (Attrs::DIM, "2"),
            (Attrs::ITALIC, "3"),
            (Attrs::UNDERLINE, "4"),
            (Attrs::REVERSE, "7"),
        ] {
            if style.attrs.has(attr) {
                write!(out, ";{code}")?;
            }
        }
        write_color(out, style.fg.at(self.depth), false)?;
        write_color(out, style.bg.at(self.depth), true)?;
        out.write_char('m')
    }
}

/// `;` and the SGR parameters of a foreground (`bg` false) or background colour; nothing for the default.
fn write_color<W: Write>(out: &mut W, c: Color, bg: bool) -> fmt::Result {
    let base = if bg { 10 } else { 0 };
    match c {
        Color::Default => Ok(()),
        Color::Indexed(i @ 0..=7) => write!(out, ";{}", 30 + base + u32::from(i)),
        Color::Indexed(i @ 8..=15) => write!(out, ";{}", 90 + base + u32::from(i) - 8),
        Color::Indexed(i) => write!(out, ";{};5;{i}", 38 + base),
        Color::Rgb(r, g, b) => write!(out, ";{};2;{r};{g};{b}", 38 + base),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str], style: Style) -> Grid<Vec<Cell>> {
        let w = rows[0].chars().count() as u16;
        let cells = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| Cell::new(c, style)))
            .collect();
        Grid::new(w, rows.len() as u16, cells).unwrap()
    }

    #[test]
    fn a_run_moves_the_cursor_sets_the_style_once_and_writes_the_cells() {
        let g = grid(
            &["hello"],
            Style::PLAIN.attrs(Attrs::BOLD).fg(Color::Indexed(2)),
        );
        let mut e = Encoder::new(Depth::TrueColor);
        let mut s = String::new();
        e.write_run(&mut s, &g, Run { x: 1, y: 0, len: 3 }).unwrap();
        assert_eq!(s, "\x1b[1;2H\x1b[0;1;32mell");
        // The cursor and the style are where the last write left them: nothing is repeated.
        let mut s = String::new();
        e.write_run(&mut s, &g, Run { x: 4, y: 0, len: 1 }).unwrap();
        assert_eq!(s, "o");
    }

    #[test]
    fn colours_are_written_at_the_output_depth() {
        let style = Style::PLAIN
            .fg(Color::Rgb(255, 0, 0))
            .bg(Color::Indexed(12));
        let g = grid(&["x"], style);
        let write = |depth| {
            let mut s = String::new();
            Encoder::new(depth)
                .write_run(&mut s, &g, Run { x: 0, y: 0, len: 1 })
                .unwrap();
            s
        };
        assert_eq!(
            write(Depth::TrueColor),
            "\x1b[1;1H\x1b[0;38;2;255;0;0;104mx"
        );
        assert_eq!(write(Depth::Ansi256), "\x1b[1;1H\x1b[0;38;5;196;104mx");
        assert_eq!(write(Depth::Ansi16), "\x1b[1;1H\x1b[0;91;104mx");
        assert_eq!(write(Depth::Mono), "\x1b[1;1H\x1b[0mx");
    }

    #[test]
    fn only_the_changes_are_sent_and_control_characters_never() {
        let old = grid(&["abc", "def"], Style::PLAIN);
        let mut new = old.clone();
        new.set(2, 1, Cell::new('\n', Style::PLAIN));
        let mut e = Encoder::new(Depth::Ansi16);
        let mut s = String::new();
        e.clear(&mut s).unwrap();
        e.write_changes(&mut s, &old, &new).unwrap();
        e.finish(&mut s).unwrap();
        assert_eq!(
            s, "\x1b[0m\x1b[2J\x1b[H\x1b[2;3H ",
            "plain style kept, no reset at the end"
        );
        let mut s = String::new();
        e.write_changes(&mut s, &new, &new).unwrap();
        assert_eq!(s, "", "nothing changed, nothing sent");
    }

    #[test]
    fn forgetting_resends_the_cursor_and_style_and_finish_resets() {
        let g = grid(&["ab"], Style::PLAIN.attrs(Attrs::REVERSE));
        let mut e = Encoder::new(Depth::Mono);
        let run = Run { x: 0, y: 0, len: 1 };
        let mut s = String::new();
        e.write_run(&mut s, &g, run).unwrap();
        e.forget();
        e.write_run(&mut s, &g, run).unwrap();
        e.finish(&mut s).unwrap();
        assert_eq!(s, "\x1b[1;1H\x1b[0;7ma\x1b[1;1H\x1b[0;7ma\x1b[0m");
        assert_eq!(e.depth(), Depth::Mono);
    }

    #[test]
    fn runs_outside_the_grid_write_nothing() {
        let g = grid(&["ab"], Style::PLAIN);
        let mut e = Encoder::new(Depth::TrueColor);
        let mut s = String::new();
        for run in [
            Run { x: 0, y: 5, len: 1 },
            Run { x: 9, y: 0, len: 3 },
            Run { x: 1, y: 0, len: 0 },
            Run {
                x: u16::MAX,
                y: 0,
                len: u16::MAX,
            },
        ] {
            e.write_run(&mut s, &g, run).unwrap();
        }
        assert_eq!(s, "");
    }
}
