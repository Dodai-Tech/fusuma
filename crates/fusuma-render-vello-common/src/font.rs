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

/// The largest whole-number magnification of the bitmap font.
pub const MAX_SCALE: u32 = 8;

/// How many times the bitmap font is magnified for a requested line height. Up to 20 pixels the nearest of the
/// three fonts is used as it is; above that the 10x20 font is scaled by the whole number nearest to
/// `size / 20` (a 40 pixel line is the 10x20 font at 2x), so a theme scaled for a 2x screen has larger text.
#[must_use]
pub fn text_scale(size: u32) -> u32 {
    if size <= 29 {
        1
    } else {
        ((size.saturating_mul(2).saturating_add(20)) / 40).clamp(1, MAX_SCALE)
    }
}

/// The size of `text` at a requested line height: the font's size, times its magnification.
#[must_use]
pub fn scaled_text_size(size: u32, text: &str) -> Size {
    let s = text_size(font_for(size), text);
    let k = text_scale(size);
    Size::new(s.w.saturating_mul(k), s.h.saturating_mul(k))
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
    fn large_text_magnifies_the_biggest_font_by_a_whole_number() {
        for (size, k) in [
            (0, 1),
            (20, 1),
            (29, 1),
            (30, 2),
            (40, 2),
            (49, 2),
            (50, 3),
            (160, 8),
            (u32::MAX, 8),
        ] {
            assert_eq!(text_scale(size), k, "size {size}");
        }
        assert_eq!(scaled_text_size(20, "abc"), Size::new(30, 20));
        assert_eq!(scaled_text_size(40, "abc"), Size::new(60, 40));
        assert_eq!(scaled_text_size(40, ""), Size::new(0, 0));
        assert_eq!(scaled_text_size(u32::MAX, "ab"), Size::new(160, 160));
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
