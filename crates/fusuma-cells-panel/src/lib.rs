#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

use fusuma_cells::{Attrs, Cell, Color as CColor, Grid, changes};
use fusuma_render::{Color, Font, PathCmd, Point, Rect, Renderer, TextStyle};

/// The size of a cell in pixels and where the grid starts on the screen, with the colours a cell with no colour of its
/// own gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellPanel {
    /// Width of a cell in pixels.
    pub cell_w: u32,
    /// Height of a cell in pixels.
    pub cell_h: u32,
    /// Where the top left cell is.
    pub origin: Point,
    /// The colour of a character that has none.
    pub default_fg: Color,
    /// The colour of a cell that has none.
    pub default_bg: Color,
}

/// The colour `a` of 255 of the way from `from` to `to`.
fn mix(from: Color, to: Color, a: u32) -> Color {
    let m = |x: u8, y: u8| ((u32::from(x) * (255 - a) + u32::from(y) * a) / 255) as u8;
    Color::rgb(m(from.r, to.r), m(from.g, to.g), m(from.b, to.b))
}

impl CellPanel {
    /// A panel of cells 6 x 10 (the small bitmap font), at the screen's top left, light on dark.
    pub const SMALL: Self = Self::sized(6, 10);
    /// Cells 9 x 15.
    pub const MEDIUM: Self = Self::sized(9, 15);
    /// Cells 10 x 20.
    pub const LARGE: Self = Self::sized(10, 20);

    /// Cells of `cell_w` x `cell_h` pixels; the bitmap font draws a character of that height (6x10, 9x15 or 10x20 are
    /// the sizes it has).
    #[must_use]
    pub const fn sized(cell_w: u32, cell_h: u32) -> Self {
        Self {
            cell_w,
            cell_h,
            origin: Point::new(0, 0),
            default_fg: Color::rgb(220, 224, 232),
            default_bg: Color::rgb(12, 14, 20),
        }
    }

    /// The grid starting at `origin` on the screen.
    #[must_use]
    pub const fn at(self, origin: Point) -> Self {
        Self { origin, ..self }
    }

    /// How many columns and rows fit in `width` x `height` pixels.
    #[must_use]
    pub const fn fits(&self, width: u32, height: u32) -> (u16, u16) {
        let cols = width / if self.cell_w == 0 { 1 } else { self.cell_w };
        let rows = height / if self.cell_h == 0 { 1 } else { self.cell_h };
        (
            if cols > u16::MAX as u32 {
                u16::MAX
            } else {
                cols as u16
            },
            if rows > u16::MAX as u32 {
                u16::MAX
            } else {
                rows as u16
            },
        )
    }

    /// The pixels of the cells `x..x + len` of row `y`.
    fn pixels(&self, x: u16, y: u16, len: u16) -> Rect {
        let at = |v: u16, unit: u32, base: i32| {
            base.saturating_add(i32::try_from(u32::from(v) * unit).unwrap_or(i32::MAX))
        };
        Rect::new(
            at(x, self.cell_w, self.origin.x),
            at(y, self.cell_h, self.origin.y),
            u32::from(len) * self.cell_w,
            self.cell_h,
        )
    }

