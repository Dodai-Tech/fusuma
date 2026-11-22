//! Colors: 8-bit RGBA (straight, not premultiplied) and RGB565 conversion for small displays.

/// An RGBA color with 8 bits per channel; alpha 255 is opaque.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (opacity).
    pub a: u8,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    /// Fully transparent.
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);

    /// An opaque color.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A color with alpha.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The same color with another alpha.
    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Whether the color is fully opaque.
    #[must_use]
    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }

    /// The nearest RGB565 value (alpha is ignored).
    #[must_use]
    pub const fn to_rgb565(self) -> u16 {
        ((self.r as u16 >> 3) << 11) | ((self.g as u16 >> 2) << 5) | (self.b as u16 >> 3)
    }

    /// An opaque color from RGB565, with the low bits filled so white stays white.
    #[must_use]
    pub const fn from_rgb565(v: u16) -> Self {
        let (r, g, b) = ((v >> 11) & 0x1f, (v >> 5) & 0x3f, v & 0x1f);
        Self::rgb(
            ((r << 3) | (r >> 2)) as u8,
            ((g << 2) | (g >> 4)) as u8,
            ((b << 3) | (b >> 2)) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_and_alpha() {
        assert!(Color::WHITE.is_opaque());
        assert!(!Color::TRANSPARENT.is_opaque());
        assert_eq!(Color::rgb(1, 2, 3).with_alpha(9), Color::rgba(1, 2, 3, 9));
    }

    #[test]
    fn rgb565_keeps_black_and_white_and_rounds_trips_within_a_step() {
        assert_eq!(Color::WHITE.to_rgb565(), 0xFFFF);
        assert_eq!(Color::BLACK.to_rgb565(), 0);
        assert_eq!(Color::from_rgb565(0xFFFF), Color::WHITE);
        assert_eq!(Color::from_rgb565(0), Color::BLACK);
        for v in [0x0000u16, 0x1234, 0xABCD, 0xF800, 0x07E0, 0x001F, 0xFFFF] {
            assert_eq!(Color::from_rgb565(v).to_rgb565(), v, "{v:#x}");
        }
        let c = Color::rgb(200, 100, 50);
        let back = Color::from_rgb565(c.to_rgb565());
        assert!(
            c.r.abs_diff(back.r) <= 8 && c.g.abs_diff(back.g) <= 4 && c.b.abs_diff(back.b) <= 8
        );
    }
}
