//! How many columns a character takes on a terminal: 0 for a combining mark or a zero-width character, 2 for a wide
//! one (East Asian wide and fullwidth forms, emoji), else 1. A table of the ranges that matter in practice, not the
//! whole of Unicode.

/// Columns `c` takes: 0, 1 or 2.
#[must_use]
pub const fn char_width(c: char) -> u8 {
    let u = c as u32;
    // Control characters are shown as a space, one column.
    if u < 0x20 || (u >= 0x7f && u < 0xa0) {
        return 1;
    }
    // Combining marks, zero-width space and joiners, variation selectors.
    if (u >= 0x0300 && u <= 0x036f)
        || (u >= 0x200b && u <= 0x200f)
        || (u >= 0x2060 && u <= 0x2064)
        || (u >= 0xfe00 && u <= 0xfe0f)
        || u == 0xfeff
    {
        return 0;
    }
    if (u >= 0x1100 && u <= 0x115f)
        || u == 0x2329
        || u == 0x232a
        || (u >= 0x2e80 && u <= 0x303e)
        || (u >= 0x3041 && u <= 0x33ff)
        || (u >= 0x3400 && u <= 0x4dbf)
        || (u >= 0x4e00 && u <= 0x9fff)
        || (u >= 0xa000 && u <= 0xa4cf)
        || (u >= 0xac00 && u <= 0xd7a3)
        || (u >= 0xf900 && u <= 0xfaff)
        || (u >= 0xfe30 && u <= 0xfe6f)
        || (u >= 0xff00 && u <= 0xff60)
        || (u >= 0xffe0 && u <= 0xffe6)
        || (u >= 0x1f300 && u <= 0x1f64f)
        || (u >= 0x1f900 && u <= 0x1f9ff)
        || (u >= 0x20000 && u <= 0x3fffd)
    {
        return 2;
    }
    1
}

/// Columns `text` takes.
#[must_use]
pub fn str_width(text: &str) -> usize {
    text.chars().map(|c| usize::from(char_width(c))).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrow_wide_and_zero_width_characters() {
        for c in ['a', 'Z', ' ', 'é', 'ș', '█', '│', '•', '►', '\n'] {
            assert_eq!(char_width(c), 1, "{c:?}");
        }
        for c in ['日', '本', '한', 'Ａ', '🙂', '😀', '，'] {
            assert_eq!(char_width(c), 2, "{c:?}");
        }
        for c in ['\u{301}', '\u{200b}', '\u{fe0f}'] {
            assert_eq!(char_width(c), 0, "{c:?}");
        }
        assert_eq!(str_width("héllo"), 5);
        assert_eq!(str_width("日本語"), 6);
        assert_eq!(
            str_width("e\u{301}"),
            1,
            "a letter and its accent are one column"
        );
        assert_eq!(str_width(""), 0);
    }
}