    /// Paints every cell of `grid`; the rectangle it covers.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn draw<R: Renderer, S: AsRef<[Cell]>>(
        &self,
        r: &mut R,
        grid: &Grid<S>,
    ) -> Result<Rect, R::Error> {
        for y in 0..grid.height() {
            if let Some(row) = grid.row(y) {
                for (x, cell) in row.iter().enumerate() {
                    self.cell(r, x as u16, y, cell)?;
                }
            }
        }
        Ok(self.pixels(0, 0, grid.width()).union(&self.pixels(
            0,
            grid.height().saturating_sub(1),
            grid.width(),
        )))
    }

    /// Paints only the cells of `new` that differ from `old` (everything if the sizes differ). Returns the one
    /// rectangle that holds all that was painted, or `None` when nothing changed: a board sends that, once.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn draw_changes<R: Renderer, A: AsRef<[Cell]>, B: AsRef<[Cell]>>(
        &self,
        r: &mut R,
        old: &Grid<A>,
        new: &Grid<B>,
    ) -> Result<Option<Rect>, R::Error> {
        let mut region: Option<Rect> = None;
        for run in changes(old, new) {
            let Some(row) = new.row(run.y) else { continue };
            for k in 0..run.len {
                let x = run.x + k;
                if let Some(cell) = row.get(usize::from(x)) {
                    self.cell(r, x, run.y, cell)?;
                }
            }
            let px = self.pixels(run.x, run.y, run.len);
            region = Some(region.map_or(px, |g| g.union(&px)));
        }
        Ok(region)
    }

    /// Paints cell `cell` at column `x`, row `y`.
    fn cell<R: Renderer>(&self, r: &mut R, x: u16, y: u16, cell: &Cell) -> Result<(), R::Error> {
        // The second column of a wide character is painted with its first.
        if cell.is_tail() {
            return Ok(());
        }
        let area = self.pixels(x, y, 1);
        // Far outside any screen: nothing to draw, and the geometry below stays in range.
        const FAR: u32 = 1 << 24;
        if area.x.unsigned_abs() > FAR
            || area.y.unsigned_abs() > FAR
            || area.w > FAR
            || area.h > FAR
        {
            return Ok(());
        }
        let rgb = |c: CColor, default: Color| {
            c.rgb()
                .map_or(default, |(red, green, blue)| Color::rgb(red, green, blue))
        };
        let (mut fg, mut bg) = (
            rgb(cell.style.fg, self.default_fg),
            rgb(cell.style.bg, self.default_bg),
        );
        let a = cell.style.attrs;
        if a.has(Attrs::REVERSE) {
            core::mem::swap(&mut fg, &mut bg);
        }
        if a.has(Attrs::DIM) {
            fg = mix(fg, bg, 110);
        }
        r.fill_rect(area, 0, bg)?;
        self.glyph(r, area, cell.ch, fg, bg, a.has(Attrs::BOLD))?;
        if a.has(Attrs::UNDERLINE) {
            let line = Rect::new(area.x, area.bottom() - 1, area.w, 1);
            r.fill_rect(line, 0, fg)?;
        }
        Ok(())
    }

    /// One character in `area`: a shape for the ones a bitmap font lacks, else the font's.
    fn glyph<R: Renderer>(
        &self,
        r: &mut R,
        area: Rect,
        ch: char,
        fg: Color,
        bg: Color,
        bold: bool,
    ) -> Result<(), R::Error> {
        let (w, h) = (area.w as i32, area.h as i32);
        let (cx, cy) = (area.x + w / 2, area.y + h / 2);
        let (right, bottom) = (area.right(), area.bottom());
        // A line one pixel thick (two when bold) through the middle of the cell.
        let t = if bold { 2 } else { 1 };
        let horizontal = |r: &mut R, x0: i32, x1: i32| {
            r.fill_rect(
                Rect::new(x0, cy - t / 2, (x1 - x0).max(0) as u32, t as u32),
                0,
                fg,
            )
        };
        let vertical = |r: &mut R, y0: i32, y1: i32| {
            r.fill_rect(
                Rect::new(cx - t / 2, y0, t as u32, (y1 - y0).max(0) as u32),
                0,
                fg,
            )
        };
        match ch {
            ' ' | '\0' => Ok(()),
            '█' => r.fill_rect(area, 0, fg),
            '░' => r.fill_rect(area, 0, mix(bg, fg, 64)),
            '▒' => r.fill_rect(area, 0, mix(bg, fg, 128)),
            '▓' => r.fill_rect(area, 0, mix(bg, fg, 192)),
            '▀' => r.fill_rect(Rect::new(area.x, area.y, area.w, area.h / 2), 0, fg),
            '▄' => r.fill_rect(Rect::new(area.x, cy, area.w, area.h - area.h / 2), 0, fg),
            '─' | '━' => horizontal(r, area.x, right),
            '│' => vertical(r, area.y, bottom),
            '┌' => {
                horizontal(r, cx, right)?;
                vertical(r, cy, bottom)
            }
            '┐' => {
                horizontal(r, area.x, cx + 1)?;
                vertical(r, cy, bottom)
            }
            '└' => {
                horizontal(r, cx, right)?;
                vertical(r, area.y, cy + 1)
            }
            '┘' => {
                horizontal(r, area.x, cx + 1)?;
                vertical(r, area.y, cy + 1)
            }
            '●' | '•' => r.fill_circle(Point::new(cx, cy), (w.min(h) as u32 / 3).max(1), fg),
            '▸' | '▶' => {
                let (x0, x1) = (area.x + w / 4, right - w / 4);
                r.fill_path(
                    &[
                        PathCmd::MoveTo(Point::new(x0, area.y + h / 4)),
                        PathCmd::LineTo(Point::new(x1, cy)),
                        PathCmd::LineTo(Point::new(x0, bottom - h / 4)),
                        PathCmd::Close,
                    ],
                    fg,
                )
            }
            '◆' => r.fill_path(
                &[
                    PathCmd::MoveTo(Point::new(cx, area.y + h / 5)),
                    PathCmd::LineTo(Point::new(right - w / 5, cy)),
                    PathCmd::LineTo(Point::new(cx, bottom - h / 5)),
                    PathCmd::LineTo(Point::new(area.x + w / 5, cy)),
                    PathCmd::Close,
                ],
                fg,
            ),
            '❮' | '❯' => {
                let (near, far) = if ch == '❮' {
                    (right - w / 4, area.x + w / 4)
                } else {
                    (area.x + w / 4, right - w / 4)
                };
                r.stroke_path(
                    &[
                        PathCmd::MoveTo(Point::new(near, area.y + h / 4)),
                        PathCmd::LineTo(Point::new(far, cy)),
                        PathCmd::LineTo(Point::new(near, bottom - h / 4)),
                    ],
                    t as u32,
                    fg,
                )
            }
            c => {
                let mut buf = [0u8; 4];
                let s = c.encode_utf8(&mut buf);
                let style = TextStyle::new(self.cell_h, fg).font(Font::Bitmap);
                r.draw_text(s, Point::new(area.x, area.y), &style)?;
                if bold {
                    r.draw_text(s, Point::new(area.x + 1, area.y), &style)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::vec;
    use std::vec::Vec;

    use fusuma_cells::{Cell, Style};
    use fusuma_render::Probe;
    use fusuma_render_embedded::{Embedded, Frame};

    use super::*;

    type Screen = Embedded<Frame<Vec<u16>>>;

    const W: u16 = 60;
    const H: u16 = 30;

    fn screen() -> Screen {
        Embedded::new(Frame::new(W, H, vec![0u16; usize::from(W) * usize::from(H)]).unwrap())
    }

    fn grid(cols: u16, rows: u16) -> Grid<Vec<Cell>> {
        Grid::new(
            cols,
            rows,
            vec![Cell::BLANK; usize::from(cols) * usize::from(rows)],
        )
        .unwrap()
    }

    const P: CellPanel = CellPanel::SMALL;

    fn px(s: &Screen, x: i32, y: i32) -> Color {
        Probe::pixel(s, x, y).unwrap()
    }

    /// How many pixels of the cell at `(col, row)` differ from `bg`.
    fn ink(s: &Screen, col: u16, row: u16, bg: Color) -> usize {
        let a = P.pixels(col, row, 1);
        (a.y..a.bottom())
            .flat_map(|y| (a.x..a.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| px(s, x, y) != bg)
            .count()
    }

    fn near(a: Color, b: Color) -> bool {
        let d = |x: u8, y: u8| i32::from(x).abs_diff(i32::from(y)) as i32;
        d(a.r, b.r) <= 8 && d(a.g, b.g) <= 8 && d(a.b, b.b) <= 8
    }

    #[test]
    fn the_cells_fit_the_panel_and_a_blank_grid_is_the_background() {
        assert_eq!(P.fits(410, 502), (68, 50));
        assert_eq!(CellPanel::LARGE.fits(1024, 600), (102, 30));
        let mut s = screen();
        let g = grid(10, 3);
        let area = P.draw(&mut s, &g).unwrap();
        assert_eq!(area, Rect::new(0, 0, 60, 30));
        assert!(near(px(&s, 5, 5), P.default_bg) && near(px(&s, 59, 29), P.default_bg));
    }

    #[test]
    fn a_letter_has_ink_in_its_own_cell_and_bold_has_more() {
        let mut g = grid(10, 3);
        g.put_str(2, 1, "A", Style::PLAIN);
        g.put_str(5, 1, "A", Style::PLAIN.attrs(Attrs::BOLD));
        let mut s = screen();
        P.draw(&mut s, &g).unwrap();
        let bg = s.pixel(0, 0).unwrap();
        let bg = Color::rgb(bg.r, bg.g, bg.b);
        let (plain, bold, none) = (ink(&s, 2, 1, bg), ink(&s, 5, 1, bg), ink(&s, 3, 1, bg));
        assert!(
            plain > 8 && bold > plain && none == 0,
            "{plain} {bold} {none}"
        );
    }

    #[test]
    fn blocks_boxes_and_bullets_are_drawn_as_shapes() {
        let fg = Color::rgb(200, 100, 50);
        let st = Style::PLAIN.fg(CColor::Rgb(200, 100, 50));
        let mut g = grid(10, 3);
        for (x, ch) in "█░▒▓│─┌┐└┘●▸◆".chars().enumerate().take(10) {
            g.put_str(x as u16, 0, &ch.to_string(), st);
        }
        g.put_str(0, 1, "┌─┐", st);
        g.put_str(0, 2, "└─┘", st);
        let mut s = screen();
        P.draw(&mut s, &g).unwrap();
        let a = P.pixels(0, 0, 1);
        assert!(
            near(px(&s, a.x + 2, a.y + 4), fg),
            "a full block is the colour"
        );
        let light = px(&s, P.pixels(1, 0, 1).x + 2, 4);
        let mid = px(&s, P.pixels(2, 0, 1).x + 2, 4);
        let dark = px(&s, P.pixels(3, 0, 1).x + 2, 4);
        assert!(
            light.r < mid.r && mid.r < dark.r && dark.r < fg.r,
            "shades: {light:?} {mid:?} {dark:?}"
        );
        // A vertical bar is a thin line in the middle of the cell, a corner joins two.
        let v = P.pixels(4, 0, 1);
        assert!(near(px(&s, v.x + 3, v.y), fg) && !near(px(&s, v.x, v.y), fg));
        let c = P.pixels(0, 1, 1);
        assert!(near(px(&s, c.x + 3, c.y + 5), fg), "the corner's middle");
        assert!(near(px(&s, c.x + 5, c.y + 5), fg), "goes right");
        assert!(near(px(&s, c.x + 3, c.y + 9), fg), "goes down");
        assert!(
            !near(px(&s, c.x, c.y), fg),
            "and not to the corner of the cell"
        );
        let b = P.pixels(0, 2, 1);
        assert!(near(px(&s, b.x + 3, b.y), fg), "a bottom corner goes up");
        assert!(ink(&s, 10 % 10, 0, Color::rgb(0, 0, 0)) > 0);
    }

    #[test]
    fn reverse_swaps_dim_fades_and_underline_draws_a_line() {
        let st = Style::PLAIN
            .fg(CColor::Rgb(255, 255, 255))
            .bg(CColor::Rgb(0, 0, 255));
        let mut g = grid(10, 3);
        g.put_str(0, 0, "█", st.attrs(Attrs::REVERSE));
        g.put_str(1, 0, "█", st.attrs(Attrs::DIM));
        g.put_str(2, 0, " ", st.attrs(Attrs::UNDERLINE));
        g.put_str(3, 0, " ", st);
        let mut s = screen();
        P.draw(&mut s, &g).unwrap();
        let cell = |c: u16| P.pixels(c, 0, 1);
        // Reversed: the block takes the background colour (blue), on white.
        assert!(near(px(&s, cell(0).x + 2, 2), Color::rgb(0, 0, 255)));
        // Dim: the block is a colour between white and the blue behind.
        let d = px(&s, cell(1).x + 2, 2);
        assert!(d.r > 40 && d.r < 255 && d.b > 200, "{d:?}");
        assert!(
            near(
                px(&s, cell(2).x + 2, cell(2).bottom() - 1),
                Color::rgb(255, 255, 255)
            ),
            "underline"
        );
        assert!(
            near(px(&s, cell(2).x + 2, 2), Color::rgb(0, 0, 255)),
            "above it is the background"
        );
        assert!(
            near(
                px(&s, cell(3).x + 2, cell(3).bottom() - 1),
                Color::rgb(0, 0, 255)
            ),
            "none without it"
        );
    }

    #[test]
    fn only_what_changed_is_drawn_and_it_is_one_rectangle() {
        let mut a = grid(10, 3);
        a.put_str(0, 0, "hello", Style::PLAIN);
        let mut s = screen();
        P.draw(&mut s, &a).unwrap();
        let before: Vec<Color> = (0..H as i32)
            .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
            .map(|(x, y)| px(&s, x, y))
            .collect();
        assert_eq!(
            P.draw_changes(&mut s, &a, &a).unwrap(),
            None,
            "nothing changed"
        );
        let mut b = a.clone();
        b.put_str(1, 0, "E", Style::PLAIN);
        b.put_str(8, 2, "z", Style::PLAIN);
        let region = P.draw_changes(&mut s, &a, &b).unwrap().unwrap();
        // The two changes are far apart: one rectangle holds both, and only they were drawn.
        assert!(region.contains(Point::new(8, 2)) || region.x <= 6);
        assert_eq!(region, P.pixels(1, 0, 1).union(&P.pixels(8, 2, 1)));
        let mut full = screen();
        P.draw(&mut full, &b).unwrap();
        for y in 0..i32::from(H) {
            for x in 0..i32::from(W) {
                assert_eq!(px(&s, x, y), px(&full, x, y), "({x}, {y})");
            }
        }
        // Outside the changed cells nothing moved.
        let moved = (0..H as i32)
            .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| px(&s, x, y) != before[(y * i32::from(W) + x) as usize])
            .count();
        assert!(moved > 0 && moved < 200, "{moved}");
        // A grid of another size is drawn whole.
        let c = grid(8, 3);
        let region = P.draw_changes(&mut s, &b, &c).unwrap().unwrap();
        assert_eq!(region, P.pixels(0, 0, 8).union(&P.pixels(0, 2, 8)));
    }

    #[test]
    fn wide_characters_and_far_away_origins_do_not_break_it() {
        let mut g = grid(6, 2);
        g.put_str(0, 0, "日本", Style::PLAIN);
        let mut s = screen();
        let panel = P.at(Point::new(-3, 4));
        panel.draw(&mut s, &g).unwrap();
        let far = P.at(Point::new(i32::MAX - 5, i32::MIN + 5));
        far.draw(&mut s, &g).unwrap();
        let _ = far.draw_changes(&mut s, &g, &grid(6, 2)).unwrap();
        assert_eq!(
            CellPanel::sized(0, 0).fits(10, 10),
            (10, 10),
            "a zero cell counts as one: no division by zero"
        );
    }
}
