//! What a board is: a screen that takes rows of pixels, and a touch panel that says where a finger is.

use fusuma::render::{Point, Rect};

/// RGBA8 (every pixel here is opaque) to RGB565, the format nearly every small panel takes.
#[must_use]
pub const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | (b as u16 >> 3)
}

/// The picture a frame drew, readable a row at a time as RGB565. A board asks for the rows it wants to show, in
/// whatever order and chunks its panel likes: a panel with a frame buffer copies them in, one on an SPI bus sends
/// them as they come.
pub trait Pixels {
    /// Writes the pixels of row `y` from column `x` on into `out` (`out.len()` of them). Rows or columns outside
    /// the picture are left as they were.
    fn row(&mut self, y: u32, x: u32, out: &mut [u16]);
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
}
