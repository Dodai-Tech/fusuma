//! What a board is: a screen that takes rows of pixels, and a touch panel that says where a finger is.

use fusuma::render::{Point, Rect};

/// RGBA8 (every pixel here is opaque) to RGB565, the format nearly every small panel takes.
#[must_use]
pub const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | (b as u16 >> 3)
}

/// `region` grown outwards to multiples of `step` pixels and cut to a `width x height` screen, for a panel that only
/// takes rectangles on such a grid (many QSPI AMOLED controllers, the CO5300 among them, need even columns and rows:
/// `step` 2). `None` when nothing of it is on the screen. A `step` of 0 or 1 only cuts to the screen.
#[must_use]
pub fn aligned(region: Rect, width: u32, height: u32, step: u32) -> Option<Rect> {
    if region.w == 0 || region.h == 0 {
        return None;
    }
    let step = i64::from(step.max(1));
    // In 64 bits a region's far edge cannot overflow.
    let (w, h) = (i64::from(width), i64::from(height));
    let x0 = i64::from(region.x).clamp(0, w);
    let y0 = i64::from(region.y).clamp(0, h);
    let x1 = (i64::from(region.x) + i64::from(region.w)).clamp(0, w);
    let y1 = (i64::from(region.y) + i64::from(region.h)).clamp(0, h);
    let down = |v: i64| v - v.rem_euclid(step);
    let up = |v: i64, max: i64| down(v + step - 1).min(max);
    let (x0, y0, x1, y1) = (down(x0), down(y0), up(x1, w), up(y1, h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    // Inside a screen of u32 sides that fits i32 coordinates, so the casts are exact.
    Some(Rect::new(
        x0 as i32,
        y0 as i32,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
    ))
}

/// How many rows of `width` pixels a band buffer of `capacity` pixels holds, rounded down to a multiple of `step` so
/// that every band of an [`aligned`] region starts on the grid too. At least `step` (at least 1), even when that is
/// more than fits: a caller sizes its buffer for one aligned band of the widest region.
#[must_use]
pub const fn band_rows(width: u32, capacity: u32, step: u32) -> u32 {
    let step = if step == 0 { 1 } else { step };
    let rows = capacity / if width == 0 { 1 } else { width };
    let rows = rows - rows % step;
    if rows == 0 { step } else { rows }
}

/// A touch sample as the controller reports it, turned to the screen: x and y swapped first if `swap_xy`, then
/// flipped along each axis that `mirror` says, and cut to the `width x height` screen.
#[must_use]
pub fn orient(x: u16, y: u16, width: u32, height: u32, swap_xy: bool, mirror: [bool; 2]) -> Point {
    let (x, y) = if swap_xy { (y, x) } else { (x, y) };
    let fit = |v: u16, max: u32, flip: bool| {
        let v = i32::from(v)
            .min(i32::try_from(max).unwrap_or(i32::MAX) - 1)
            .max(0);
        if flip {
            i32::try_from(max).unwrap_or(i32::MAX) - 1 - v
        } else {
            v
        }
    };
    Point::new(fit(x, width, mirror[0]), fit(y, height, mirror[1]))
}

/// How many failed reads in a row a held finger survives. Some controllers (the FT3168) answer NAK now and then
/// while idle or touched; more failures than this mean the bus is gone, and a finger is let go rather than held
/// forever.
pub const HELD_THROUGH_ERRORS: u32 = 5;

/// Where the finger is, from samples that may fail: a failed read keeps the last position, up to
/// [`HELD_THROUGH_ERRORS`] in a row.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeldTouch {
    held: Option<Point>,
    failed: u32,
    /// Failed reads since the start.
    pub errors: u32,
}

impl HeldTouch {
    /// Takes one sample (`None` when the read failed) and says where the finger is.
    pub fn sample(&mut self, read: Option<Option<Point>>) -> Option<Point> {
        match read {
            Some(at) => {
                self.held = at;
                self.failed = 0;
            }
            None => {
                self.errors = self.errors.saturating_add(1);
                self.failed = self.failed.saturating_add(1);
                if self.failed > HELD_THROUGH_ERRORS {
                    self.held = None;
                }
            }
        }
        self.held
    }
}

/// The picture a frame drew, readable a row at a time as RGB565. A board asks for the rows it wants to show, in
/// whatever order and chunks its panel likes: a panel with a frame buffer copies them in, one on an SPI bus sends
/// them as they come.
pub trait Pixels {
    /// Writes the pixels of row `y` from column `x` on into `out` (`out.len()` of them). Rows or columns outside
    /// the picture are left as they were.
    fn row(&mut self, y: u32, x: u32, out: &mut [u16]);

    /// Writes `block` into `out` in one call, as many rows as `out` holds. Faster than [`row`](Self::row) row by
    /// row: one call, one pass, and the byte swap a panel needs done on the way. What lies outside the picture is
    /// left as it was.
    fn rows(&mut self, block: Block, out: &mut [u16]) {
        let width = block.width as usize;
        if width == 0 || block.stride < width {
            return;
        }
        for (i, line) in out.chunks_mut(block.stride).enumerate() {
            let Some(dst) = line.get_mut(..width) else {
                break;
            };
            self.row(block.y.saturating_add(i as u32), block.x, dst);
            block.order.apply(dst);
        }
    }
}

/// Which rows a board asks [`Pixels::rows`] for, and how it wants them laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Block {
    /// The first row.
    pub y: u32,
    /// The first column.
    pub x: u32,
    /// Pixels a row.
    pub width: u32,
    /// Where each row goes in the output: one row every `stride` pixels (`width` for a packed band, the screen
    /// width for a frame buffer).
    pub stride: usize,
    /// The byte order the panel takes.
    pub order: ByteOrder,
}

