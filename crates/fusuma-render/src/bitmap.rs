//! The built-in bitmap fonts: three monospace sizes that need no allocator, no font file and no Unicode tables. Parley gives real
//! text where there is room for it; everywhere else, and for the first frame, every backend uses these, so text measures and
//! draws the same everywhere.
//!
//! The characters are ISO 8859-1 (Latin-1: `é`, `ü`, `ñ`, `ß`, `£`, `¿`, ...). With the `bitmap-extended` feature a character that
//! is not in it is looked for in ISO 8859-16 (Romanian `ș ț ă â î`, Polish, Hungarian, Croatian), then 8859-2 (Czech, Slovak, ...),
//! 8859-5 (Cyrillic) and 8859-7 (Greek), one character at a time; every font of a size is as wide as the others, so a text is as
//! wide as it has characters whichever fonts they come from. Anything else is a `?`.

use embedded_graphics::mono_font::{
    MonoFont,
    iso_8859_1::{FONT_6X10, FONT_9X15, FONT_10X20},
};

use crate::geometry::Size;

/// The font that comes closest to a requested line height (Latin-1).
#[must_use]
pub fn font_for(size: u32) -> &'static MonoFont<'static> {
    match size {
        0..=12 => &FONT_6X10,
        13..=17 => &FONT_9X15,
        _ => &FONT_10X20,
    }
}

/// The font of the size that draws `c`: the Latin-1 one, or with `bitmap-extended` the first of the other encodings that has
/// it. A character that no font has is drawn by the Latin-1 font as its `?`.
#[must_use]
pub fn font_for_char(size: u32, c: char) -> &'static MonoFont<'static> {
    let base = font_for(size);
    #[cfg(feature = "bitmap-extended")]
    {
        use embedded_graphics::mono_font::{
            iso_8859_2, iso_8859_5, iso_8859_7, iso_8859_16, mapping,
        };
        if !mapping::ISO_8859_1.contains(c) {
            let (small, mid, big) = (size <= 12, (13..=17).contains(&size), size > 17);
            let pick = |a: &'static MonoFont<'static>,
                        b: &'static MonoFont<'static>,
                        d: &'static MonoFont<'static>| {
                if small {
                    a
                } else if mid {
                    b
                } else {
                    debug_assert!(big);
                    d
                }
            };
            if mapping::ISO_8859_16.contains(c) {
                return pick(
                    &iso_8859_16::FONT_6X10,
                    &iso_8859_16::FONT_9X15,
                    &iso_8859_16::FONT_10X20,
                );
            }
            if mapping::ISO_8859_2.contains(c) {
                return pick(
                    &iso_8859_2::FONT_6X10,
                    &iso_8859_2::FONT_9X15,
                    &iso_8859_2::FONT_10X20,
                );
            }
            if mapping::ISO_8859_5.contains(c) {
                return pick(
                    &iso_8859_5::FONT_6X10,
                    &iso_8859_5::FONT_9X15,
                    &iso_8859_5::FONT_10X20,
                );
            }
            if mapping::ISO_8859_7.contains(c) {
                return pick(
                    &iso_8859_7::FONT_6X10,
                    &iso_8859_7::FONT_9X15,
                    &iso_8859_7::FONT_10X20,
                );
            }
        }
    }
    let _ = c;
    base
}

/// A piece of a text that one font draws: the characters and the font.
#[derive(Clone, Copy, Debug)]
pub struct Run<'a> {
    /// The characters.
    pub text: &'a str,
    /// The font that draws them.
    pub font: &'static MonoFont<'static>,
    /// How many characters come before this run in the text.
    pub before: u32,
}

/// `text` cut into runs of one font each, for a line height of `size`. A text of one encoding (nearly all of them) is one run.
pub fn runs(text: &str, size: u32) -> impl Iterator<Item = Run<'_>> {
    let mut start = 0usize;
    let mut before = 0u32;
    core::iter::from_fn(move || {
        if start >= text.len() {
            return None;
        }
        let rest = &text[start..];
        let mut chars = rest.char_indices();
        let (_, first) = chars.next()?;
        let font = font_for_char(size, first);
        let mut end = first.len_utf8();
        let mut count = 1u32;
        for (i, c) in chars {
            if !core::ptr::eq(font_for_char(size, c), font) {
                break;
            }
            end = i + c.len_utf8();
            count += 1;
        }
        let run = Run {
            text: &rest[..end],
            font,
            before,
        };
        start += end;
        before += count;
        Some(run)
    })
}

