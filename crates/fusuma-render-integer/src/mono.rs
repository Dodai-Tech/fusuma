//! A one-bit framebuffer for black and white panels (e-paper, memory LCDs): the same renderer draws on it, and what it holds is
//! what the panel takes. Eight pixels a byte, most significant bit first, a **set bit is white**, rows one after the other
//! each a whole number of bytes. An 800x480 picture is 48 000 bytes where RGB565 is 768 000.
//!
//! Colours are turned to black or white as they are drawn, by a [`Dither`]: a threshold keeps text and lines crisp (what a
//! black-on-white theme wants), an ordered dither makes a gray a pattern of dots. Anti-aliased edges of text and shapes are
//! mixed with what is already there and then turned the same way, so a threshold gives hard edges and a dither gives
//! stippled ones.

use core::convert::Infallible;
use embedded_graphics::{
    Pixel,
    geometry::{OriginDimensions, Size},
    pixelcolor::{IntoStorage, Rgb565},
    prelude::{DrawTarget, Point},
    primitives::Rectangle,
};

use crate::frame::{Blend, PixelSource};

/// The brightness of an RGB565 pixel, 0 to 255 (Rec. 601 weights).
#[must_use]
pub const fn luma565(p: u16) -> u8 {
    let r = ((p >> 11) & 0x1F) as u32;
    let g = ((p >> 5) & 0x3F) as u32;
    let b = (p & 0x1F) as u32;
    // Each channel spread to 8 bits, weights 77, 150, 29 of 256.
    let (r, g, b) = (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    );
    ((77 * r + 150 * g + 29 * b) >> 8) as u8
}

/// How a gray becomes black or white.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dither {
    /// Black below the level (0 to 255), white from it: text and lines stay crisp. The default, at 128.
    Threshold(u8),
    /// A 4x4 ordered dither (Bayer): a gray is a pattern of dots, the same pattern wherever the pixel is, so a partial refresh
    /// matches the picture around it.
    Ordered,
}

impl Default for Dither {
    fn default() -> Self {
        Self::Threshold(128)
    }
}

const BAYER4: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

impl Dither {
    /// Whether the pixel at `(x, y)` with brightness `luma` is white.
    #[must_use]
    pub const fn white(self, x: u32, y: u32, luma: u8) -> bool {
        match self {
            Self::Threshold(level) => luma >= level,
            Self::Ordered => {
                let t = BAYER4[(y & 3) as usize][(x & 3) as usize] as u32 * 16 + 8;
                luma as u32 >= t
            }
        }
    }

    /// Whether the answer does not depend on where the pixel is (so a whole run can be set at once).
    #[must_use]
    pub const fn is_flat(self) -> bool {
        matches!(self, Self::Threshold(_))
    }
}

/// A one-bit framebuffer of `width` x `height` pixels over any byte storage.
#[derive(Clone, Debug)]
pub struct MonoFrame<S> {
    width: u16,
    height: u16,
    stride: usize,
    bits: S,
    dither: Dither,
}

impl<S: AsRef<[u8]>> MonoFrame<S> {
    /// Bytes in a row of `width` pixels.
    #[must_use]
    pub const fn stride_of(width: u16) -> usize {
        (width as usize).div_ceil(8)
    }

    /// Wraps `bits`, which must hold exactly `stride_of(width) * height` bytes; `None` otherwise. Start it as `0xFF` (paper):
    /// the pixels past the right edge of a row stay white for ever, as panels want them.
    #[must_use]
    pub fn new(width: u16, height: u16, bits: S, dither: Dither) -> Option<Self> {
        let stride = Self::stride_of(width);
        (bits.as_ref().len() == stride * usize::from(height)).then_some(Self {
            width,
            height,
            stride,
            bits,
            dither,
        })
    }