impl Block {
    /// A packed band: rows of `width` pixels from `(x, y)`, one after the other.
    #[must_use]
    pub const fn band(x: u32, y: u32, width: u32, order: ByteOrder) -> Self {
        Self {
            y,
            x,
            width,
            stride: width as usize,
            order,
        }
    }

    /// The part of this block inside a `w` x `h` picture, for an output of `out` pixels: the pixels a row and the
    /// number of rows. `None` when nothing is inside.
    #[must_use]
    pub fn fit(&self, w: u32, h: u32, out: usize) -> Option<(usize, usize)> {
        let (w, x, width, stride) = (
            w as usize,
            self.x as usize,
            self.width as usize,
            self.stride,
        );
        if x >= w || self.y >= h || width == 0 || stride < width {
            return None;
        }
        // Rows that `out` holds whole (a last one may be cut to `width`).
        let fit = if out >= width {
            (out - width) / stride + 1
        } else {
            0
        };
        let rows = fit.min((h - self.y) as usize);
        (rows > 0).then_some((width.min(w - x), rows))
    }
}

/// The byte order a panel takes its RGB565 pixels in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ByteOrder {
    /// The CPU's own (little-endian on every ESP chip): a frame buffer scanned out by DMA.
    #[default]
    Native,
    /// High byte first, as most SPI and QSPI panels take it.
    BigEndian,
}

impl ByteOrder {
    /// `px` (native RGB565) as this order puts it in memory.
    #[must_use]
    #[inline]
    pub const fn of(self, px: u16) -> u16 {
        match self {
            Self::Native => px,
            Self::BigEndian => px.to_be(),
        }
    }

    /// Turns native pixels into this order, in place.
    pub fn apply(self, pixels: &mut [u16]) {
        if self == Self::BigEndian {
            for p in pixels {
                *p = p.to_be();
            }
        }
    }
}

/// A board: one screen and, usually, one touch panel. A chip's runtime brings the hardware up and hands the
/// result to the shared [`Driver`](crate::Driver) as a `Board`; nothing in a board knows about widgets.
pub trait Board {
    /// What goes wrong talking to the hardware.
    type Error: core::fmt::Debug;

    /// The screen size in pixels.
    fn size(&self) -> (u32, u32);

    /// The outline of the visible screen: a watch has rounded corners, a round one is a circle. The default is a
    /// rectangle. The app gets it at start and keeps its layout inside what the shape leaves.
    fn shape(&self) -> fusuma::Shape {
        fusuma::Shape::Rect
    }

