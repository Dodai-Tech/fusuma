//! Text style. A backend picks the font; `size` is the line height in pixels it should aim for.

use crate::color::Color;

/// Horizontal alignment relative to the anchor point of `draw_text`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    /// The anchor is the left edge.
    #[default]
    Left,
    /// The anchor is the center.
    Center,
    /// The anchor is the right edge.
    Right,
}

/// Which font a run of text asks for. A backend that has no vector fonts (or not this one) draws its built-in
/// bitmap font instead, so asking is always safe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Font {
    /// The UI font (Inter when the `inter` font is bundled).
    #[default]
    Sans,
    /// The bold face of the UI font.
    SansBold,
    /// The fixed-width font (JetBrains Mono when bundled), for logs and code.
    Mono,
    /// The built-in bitmap font, on every backend and with no font data: three monospace sizes.
    Bitmap,
}

/// How to draw a run of text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextStyle {
    /// Requested line height in pixels; backends use the closest font they have.
    pub size: u32,
    /// Text color.
    pub color: Color,
    /// Alignment around the anchor.
    pub align: Align,
    /// Which font.
    pub font: Font,
}

impl TextStyle {
    /// Left-aligned text of `size` pixels in `color`.
    #[must_use]
    pub const fn new(size: u32, color: Color) -> Self {
        Self {
            size,
            color,
            align: Align::Left,
            font: Font::Sans,
        }
    }

    /// The same style in another font.
    #[must_use]
    pub const fn font(self, font: Font) -> Self {
        Self { font, ..self }
    }

    /// The same style with another alignment.
    #[must_use]
    pub const fn aligned(self, align: Align) -> Self {
        Self { align, ..self }
    }
}

/// The left edge of a run `width` pixels wide for an anchor at `x`.
#[must_use]
pub fn left_edge(x: i32, width: u32, align: Align) -> i32 {
    let w = i64::from(width);
    let dx = match align {
        Align::Left => 0,
        Align::Center => w / 2,
        Align::Right => w,
    };
    (i64::from(x) - dx).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_moves_the_left_edge() {
        assert_eq!(left_edge(100, 40, Align::Left), 100);
        assert_eq!(left_edge(100, 40, Align::Center), 80);
        assert_eq!(left_edge(100, 40, Align::Right), 60);
        assert_eq!(left_edge(i32::MIN, 40, Align::Right), i32::MIN);
    }

    #[test]
    fn style_builders() {
        let s = TextStyle::new(16, Color::WHITE).aligned(Align::Center);
        assert_eq!((s.size, s.align), (16, Align::Center));
        assert_eq!(TextStyle::new(8, Color::BLACK).align, Align::Left);
    }

    #[test]
    fn a_style_defaults_to_the_sans_font_and_can_change_it() {
        let s = TextStyle::new(20, Color::WHITE);
        assert_eq!(s.font, Font::Sans);
        assert_eq!(s.font(Font::Mono).font, Font::Mono);
        assert_eq!(s.font(Font::Bitmap).size, 20, "the rest is kept");
        assert_eq!(Font::default(), Font::Sans);
    }
}
