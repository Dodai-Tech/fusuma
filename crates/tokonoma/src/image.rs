//! A borrowed RGBA image.

/// An image of `width` x `height` pixels: 4 bytes per pixel, straight RGBA, row by row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image<'a> {
    width: u32,
    height: u32,
    rgba: &'a [u8],
}

impl<'a> Image<'a> {
    /// Wraps `rgba`, which must hold exactly `width * height * 4` bytes; `None` otherwise.
    #[must_use]
    pub fn new(width: u32, height: u32, rgba: &'a [u8]) -> Option<Self> {
        let want = u64::from(width)
            .checked_mul(u64::from(height))?
            .checked_mul(4)?;
        (rgba.len() as u64 == want).then_some(Self {
            width,
            height,
            rgba,
        })
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// The raw bytes.
    #[must_use]
    pub const fn rgba(&self) -> &'a [u8] {
        self.rgba
    }

    /// The pixel at `(x, y)` as `[r, g, b, a]`, or `None` outside the image.
    #[must_use]
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (u64::from(y) * u64::from(self.width) + u64::from(x)) as usize * 4;
        self.rgba.get(i..i + 4)?.try_into().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_must_match() {
        assert!(Image::new(2, 2, &[0; 16]).is_some());
        assert!(Image::new(2, 2, &[0; 15]).is_none());
        assert!(Image::new(0, 0, &[]).is_some());
        assert!(
            Image::new(u32::MAX, u32::MAX, &[]).is_none(),
            "no overflow on huge sizes"
        );
    }

    #[test]
    fn pixels_are_read_row_by_row() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let img = Image::new(2, 2, &data).unwrap();
        assert_eq!(img.pixel(0, 0), Some([1, 2, 3, 4]));
        assert_eq!(img.pixel(1, 0), Some([5, 6, 7, 8]));
        assert_eq!(img.pixel(0, 1), Some([9, 10, 11, 12]));
        assert_eq!(img.pixel(2, 0), None);
        assert_eq!(img.pixel(0, 2), None);
    }
}
