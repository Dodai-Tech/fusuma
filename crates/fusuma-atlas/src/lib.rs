//! Glyph atlases: the types a font or an icon set rasterised at build time is stored in, for a renderer that draws text
//! by blending coverage rather than filling outlines. No allocator, no floating point.
#![no_std]

/// One glyph: where its coverage is and how it sits on the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasGlyph {
    /// The character, as a code point.
    pub ch: u32,
    /// How far the pen moves after it, in 1/64 pixel.
    pub advance: u16,
    /// Its left edge from the pen, in pixels.
    pub left: i8,
    /// Its top edge from the top of the line, in pixels.
    pub top: i8,
    /// Width of its coverage in pixels (0 for a space).
    pub width: u8,
    /// Height of its coverage in pixels.
    pub height: u8,
    /// Where its coverage starts in [`Atlas::coverage`].
    pub at: u32,
}

/// The kerning of a face: how much closer or further apart two characters sit than their advances say.
#[derive(Clone, Copy, Debug)]
pub struct Kerning {
    /// The font's units per em, which the adjustments are in.
    pub upm: u16,
    /// `(first << 16 | second, adjustment)` for the printable ASCII pairs that have one, sorted by the key.
    pub pairs: &'static [(u32, i16)],
}

/// A face at one line height.
#[derive(Clone, Copy, Debug)]
pub struct Atlas {
    /// The family's id.
    pub family: &'static str,
    /// Whether it is the bold face.
    pub bold: bool,
    /// The line height in pixels: what `TextStyle::size` asks for.
    pub line: u16,
    /// The baseline, from the top of the line.
    pub baseline: u16,
    /// The font's em in 1/64 pixel at this line height (the scale of the kerning).
    pub em64: u16,
    /// The face's kerning.
    pub kern: &'static Kerning,
    /// The glyphs, sorted by character.
    pub glyphs: &'static [AtlasGlyph],
    /// Coverage, 4 bits a pixel (high nibble first), each glyph's rows padded to a whole byte.
    pub coverage: &'static [u8],
}

impl Atlas {
    /// The glyph of `ch`, if the atlas has it.
    #[must_use]
    pub fn glyph(&self, ch: char) -> Option<&AtlasGlyph> {
        let ch = ch as u32;
        self.glyphs
            .binary_search_by_key(&ch, |g| g.ch)
            .ok()
            .map(|i| &self.glyphs[i])
    }

    /// The glyph drawn for `ch`: its own, else a question mark (a character the subset does not have).
    #[must_use]
    pub fn glyph_or_replacement(&self, ch: char) -> Option<&AtlasGlyph> {
        self.glyph(ch).or_else(|| self.glyph('?'))
    }

    /// The coverage of glyph `g` at pixel `(x, y)` inside it, 0 to 15.
    #[must_use]
    pub fn coverage_at(&self, g: &AtlasGlyph, x: u32, y: u32) -> u8 {
        if x >= u32::from(g.width) || y >= u32::from(g.height) {
            return 0;
        }
        let stride = u32::from(g.width).div_ceil(2);
        let i = (g.at + y * stride + x / 2) as usize;
        let byte = self.coverage.get(i).copied().unwrap_or(0);
        if x.is_multiple_of(2) {
            byte >> 4
        } else {
            byte & 0x0f
        }
    }

    /// How far the pen moves before `second` when it follows `first`, in 1/64 pixel (negative: closer). The
    /// characters are the ones drawn, a missing one being the question mark.
    #[must_use]
    pub fn kern(&self, first: char, second: char) -> i32 {
        let drawn = |c: char| if self.glyph(c).is_some() { c } else { '?' };
        let (a, b) = (drawn(first), drawn(second));
        if a as u32 > 0xffff || b as u32 > 0xffff || self.kern.pairs.is_empty() {
            return 0;
        }
        let key = (a as u32) << 16 | b as u32;
        let Ok(i) = self.kern.pairs.binary_search_by_key(&key, |p| p.0) else {
            return 0;
        };
        i32::from(self.kern.pairs[i].1) * i32::from(self.em64) / i32::from(self.kern.upm.max(1))
    }

    /// The width of `text` in pixels, rounded up: the pen's travel, kerning included.
    #[must_use]
    pub fn width(&self, text: &str) -> u32 {
        let mut total: i64 = 0;
        let mut prev: Option<char> = None;
        for c in text.chars() {
            let Some(g) = self.glyph_or_replacement(c) else {
                continue;
            };
            if let Some(p) = prev {
                total += i64::from(self.kern(p, c));
            }
            total += i64::from(g.advance);
            prev = Some(c);
        }
        (total.max(0) as u32).div_ceil(64)
    }

    /// The atlas of `family` (bold or not) that best fits a line of `size` pixels: the largest not above it, else the
    /// smallest.
    #[must_use]
    pub fn pick(
        atlases: &'static [Atlas],
        family: &str,
        bold: bool,
        size: u32,
    ) -> Option<&'static Atlas> {
        let of = || {
            atlases
                .iter()
                .filter(move |a| a.family == family && a.bold == bold)
        };
        of().filter(|a| u32::from(a.line) <= size)
            .max_by_key(|a| a.line)
            .or_else(|| of().min_by_key(|a| a.line))
    }
}
