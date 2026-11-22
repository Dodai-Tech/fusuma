//! A plain RGB565 framebuffer over any storage (`Vec<u16>`, `[u16; N]`, `&mut [u16]`).

use core::convert::Infallible;
use embedded_graphics::{
    Pixel,
    geometry::{OriginDimensions, Size},
    pixelcolor::{IntoStorage, Rgb565},
    prelude::{DrawTarget, Point},
    primitives::Rectangle,
};

/// Reads a pixel back; lets a target be used with [`tokonoma::Probe`] in tests and screenshots.
pub trait PixelSource {
    /// The pixel at `(x, y)`, `None` outside.
    fn pixel_at(&self, x: i32, y: i32) -> Option<Rgb565>;
}

/// An RGB565 framebuffer of `width` x `height` pixels.
#[derive(Clone, Debug)]
pub struct Frame<S> {
    width: u16,
    height: u16,
    pixels: S,
}

impl<S: AsRef<[u16]>> Frame<S> {
    /// Wraps `pixels`, which must hold exactly `width * height` values; `None` otherwise.
    #[must_use]
    pub fn new(width: u16, height: u16, pixels: S) -> Option<Self> {
        (pixels.as_ref().len() == usize::from(width) * usize::from(height)).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// All pixels, row by row.
    #[must_use]
    pub fn pixels(&self) -> &[u16] {
        self.pixels.as_ref()
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
}

impl<S: AsRef<[u16]>> PixelSource for Frame<S> {
    fn pixel_at(&self, x: i32, y: i32) -> Option<Rgb565> {
        let (x, y) = (usize::try_from(x).ok()?, usize::try_from(y).ok()?);
        if x >= usize::from(self.width) || y >= usize::from(self.height) {
            return None;
        }
        let v = *self.pixels.as_ref().get(y * usize::from(self.width) + x)?;
        Some(Rgb565::new(
            (v >> 11) as u8 & 0x1f,
            (v >> 5) as u8 & 0x3f,
            v as u8 & 0x1f,
        ))
    }
}

impl<S: AsRef<[u16]>> OriginDimensions for Frame<S> {
    fn size(&self) -> Size {
        Size::new(u32::from(self.width), u32::from(self.height))
    }
}

impl<S: AsRef<[u16]> + AsMut<[u16]>> DrawTarget for Frame<S> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        let (w, h) = (i32::from(self.width), i32::from(self.height));
        let buf = self.pixels.as_mut();
        for Pixel(p, c) in pixels {
            if p.x >= 0 && p.y >= 0 && p.x < w && p.y < h {
                buf[(p.y * w + p.x) as usize] = c.into_storage();
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Infallible> {
        let area = area.intersection(&Rectangle::new(Point::zero(), self.size()));
        let Some(br) = area.bottom_right() else {
            return Ok(());
        };
        let w = usize::from(self.width);
        let (x0, x1) = (area.top_left.x as usize, br.x as usize + 1);
        let value = color.into_storage();
        let buf = self.pixels.as_mut();
        for y in area.top_left.y as usize..=br.y as usize {
            buf[y * w + x0..y * w + x1].fill(value);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;

    #[test]
    fn size_must_match_and_pixels_read_back() {
        assert!(Frame::new(2, 2, vec![0u16; 3]).is_none());
        let mut f = Frame::new(3, 2, [0u16; 6]).unwrap();
        f.fill_solid(
            &Rectangle::new(Point::new(1, 0), Size::new(5, 5)),
            Rgb565::new(31, 0, 0),
        )
        .unwrap();
        assert_eq!(f.pixel_at(0, 0), Some(Rgb565::new(0, 0, 0)));
        assert_eq!(f.pixel_at(2, 1), Some(Rgb565::new(31, 0, 0)));
        assert_eq!(
            (f.pixel_at(3, 0), f.pixel_at(-1, 0), f.pixel_at(0, 2)),
            (None, None, None)
        );
        f.draw_iter([
            Pixel(Point::new(0, 0), Rgb565::new(0, 63, 0)),
            Pixel(Point::new(9, 9), Rgb565::new(1, 1, 1)),
        ])
        .unwrap();
        assert_eq!(f.pixel_at(0, 0), Some(Rgb565::new(0, 63, 0)));
        assert_eq!((f.width(), f.height(), f.pixels().len()), (3, 2, 6));
    }

    #[test]
    fn a_fill_outside_changes_nothing() {
        let mut f = Frame::new(2, 2, vec![0u16; 4]).unwrap();
        f.fill_solid(
            &Rectangle::new(Point::new(5, 5), Size::new(2, 2)),
            Rgb565::new(31, 63, 31),
        )
        .unwrap();
        assert!(f.pixels().iter().all(|&p| p == 0));
    }
}