/// The largest whole-number magnification of the bitmap font.
pub const MAX_SCALE: u32 = 8;

/// How many times the bitmap font is magnified for a requested line height. Up to 29 pixels the nearest of the three fonts is
/// used as it is; above that the 10x20 font is scaled by the whole number nearest to `size / 20` (a 40 pixel line is the 10x20
/// font at 2x), so a theme scaled for a 2x screen has larger text.
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

/// The size of `text` in `font`, in pixels: characters, not bytes, side by side. (Every font of a size is as wide as the others.)
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
    extern crate std;
    use std::vec::Vec;

    use super::*;

    #[test]
    fn the_font_follows_the_requested_height_and_large_text_magnifies_by_a_whole_number() {
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
    fn text_is_measured_in_characters_not_bytes_for_every_script() {
        let f = font_for(20);
        assert_eq!(text_size(f, "abc"), Size::new(30, 20));
        assert_eq!(text_size(f, ""), Size::new(0, 0));
        assert_eq!(text_size(f, "é"), Size::new(10, 20));
        assert_eq!(
            text_size(f, "日本語"),
            Size::new(30, 20),
            "three characters, whatever they are drawn as"
        );
        assert_eq!(text_size(font_for(10), "ab"), Size::new(12, 10));
        let long = "x".repeat(10_000);
        assert_eq!(text_size(f, &long).w, 100_000);
    }

    #[test]
    fn latin_1_text_has_its_own_glyphs_and_is_one_run() {
        for c in "éèêëüöäßñçÅøþ£¿¡©".chars() {
            assert!(core::ptr::eq(font_for_char(20, c), font_for(20)), "{c}");
        }
        let r: Vec<_> = runs("Café Zürich ñ", 14).collect();
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].text, r[0].before), ("Café Zürich ñ", 0));
        assert_eq!(runs("", 14).count(), 0);
        // The é is a real glyph and not the question mark.
        let font = font_for(20);
        assert_ne!(font.glyph_mapping.index('é'), font.glyph_mapping.index('?'));
        assert_ne!(font.glyph_mapping.index('ü'), font.glyph_mapping.index('?'));
    }

    #[cfg(feature = "bitmap-extended")]
    #[test]
    fn extended_characters_come_from_the_encoding_that_has_them_one_character_at_a_time() {
        let same =
            |a: &'static MonoFont<'static>, b: &'static MonoFont<'static>| core::ptr::eq(a, b);
        // Romanian (â and î are Latin-1 already) is in 8859-16, Cyrillic in 8859-5, Greek in 8859-7; none of them is the Latin-1 font.
        for c in "șțăȘȚ".chars() {
            assert!(!same(font_for_char(14, c), font_for(14)), "{c}");
        }
        assert!(!same(font_for_char(14, 'Ж'), font_for(14)));
        assert!(!same(font_for_char(14, 'λ'), font_for(14)));
        assert!(
            same(font_for_char(14, 'a'), font_for(14)),
            "ASCII stays Latin-1"
        );
        // All three sizes work, and each size's fonts are as high as the Latin-1 one.
        for size in [8, 14, 20] {
            for c in ['ș', 'Ж', 'λ'] {
                assert_eq!(
                    font_for_char(size, c).character_size,
                    font_for(size).character_size
                );
            }
        }
        // A mixed text is cut into runs where the font changes, and the runs add up to the text.
        let text = "aș bЖ cλ ok";
        let r: Vec<_> = runs(text, 14).collect();
        assert!(r.len() >= 5, "{} runs", r.len());
        assert_eq!(r.iter().map(|x| x.text).collect::<Vec<_>>().concat(), text);
        let mut count = 0;
        for run in &r {
            assert_eq!(run.before, count);
            count += run.text.chars().count() as u32;
        }
        // Something no font has stays a question mark (Latin-1's).
        assert!(same(font_for_char(14, '日'), font_for(14)));
    }
}
