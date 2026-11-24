//! What changed between two frames: runs of cells, row by row.

use crate::cell::Cell;
use crate::grid::Grid;

/// Unchanged cells between two changed ones that are sent anyway, so that one run covers both: moving the cursor
/// costs about as many bytes as this.
pub const MERGE_GAP: u16 = 4;

/// A run of cells in one row, from column `x`, `len` long.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Run {
    /// Column of the first cell.
    pub x: u16,
    /// Row.
    pub y: u16,
    /// How many cells.
    pub len: u16,
}

/// The runs of [`changes`], in reading order.
#[derive(Clone, Debug)]
pub struct Changes<'a> {
    /// `None` when the sizes differ: then everything has changed.
    old: Option<&'a [Cell]>,
    new: &'a [Cell],
    width: u16,
    height: u16,
    x: u16,
    y: u16,
}

/// The cells of `new` that differ from `old`, as runs. When the sizes differ every row of `new` is one run.
pub fn changes<'a, A: AsRef<[Cell]>, B: AsRef<[Cell]>>(
    old: &'a Grid<A>,
    new: &'a Grid<B>,
) -> Changes<'a> {
    let same = (old.width(), old.height()) == (new.width(), new.height());
    Changes {
        old: same.then(|| old.cells()),
        new: new.cells(),
        width: new.width(),
        height: new.height(),
        x: 0,
        y: 0,
    }
}

impl Changes<'_> {
    fn differs(&self, x: u16, y: u16) -> bool {
        let i = usize::from(y) * usize::from(self.width) + usize::from(x);
        self.old.is_none_or(|old| old[i] != self.new[i])
    }
}

impl Iterator for Changes<'_> {
    type Item = Run;

    fn next(&mut self) -> Option<Run> {
        while self.y < self.height {
            let y = self.y;
            // The first changed cell from here in this row.
            let start = (self.x..self.width).find(|&x| self.differs(x, y));
            let Some(start) = start else {
                (self.x, self.y) = (0, y + 1);
                continue;
            };
            // Extend over changed cells, and over short gaps that end in another change.
            let mut end = start + 1;
            let mut x = end;
            while x < self.width {
                if self.differs(x, y) {
                    end = x + 1;
                } else if x - end >= MERGE_GAP {
                    break;
                }
                x += 1;
            }
            self.x = end;
            return Some(Run {
                x: start,
                y,
                len: end - start,
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Style;

    fn grid(rows: &[&str]) -> Grid<Vec<Cell>> {
        let w = rows[0].chars().count() as u16;
        let cells = rows
            .iter()
            .flat_map(|r| r.chars().map(|c| Cell::new(c, Style::PLAIN)))
            .collect();
        Grid::new(w, rows.len() as u16, cells).unwrap()
    }

    fn runs(a: &[&str], b: &[&str]) -> Vec<(u16, u16, u16)> {
        changes(&grid(a), &grid(b))
            .map(|r| (r.x, r.y, r.len))
            .collect()
    }

    #[test]
    fn the_same_frame_has_no_changes() {
        assert_eq!(runs(&["abc", "def"], &["abc", "def"]), []);
    }

    #[test]
    fn changes_are_runs_in_reading_order() {
        assert_eq!(
            runs(
                &["abcdefghijkl", "abcdefghijkl"],
                &["aXcdefghijkY", "abcdeZZhijkl"]
            ),
            [(1, 0, 1), (11, 0, 1), (5, 1, 2)]
        );
    }

    #[test]
    fn a_short_gap_is_merged_and_a_long_one_is_not() {
        // Gap of 3 between X and Y: one run. Gap of 5: two.
        assert_eq!(runs(&["............"], &[".X...Y......"]), [(1, 0, 5)]);
        assert_eq!(
            runs(&["............"], &[".X.....Y...."]),
            [(1, 0, 1), (7, 0, 1)]
        );
    }

    #[test]
    fn a_change_of_style_is_a_change() {
        let a = grid(&["ab"]);
        let mut b = grid(&["ab"]);
        b.set(
            1,
            0,
            Cell::new('b', Style::PLAIN.attrs(crate::cell::Attrs::BOLD)),
        );
        assert_eq!(
            changes(&a, &b).collect::<Vec<_>>(),
            [Run { x: 1, y: 0, len: 1 }]
        );
    }

    #[test]
    fn a_new_size_redraws_every_row() {
        assert_eq!(runs(&["ab"], &["abc", "def"]), [(0, 0, 3), (0, 1, 3)]);
        assert_eq!(
            changes(&Grid::new(0, 0, Vec::new()).unwrap(), &grid(&["x"])).count(),
            1
        );
    }

    #[test]
    fn every_changed_cell_is_in_exactly_one_run_and_runs_do_not_overlap() {
        // A deterministic pseudo-random pair of frames.
        let mut seed = 0x2545_f491_u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..200 {
            let (w, h) = (1 + (next() % 30) as u16, 1 + (next() % 6) as u16);
            let n = usize::from(w) * usize::from(h);
            let a: Vec<Cell> = (0..n).map(|_| Cell::new('.', Style::PLAIN)).collect();
            let mut b = a.clone();
            for c in &mut b {
                if next() % 4 == 0 {
                    c.ch = 'x';
                }
            }
            let (ga, gb) = (Grid::new(w, h, a).unwrap(), Grid::new(w, h, b).unwrap());
            let mut covered = vec![0u8; n];
            for r in changes(&ga, &gb) {
                assert!(r.len > 0 && r.x + r.len <= w && r.y < h, "{r:?}");
                for x in r.x..r.x + r.len {
                    covered[usize::from(r.y) * usize::from(w) + usize::from(x)] += 1;
                }
            }
            for (i, (old, new)) in ga.cells().iter().zip(gb.cells()).enumerate() {
                assert!(covered[i] <= 1, "cell {i} in two runs");
                if old != new {
                    assert_eq!(covered[i], 1, "changed cell {i} is in a run");
                }
            }
        }
    }
}