    /// All bytes, row by row: what a panel takes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.bits.as_ref()
    }

    /// Bytes in a row.
    #[must_use]
    pub const fn stride(&self) -> usize {
        self.stride
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u16 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u16 {
        self.height
    }

    /// The dither in use.
    #[must_use]
    pub const fn dither(&self) -> Dither {
        self.dither
    }

    /// Whether the pixel at `(x, y)` is white; `None` outside.
    #[must_use]
    pub fn white_at(&self, x: i32, y: i32) -> Option<bool> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x >= usize::from(self.width) || y >= usize::from(self.height) {
            return None;
        }
        let byte = *self.bits.as_ref().get(y * self.stride + x / 8)?;
        Some(byte & (0x80 >> (x % 8)) != 0)
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> MonoFrame<S> {
    /// Changes the dither from now on.
    pub fn set_dither(&mut self, dither: Dither) {
        self.dither = dither;
    }

    fn put(&mut self, x: usize, y: usize, white: bool) {
        let at = y * self.stride + x / 8;
        let mask = 0x80u8 >> (x % 8);
        let b = &mut self.bits.as_mut()[at];
        if white {
            *b |= mask;
        } else {
            *b &= !mask;
        }
    }

    /// Sets pixels `x0..x1` of row `y` (inside the frame) to one colour, a byte at a time where it can.
    fn run(&mut self, y: usize, x0: usize, x1: usize, white: bool) {
        let row = &mut self.bits.as_mut()[y * self.stride..(y + 1) * self.stride];
        let (b0, b1) = (x0 / 8, (x1 - 1) / 8);
        let head = 0xFFu8 >> (x0 % 8);
        let tail = 0xFFu8 << (7 - (x1 - 1) % 8);
        let apply = |b: &mut u8, mask: u8| {
            if white {
                *b |= mask;
            } else {
                *b &= !mask;
            }
        };
        if b0 == b1 {
            apply(&mut row[b0], head & tail);
            return;
        }
        apply(&mut row[b0], head);
        for b in &mut row[b0 + 1..b1] {
            *b = if white { 0xFF } else { 0x00 };
        }
        apply(&mut row[b1], tail);
    }
}

impl<S: AsRef<[u8]>> PixelSource for MonoFrame<S> {
    fn pixel_at(&self, x: i32, y: i32) -> Option<Rgb565> {
        self.white_at(x, y).map(|w| {
            if w {
                Rgb565::new(31, 63, 31)
            } else {
                Rgb565::new(0, 0, 0)
            }
        })
    }
}

impl<S: AsRef<[u8]>> OriginDimensions for MonoFrame<S> {
    fn size(&self) -> Size {
        Size::new(u32::from(self.width), u32::from(self.height))
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> DrawTarget for MonoFrame<S> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        let (w, h) = (i32::from(self.width), i32::from(self.height));
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.y >= 0 && p.x < w && p.y < h {
                let white = self
                    .dither
                    .white(p.x as u32, p.y as u32, luma565(c.into_storage()));
                self.put(p.x as usize, p.y as usize, white);
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Infallible> {
        let area = area.intersection(&Rectangle::new(Point::zero(), self.size()));
        let Some(br) = area.bottom_right() else {
            return Ok(());
        };
        let l = luma565(color.into_storage());
        let (x0, x1) = (area.top_left.x as usize, br.x as usize + 1);
        for y in area.top_left.y as usize..=br.y as usize {
            if self.dither.is_flat() {
                let white = self.dither.white(0, 0, l);
                self.run(y, x0, x1, white);
            } else {
                for x in x0..x1 {
                    let white = self.dither.white(x as u32, y as u32, l);
                    self.put(x, y, white);
                }
            }
        }
        Ok(())
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> MonoFrame<S> {
    /// Mixes `src` (a brightness) into pixel `(x, y)` with coverage `a`, then turns the result to black or white.
    fn blend_pixel(&mut self, x: usize, y: usize, src: u8, a: u8) {
        if a == 0 {
            return;
        }
        let dst: u32 = if self.white_at(x as i32, y as i32) == Some(true) {
            255
        } else {
            0
        };
        let a = u32::from(a);
        let mixed = ((u32::from(src) * a + dst * (255 - a) + 127) / 255) as u8;
        let white = self.dither.white(x as u32, y as u32, mixed);
        self.put(x, y, white);
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> Blend for MonoFrame<S> {
    fn can_blend(&self) -> bool {
        true
    }

    fn blend_row(&mut self, x: i32, y: i32, alpha: &[u8], color: Rgb565) -> bool {
        let (w, h) = (i32::from(self.width), i32::from(self.height));
        if y < 0 || y >= h {
            return true;
        }
        let src = luma565(color.into_storage());
        for (i, &a) in alpha.iter().enumerate() {
            let Some(px) = i32::try_from(i).ok().and_then(|i| x.checked_add(i)) else {
                break;
            };
            if px >= 0 && px < w {
                self.blend_pixel(px as usize, y as usize, src, a);
            }
        }
        true
    }

    fn blend_colors(&mut self, x: i32, y: i32, alpha: &[u8], colors: &[u16]) -> bool {
        let (w, h) = (i32::from(self.width), i32::from(self.height));
        if y < 0 || y >= h {
            return true;
        }
        for (i, (&a, &c)) in alpha.iter().zip(colors).enumerate() {
            let Some(px) = i32::try_from(i).ok().and_then(|i| x.checked_add(i)) else {
                break;
            };
            if px >= 0 && px < w {
                self.blend_pixel(px as usize, y as usize, luma565(c), a);
            }
        }
        true
    }

    fn shift_rows(&mut self, x: i32, y: i32, w: u32, h: u32, dy: i32) -> bool {
        let (fw, fh) = (usize::from(self.width), usize::from(self.height));
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return false;
        };
        let (w, h) = (w as usize, h as usize);
        if x + w > fw || y + h > fh {
            return false;
        }
        // Whole bytes only: the window starts on a byte and ends on one or at the frame's edge.
        if x % 8 != 0 || (w % 8 != 0 && x + w != fw) {
            return false;
        }
        let by = dy.unsigned_abs() as usize;
        if by == 0 || by >= h {
            return true;
        }
        let (b0, b1) = (x / 8, (x + w).div_ceil(8));
        let stride = self.stride;
        let bits = self.bits.as_mut();
        for i in 0..h - by {
            let (from, to) = if dy > 0 {
                let r = h - by - 1 - i;
                (y + r, y + r + by)
            } else {
                (y + by + i, y + i)
            };
            bits.copy_within(from * stride + b0..from * stride + b1, to * stride + b0);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    fn paper(w: u16, h: u16, d: Dither) -> MonoFrame<Vec<u8>> {
        MonoFrame::new(
            w,
            h,
            vec![0xFF; MonoFrame::<Vec<u8>>::stride_of(w) * usize::from(h)],
            d,
        )
        .unwrap()
    }

    const BLACK: Rgb565 = Rgb565::new(0, 0, 0);
    const WHITE: Rgb565 = Rgb565::new(31, 63, 31);

    #[test]
    fn brightness_runs_from_black_to_white_and_green_is_brighter_than_blue() {
        assert_eq!(luma565(0), 0);
        assert_eq!(luma565(0xFFFF), 255);
        assert!(luma565(0x07E0) > luma565(0x001F) * 3);
    }

    #[test]
    fn the_size_must_match_and_a_pixel_reads_back_with_the_edge_as_paper() {
        assert!(MonoFrame::new(10, 2, vec![0u8; 3], Dither::default()).is_none());
        let mut f = paper(10, 2, Dither::default());
        assert_eq!(
            (f.stride(), f.bytes().len(), f.width(), f.height()),
            (2, 4, 10, 2)
        );
        f.fill_solid(&Rectangle::new(Point::new(2, 0), Size::new(3, 1)), BLACK)
            .unwrap();
        assert_eq!(f.white_at(1, 0), Some(true));
        assert_eq!(f.white_at(2, 0), Some(false));
        assert_eq!(f.white_at(4, 0), Some(false));
        assert_eq!(f.white_at(5, 0), Some(true));
        assert_eq!(
            (f.white_at(10, 0), f.white_at(-1, 0), f.white_at(0, 2)),
            (None, None, None)
        );
        // Row 0: 11000111 11111111 (the last six bits are past the edge, and stay paper).
        assert_eq!(&f.bytes()[..2], &[0b1100_0111, 0xFF]);
        assert_eq!(f.pixel_at(2, 0), Some(BLACK));
        assert_eq!(f.pixel_at(5, 0), Some(WHITE));
    }

    #[test]
    fn a_fill_is_right_in_every_alignment_and_never_touches_the_padding_or_other_rows() {
        for w in [1u16, 7, 8, 9, 15, 16, 17, 33] {
            for x0 in 0..w {
                for x1 in x0 + 1..=w {
                    let mut f = paper(w, 3, Dither::default());
                    f.fill_solid(
                        &Rectangle::new(
                            Point::new(i32::from(x0), 1),
                            Size::new(u32::from(x1 - x0), 1),
                        ),
                        BLACK,
                    )
                    .unwrap();
                    for y in 0..3 {
                        for x in 0..w {
                            let want = !(y == 1 && (x0..x1).contains(&x));
                            assert_eq!(
                                f.white_at(i32::from(x), y),
                                Some(want),
                                "w {w} [{x0},{x1}) at ({x}, {y})"
                            );
                        }
                    }
                    // The bits past the right edge are still set.
                    for y in 0..3usize {
                        let last = f.bytes()[y * f.stride() + f.stride() - 1];
                        let pad = if w % 8 == 0 { 0 } else { 0xFFu8 >> (w % 8) };
                        assert_eq!(last & pad, pad, "padding, w {w} [{x0},{x1}) row {y}");
                    }
                }
            }
        }
        // A fill outside changes nothing.
        let mut f = paper(8, 2, Dither::default());
        f.fill_solid(&Rectangle::new(Point::new(20, 20), Size::new(4, 4)), BLACK)
            .unwrap();
        assert!(f.bytes().iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn a_threshold_is_crisp_and_an_ordered_dither_turns_a_gray_into_dots_that_average_right() {
        let gray = Rgb565::new(15, 31, 15);
        let mut t = paper(16, 16, Dither::Threshold(128));
        t.fill_solid(&Rectangle::new(Point::zero(), Size::new(16, 16)), gray)
            .unwrap();
        let ones = |f: &MonoFrame<Vec<u8>>| f.bytes().iter().map(|b| b.count_ones()).sum::<u32>();
        assert!(
            ones(&t) == 0 || ones(&t) == 256,
            "all one colour: {}",
            ones(&t)
        );
        let mut o = paper(16, 16, Dither::Ordered);
        o.fill_solid(&Rectangle::new(Point::zero(), Size::new(16, 16)), gray)
            .unwrap();
        let white = ones(&o);
        assert!(
            (100..=156).contains(&white),
            "about half the dots are white: {white} of 256"
        );
        // The same gray gives the same pattern wherever it is (a partial refresh matches the rest).
        assert_eq!(o.white_at(1, 2), o.white_at(5, 6));
        // Black and white stay black and white.
        let mut b = paper(8, 8, Dither::Ordered);
        b.fill_solid(&Rectangle::new(Point::zero(), Size::new(8, 8)), BLACK)
            .unwrap();
        assert_eq!(ones(&b), 0);
    }

    #[test]
    fn blending_mixes_with_what_is_there_then_turns_black_or_white() {
        // Black text on paper: full coverage is black, none stays, a little stays white and most goes black (threshold).
        let mut f = paper(8, 1, Dither::Threshold(128));
        assert!(f.can_blend());
        assert!(f.blend_row(1, 0, &[255, 0, 60, 200], BLACK));
        assert_eq!(
            [1, 2, 3, 4].map(|x| f.white_at(x, 0).unwrap()),
            [false, true, true, false]
        );
        // White on black.
        let mut g = paper(4, 1, Dither::Threshold(128));
        g.fill_solid(&Rectangle::new(Point::zero(), Size::new(4, 1)), BLACK)
            .unwrap();
        assert!(g.blend_row(0, 0, &[255, 40, 220], WHITE));
        assert_eq!(
            [0, 1, 2].map(|x| g.white_at(x, 0).unwrap()),
            [true, false, true]
        );
        // A row outside, and pixels outside a row, are skipped.
        assert!(g.blend_row(0, 5, &[255], WHITE));
        assert!(g.blend_row(-2, 0, &[255, 255, 255], BLACK));
        assert_eq!(
            g.white_at(0, 0),
            Some(false),
            "pixel 0 was the third of the row"
        );
        // With colours: an image with transparency.
        let mut h = paper(3, 1, Dither::default());
        assert!(h.blend_colors(0, 0, &[255, 255, 0], &[0x0000, 0xFFFF, 0x0000]));
        assert_eq!(
            [0, 1, 2].map(|x| h.white_at(x, 0).unwrap()),
            [false, true, true]
        );
    }

    #[test]
    fn shifting_rows_moves_whole_bytes_both_ways_and_refuses_what_it_cannot() {
        let (w, h) = (24u16, 10u16);
        let fresh = || {
            let bits: Vec<u8> = (0..MonoFrame::<Vec<u8>>::stride_of(w) * usize::from(h))
                .map(|i| i as u8)
                .collect();
            MonoFrame::new(w, h, bits, Dither::default()).unwrap()
        };
        for dy in [-3, -1, 1, 2, 4] {
            let mut f = fresh();
            let before = f.bytes().to_vec();
            assert!(f.shift_rows(8, 2, 16, 6, dy));
            let s = f.stride();
            for y in 0..10usize {
                for b in 0..s {
                    let inside = b >= 1 && (2..8).contains(&y);
                    let from = y as i32 - dy;
                    let want = if inside && (2..8).contains(&from) {
                        before[from as usize * s + b]
                    } else {
                        before[y * s + b]
                    };
                    assert_eq!(f.bytes()[y * s + b], want, "dy {dy} byte {b} row {y}");
                }
            }
        }
        let mut f = fresh();
        assert!(!f.shift_rows(3, 0, 8, 4, 1), "not on a byte");
        assert!(!f.shift_rows(0, 0, 12, 4, 1), "ends inside a byte");
        assert!(!f.shift_rows(16, 0, 16, 4, 1), "past the edge");
        assert!(f.shift_rows(0, 0, 24, 4, 9) && f.shift_rows(0, 0, 24, 4, 0));
    }

    #[test]
    fn the_renderer_draws_on_it_text_edges_and_all() {
        use fusuma_render::{Color, Rect, Renderer};
        let mut r = crate::Embedded::new(paper(64, 32, Dither::Threshold(192)));
        r.clear(Color::WHITE).unwrap();
        r.fill_rect(Rect::new(4, 4, 20, 10), 3, Color::BLACK)
            .unwrap();
        // The middle of the box is ink, the paper around it is paper.
        assert_eq!(r.target().white_at(14, 9), Some(false));
        assert_eq!(r.target().white_at(40, 20), Some(true));
        // A bit of text: some ink, and only black and white are in the picture.
        let style = fusuma_render::TextStyle::new(12, Color::BLACK);
        r.draw_text("Hi", fusuma_render::Point::new(30, 4), &style)
            .unwrap();
        let ink = (26..64)
            .flat_map(|x| (0..32).map(move |y| (x, y)))
            .filter(|&(x, y)| r.target().white_at(x, y) == Some(false))
            .count();
        assert!(ink > 10, "the text left ink: {ink}");
    }
}
