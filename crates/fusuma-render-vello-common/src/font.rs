//! The built-in bitmap fonts: three monospace sizes that need no allocator, no font file and no Unicode
//! tables. Parley will give real text on desktop and phone; until then every backend uses these, so text
//! measures and draws the same everywhere.

use embedded_graphics::mono_font::{
    MonoFont,
    ascii::{FONT_6X10, FONT_9X15, FONT_10X20},
};
use fusuma_render::Size;

/// The font that comes closest to a requested line height.
#[must_use]
pub fn font_for(size: u32) -> &'static MonoFont<'static> {
    match size {
        0..=12 => &FONT_6X10,
        13..=17 => &FONT_9X15,
        _ => &FONT_10X20,
    }
}

/// The size of `text` in `font`, in pixels: characters, not bytes, side by side.
#[must_use]
pub fn text_size(font: &MonoFont<'_>, text: &str) -> Size {
    let n = text.chars().count() as u32;
    if n == 0 {
        return Size::new(0, 0);
    }
    let w = n * font.character_size.width + (n - 1) * font.character_spacing;
    Size::new(w, font.character_size.height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_font_follows_the_requested_height() {
        for (size, height) in [
            (0, 10),
            (12, 10),
            (13, 15),
            (17, 15),
            (18, 20),
            (u32::MAX, 20),
        ] {
            assert_eq!(font_for(size).character_size.height, height, "size {size}");
        }
    }

    #[test]
    fn text_is_measured_in_characters_not_bytes() {
        assert_eq!(text_size(&FONT_10X20, "abc"), Size::new(30, 20));
        assert_eq!(text_size(&FONT_10X20, ""), Size::new(0, 0));
        assert_eq!(text_size(&FONT_10X20, "é"), Size::new(10, 20));
        assert_eq!(text_size(&FONT_6X10, "ab"), Size::new(12, 10));
    }

    #[test]
    fn a_long_text_does_not_overflow_the_measure_of_a_reasonable_string() {
        let long = "x".repeat(10_000);
        assert_eq!(text_size(&FONT_10X20, &long).w, 100_000);
    }
}
