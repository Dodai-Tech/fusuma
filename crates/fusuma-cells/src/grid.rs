//! A grid of cells over any storage.

use crate::cell::{Cell, Style};

/// `width x height` cells, row by row, over storage `S`: a `[Cell; N]` (no allocator) or a `Vec<Cell>`. Reading or
/// writing outside the grid does nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid<S> {
    width: u16,
    height: u16,
    cells: S,
}

impl<S: AsRef<[Cell]>> Grid<S> {
    /// A grid over `cells`, which must hold exactly `width * height` of them; `None` otherwise.
    #[must_use]
    pub fn new(width: u16, height: u16, cells: S) -> Option<Self> {
        (cells.as_ref().len() == usize::from(width) * usize::from(height)).then_some(Self {
            width,
            height,
            cells,
        })
    }

    /// Columns.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Rows.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// Every cell, row by row.
    #[must_use]
    pub fn cells(&self) -> &[Cell] {
        self.cells.as_ref()
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        (x < self.width && y < self.height)
            .then(|| usize::from(y) * usize::from(self.width) + usize::from(x))
    }

    /// The cell at column `x`, row `y`.
    #[must_use]
    pub fn get(&self, x: u16, y: u16) -> Option<Cell> {
        self.index(x, y).map(|i| self.cells.as_ref()[i])
    }

    /// Row `y`.
    #[must_use]
    pub fn row(&self, y: u16) -> Option<&[Cell]> {
        let start = self.index(0, y)?;
        Some(&self.cells.as_ref()[start..start + usize::from(self.width)])
    }

    /// Gives the storage back.
    pub fn into_cells(self) -> S {
        self.cells
    }
}

impl<S: AsRef<[Cell]> + AsMut<[Cell]>> Grid<S> {
    /// Puts `cell` at column `x`, row `y`; false outside the grid.
    pub fn set(&mut self, x: u16, y: u16, cell: Cell) -> bool {
        let Some(i) = self.index(x, y) else {
            return false;
        };
        self.cells.as_mut()[i] = cell;
        true
    }

    /// Fills the whole grid with `cell`.
    pub fn clear(&mut self, cell: Cell) {
        self.cells.as_mut().fill(cell);
    }

    /// Fills the `w x h` cells from column `x`, row `y`, cut to the grid.
    pub fn fill(&mut self, x: u16, y: u16, w: u16, h: u16, cell: Cell) {
        let x1 = x.saturating_add(w).min(self.width);
        let y1 = y.saturating_add(h).min(self.height);
        let width = usize::from(self.width);
        let cells = self.cells.as_mut();
        for row in y..y1 {
            let at = usize::from(row) * width;
            if x < x1 {
                cells[at + usize::from(x)..at + usize::from(x1)].fill(cell);
            }
        }
    }

    /// Writes `text` from column `x` of row `y` in `style`, one character a cell, cut at the end of the row. Returns
    /// how many cells were written.
    pub fn put_str(&mut self, x: u16, y: u16, text: &str, style: Style) -> u16 {
        let mut col = x;
        for ch in text.chars() {
            if !self.set(col, y, Cell::new(ch, style)) {
                break;
            }
            col += 1;
        }
        col - x
    }

    /// Copies `other` into this grid; false (and nothing copied) when the sizes differ.
    pub fn copy_from<T: AsRef<[Cell]>>(&mut self, other: &Grid<T>) -> bool {
        if (self.width, self.height) != (other.width, other.height) {
            return false;
        }
        self.cells.as_mut().copy_from_slice(other.cells.as_ref());
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::{Attrs, Color};

    fn grid() -> Grid<[Cell; 12]> {
        Grid::new(4, 3, [Cell::BLANK; 12]).unwrap()
    }

    fn text<S: AsRef<[Cell]>>(g: &Grid<S>, y: u16) -> String {
        g.row(y).unwrap().iter().map(|c| c.ch).collect()
    }

    #[test]
    fn the_storage_must_fit_the_size() {
        assert!(Grid::new(4, 3, [Cell::BLANK; 11]).is_none());
        assert!(Grid::new(0, 0, [Cell::BLANK; 0]).is_some());
        let g = grid();
        assert_eq!((g.width(), g.height(), g.cells().len()), (4, 3, 12));
    }

    #[test]
    fn text_is_cut_at_the_end_of_its_row_and_outside_is_ignored() {
        let mut g = grid();
        let bold = Style::PLAIN.attrs(Attrs::BOLD);
        assert_eq!(g.put_str(1, 0, "héllo", bold), 3);
        assert_eq!(text(&g, 0), " hél");
        assert_eq!(text(&g, 1), "    ", "nothing wrapped onto the next row");
        assert_eq!(g.get(1, 0).unwrap().style, bold);
        assert_eq!(g.put_str(9, 1, "x", bold), 0);
        assert_eq!(g.put_str(0, 9, "x", bold), 0);
        assert!(!g.set(4, 0, Cell::BLANK));
        assert_eq!((g.get(4, 0), g.get(0, 3), g.row(3)), (None, None, None));
    }

    #[test]
    fn fills_are_cut_to_the_grid() {
        let mut g = grid();
        let x = Cell::new('x', Style::PLAIN.bg(Color::Indexed(1)));
        g.fill(2, 1, 9, 9, x);
        assert_eq!(
            [text(&g, 0), text(&g, 1), text(&g, 2)],
            ["    ", "  xx", "  xx"]
        );
        g.fill(u16::MAX, u16::MAX, u16::MAX, u16::MAX, x);
        g.fill(0, 0, 0, 3, Cell::new('y', Style::PLAIN));
        assert_eq!(text(&g, 0), "    ", "a fill of no width changes nothing");
        g.clear(Cell::new('.', Style::PLAIN));
        assert_eq!(text(&g, 2), "....");
    }

    #[test]
    fn a_copy_needs_the_same_size() {
        let mut a = grid();
        let mut b = grid();
        b.put_str(0, 0, "abcd", Style::PLAIN);
        assert!(a.copy_from(&b));
        assert_eq!(a, b);
        let other = Grid::new(2, 2, vec![Cell::BLANK; 4]).unwrap();
        assert!(!a.copy_from(&other));
        assert_eq!(text(&a, 0), "abcd");
        assert_eq!(a.into_cells()[0].ch, 'a');
    }
}