    /// What the device is: its form factor, the glass's size, whether it reflects light and how it is driven. The default
    /// is a screen of [`size`](Board::size) and [`shape`](Board::shape) that nothing else is known of; a board built
    /// from a board file says the rest (its `[screen]`).
    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        fusuma::Screen::new(w, h).with_shape(self.shape())
    }

    /// Where a finger is touching, in screen pixels, or `None` when there is none. A board without a touch
    /// panel always answers `None`.
    ///
    /// # Errors
    /// The bus error. A flaky sample is better answered `Ok(None)`: the driver treats an error as a lifted finger
    /// and keeps running.
    fn touch(&mut self) -> Result<Option<Point>, Self::Error>;

    /// Shows `region` of the picture on the screen, taking its pixels from `pixels`. The region is already cut
    /// to the screen.
    ///
    /// # Errors
    /// The bus error.
    fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) -> Result<(), Self::Error>;

    /// A monotonic clock in microseconds, for the [`Driver`](crate::Driver) to time the stages of a step
    /// ([`Timing`](crate::Timing)). `None` (the default) means no timing.
    fn now_us(&self) -> Option<u64> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_keeps_the_top_bits_of_each_channel() {
        assert_eq!(rgb565(0, 0, 0), 0);
        assert_eq!(rgb565(255, 255, 255), 0xFFFF);
        assert_eq!(rgb565(255, 0, 0), 0xF800);
        assert_eq!(rgb565(0, 255, 0), 0x07E0);
        assert_eq!(rgb565(0, 0, 255), 0x001F);
        assert_eq!(rgb565(0x07, 0x03, 0x07), 0, "the low bits are dropped");
    }

    #[test]
    fn touch_is_turned_to_the_screen_and_cut_to_it() {
        assert_eq!(
            orient(10, 20, 100, 50, false, [false, false]),
            Point::new(10, 20)
        );
        assert_eq!(
            orient(10, 20, 100, 50, false, [true, true]),
            Point::new(89, 29)
        );
        assert_eq!(
            orient(10, 20, 100, 50, true, [false, false]),
            Point::new(20, 10),
            "swapped"
        );
        assert_eq!(
            orient(10, 20, 100, 50, true, [true, false]),
            Point::new(79, 10),
            "swapped, then flipped"
        );
        assert_eq!(
            orient(500, 500, 100, 50, false, [false, false]),
            Point::new(99, 49),
            "cut to the screen"
        );
        assert_eq!(
            orient(500, 0, 100, 50, false, [true, false]),
            Point::new(0, 0)
        );
    }

    #[test]
    fn a_held_finger_survives_a_few_failed_reads_but_not_a_dead_bus() {
        let mut t = HeldTouch::default();
        let p = Some(Point::new(5, 6));
        assert_eq!(t.sample(Some(p)), p);
        for _ in 0..HELD_THROUGH_ERRORS {
            assert_eq!(t.sample(None), p, "a NAK now and then keeps the finger");
        }
        assert_eq!(t.sample(None), None, "one more: the bus is gone, let go");
        assert_eq!(t.sample(None), None);
        assert_eq!(t.sample(Some(p)), p, "a good read takes it again");
        assert_eq!(t.sample(Some(None)), None, "lifted");
        assert_eq!(t.errors, HELD_THROUGH_ERRORS + 2);
    }

    #[test]
    fn a_region_grows_outwards_to_the_grid_and_stays_on_the_screen() {
        let r = |x, y, w, h| Rect::new(x, y, w, h);
        assert_eq!(aligned(r(3, 5, 4, 1), 410, 502, 2), Some(r(2, 4, 6, 2)));
        assert_eq!(
            aligned(r(2, 4, 6, 2), 410, 502, 2),
            Some(r(2, 4, 6, 2)),
            "already aligned"
        );
        assert_eq!(
            aligned(r(0, 0, 410, 502), 410, 502, 2),
            Some(r(0, 0, 410, 502))
        );
        assert_eq!(
            aligned(r(-5, 499, 20, 50), 410, 502, 2),
            Some(r(0, 498, 16, 4)),
            "cut, then aligned"
        );
        assert_eq!(aligned(r(3, 3, 1, 1), 7, 7, 2), Some(r(2, 2, 2, 2)));
        assert_eq!(
            aligned(r(5, 5, 9, 9), 7, 7, 2),
            Some(r(4, 4, 3, 3)),
            "an odd screen edge is kept"
        );
        assert_eq!(aligned(r(3, 5, 4, 1), 410, 502, 1), Some(r(3, 5, 4, 1)));
        assert_eq!(aligned(r(3, 5, 4, 1), 410, 502, 0), Some(r(3, 5, 4, 1)));
        assert_eq!(
            aligned(r(500, 0, 4, 4), 410, 502, 2),
            None,
            "off the screen"
        );
        assert_eq!(aligned(r(10, 10, 0, 4), 410, 502, 2), None, "empty");
        assert_eq!(
            aligned(r(i32::MAX, i32::MIN, u32::MAX, u32::MAX), 410, 502, 2),
            None
        );
        assert_eq!(
            aligned(r(i32::MIN, i32::MIN, u32::MAX, u32::MAX), 410, 502, 2),
            Some(r(0, 0, 410, 502))
        );
    }

    #[test]
    fn bands_hold_whole_rows_and_start_on_the_grid() {
        assert_eq!(
            band_rows(410, 16 * 1024, 2),
            38,
            "39 rows fit, an odd number: one fewer"
        );
        assert_eq!(band_rows(410, 16 * 1024, 1), 39);
        assert_eq!(band_rows(2, 16 * 1024, 2), 8192);
        assert_eq!(
            band_rows(20_000, 16 * 1024, 2),
            2,
            "not one row fits: still one aligned band"
        );
        assert_eq!((band_rows(0, 100, 0), band_rows(0, 0, 0)), (100, 1));
        // Every band of a full aligned frame starts on an even row.
        let rows = band_rows(410, 16 * 1024, 2);
        assert!((0..502).step_by(rows as usize).all(|y| y % 2 == 0));
    }
}
