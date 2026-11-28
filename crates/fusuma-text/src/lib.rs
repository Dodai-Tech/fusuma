#![doc = include_str!("../README.md")]
#![no_std]

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;

pub use fusuma_fonts::{Face, FontSet};
use fusuma_render::{Font, Size};
use parley::fontique::Blob;
use parley::{
    FontContext, FontFamily, FontWeight, LayoutContext, PositionedLayoutItem, StyleProperty,
};

pub use parley::FontData;

/// How many measurements and layouts are remembered. Layout asks for the same few strings every frame, so a
/// small cache removes nearly all the work; it forgets the oldest first.
const CACHE: usize = 48;

/// A glyph, placed relative to the top left of its text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// The glyph's index in its font.
    pub id: u32,
    /// Distance from the left of the text, in pixels.
    pub x: f32,
    /// Distance of the baseline from the top of the text, in pixels.
    pub y: f32,
}

/// A run of glyphs in one font at one size.
#[derive(Clone, Debug)]
pub struct Run {
    /// The font data.
    pub font: FontData,
    /// The font size in pixels per em.
    pub size: f32,
    /// The glyphs.
    pub glyphs: Vec<Glyph>,
}

/// Text laid out on one line: its size and what to draw.
#[derive(Clone, Debug)]
pub struct Laid {
    /// The width and the line height, in whole pixels.
    pub size: Size,
    /// The glyph runs.
    pub runs: Vec<Run>,
}

/// `v` rounded up to a whole pixel, at least 0. `f32::ceil` is not in `core`, so this does it by hand.
fn ceil_px(v: f32) -> u32 {
    if v.is_nan() || v <= 0.0 {
        return 0;
    }
    let whole = v as u32;
    if (whole as f32) < v {
        whole.saturating_add(1)
    } else {
        whole
    }
}

/// What a cache entry is the answer to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Key {
    text: String,
    font: Font,
    size: u32,
}

/// Fonts registered, text laid out and measured, with a cache. One per renderer (it is not shared).
pub struct TextEngine {
    fonts: FontContext,
    layouts: LayoutContext<()>,
    /// Which linked family fills each role.
    set: FontSet,
    cache: Vec<(Key, Option<Laid>)>,
}

impl core::fmt::Debug for TextEngine {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TextEngine")
            .field("set", &self.set)
            .field("cached", &self.cache.len())
            .finish_non_exhaustive()
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// The face a font role asks for; the bitmap font is not this engine's, and the icons are `fusuma-icons`'.
const fn face(font: Font) -> Option<Face> {
    match font {
        Font::Sans => Some(Face::Sans),
        Font::SansBold => Some(Face::SansBold),
        Font::Mono => Some(Face::Mono),
        Font::Icons | Font::Bitmap => None,
    }
}

impl TextEngine {
    /// An engine with every font file the build linked registered, drawing the default set
    /// ([`FontSet::default_set`]). The font data is used where it is (it is `'static`), not copied.
    #[must_use]
    pub fn new() -> Self {
        let mut fonts = FontContext::new();
        for f in fusuma_fonts::files() {
            let blob: Blob<u8> = Blob::new(Arc::new(f.data));
            fonts.collection.register_fonts(blob, None);
        }
        #[cfg(feature = "icons")]
        fonts
            .collection
            .register_fonts(Blob::new(Arc::new(fusuma_icons::FONT)), None);
        Self {
            fonts,
            layouts: LayoutContext::new(),
            set: FontSet::default_set(),
            cache: Vec::new(),
        }
    }

    /// Draws the roles with `set`'s families from now on.
    pub fn set_fonts(&mut self, set: FontSet) {
        if set != self.set {
            self.set = set;
            self.cache.clear();
        }
    }

    /// The families the roles are drawn with.
    #[must_use]
    pub const fn fonts(&self) -> &FontSet {
        &self.set
    }

    /// The family name and weight that draw `font`, if a family fills its role.
    fn spec(&self, font: Font) -> Option<(&'static str, f32)> {
        #[cfg(feature = "icons")]
        if font == Font::Icons {
            return Some((fusuma_icons::FONT_FAMILY, 400.0));
        }
        let (family, file) = self.set.file(face(font)?)?;
        Some((family.name, if file.bold { 700.0 } else { 400.0 }))
    }

    /// Whether `font` can be laid out here: a bitmap font never can (a backend draws that itself), and a
    /// vector font only when its cargo feature bundled it.
    #[must_use]
    pub fn has(&self, font: Font) -> bool {
        self.spec(font).is_some()
    }

    /// The font size that makes the line `size` pixels high: Inter and JetBrains Mono have a line height of
    /// about 1.21 em, so a 20 pixel line is a 16.5 pixel font.
    fn em(size: u32) -> f32 {
        size as f32 * 100.0 / 121.0
    }

    /// `text` laid out on one line in `font` with a line `size` pixels high; `None` when the font is not
    /// available, for the caller to use its bitmap font. Cached.
    pub fn layout(&mut self, text: &str, font: Font, size: u32) -> Option<&Laid> {
        let (family, weight) = self.spec(font)?;
        if size == 0 {
            return None;
        }
        let key = Key {
            text: text.into(),
            font,
            size,
        };
        let at = match self.cache.iter().position(|(k, _)| *k == key) {
            Some(i) => i,
            None => {
                let laid = self.build(text, family, weight, size, font == Font::Icons);
                if self.cache.len() >= CACHE {
                    self.cache.remove(0);
                }
                self.cache.push((key, laid));
                self.cache.len() - 1
            }
        };
        self.cache[at].1.as_ref()
    }

    /// The size of `text` in `font` at a line height of `size`; `None` as for [`layout`](Self::layout).
    pub fn measure(&mut self, text: &str, font: Font, size: u32) -> Option<Size> {
        self.layout(text, font, size).map(|l| l.size)
    }

    /// Lays `text` out. Icons (`icons`) fill a square as high as the line: their em is the line and their baseline
    /// its bottom, as their font draws them (Lucide stands on the baseline, one em high).
    fn build(
        &mut self,
        text: &str,
        family: &'static str,
        weight: f32,
        size: u32,
        icons: bool,
    ) -> Option<Laid> {
        let em = if icons { size as f32 } else { Self::em(size) };
        let mut b = self
            .layouts
            .ranged_builder(&mut self.fonts, text, 1.0, true);
        b.push_default(StyleProperty::FontSize(em));
        b.push_default(StyleProperty::FontFamily(FontFamily::named(family)));
        b.push_default(StyleProperty::FontWeight(FontWeight::new(weight)));
        let mut layout = b.build(text);
        layout.break_all_lines(None);
        let mut runs = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let mut x = run.offset();
                let y = if icons { size as f32 } else { run.baseline() };
                let glyphs = run
                    .glyphs()
                    .map(|g| {
                        let at = Glyph {
                            id: g.id,
                            x: x + g.x,
                            y: y - g.y,
                        };
                        x += g.advance;
                        at
                    })
                    .collect();
                runs.push(Run {
                    font: run.run().font().clone(),
                    size: run.run().font_size(),
                    glyphs,
                });
            }
        }
        // Whole pixels, rounded up so the text never sticks out of the box it was measured for; the line is
        // never shorter than asked, so a layout in `size` pixel lines stacks like the bitmap font's.
        let width = ceil_px(layout.width());
        // An icon's line is its square, whatever the font's ascent and descent say.
        let height = if icons {
            size
        } else {
            ceil_px(layout.height()).max(size)
        };
        Some(Laid {
            size: Size::new(width, height),
            runs,
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn engine() -> TextEngine {
        TextEngine::new()
    }

    #[test]
    fn the_bundled_fonts_are_available_and_the_bitmap_font_is_not_this_engines_business() {
        let e = engine();
        assert!(e.has(Font::Sans) && e.has(Font::SansBold) && e.has(Font::Mono));
        assert!(!e.has(Font::Bitmap));
    }

    #[test]
    fn text_is_proportional_and_wider_when_there_is_more_of_it() {
        let mut e = engine();
        let w = |e: &mut TextEngine, s: &str| e.measure(s, Font::Sans, 20).unwrap().w;
        assert!(
            w(&mut e, "iiii") < w(&mut e, "WWWW"),
            "an i is narrower than a W"
        );
        assert!(w(&mut e, "abcdef") > w(&mut e, "abc"));
        assert_eq!(w(&mut e, ""), 0);
    }

    #[test]
    fn the_line_is_at_least_as_high_as_asked_and_grows_with_the_size() {
        let mut e = engine();
        for size in [12, 20, 40] {
            let h = e.measure("Hxg", Font::Sans, size).unwrap().h;
            assert!(
                h >= size && h <= size + size / 4 + 2,
                "size {size}: height {h}"
            );
        }
        let (small, big) = (
            e.measure("Hello", Font::Sans, 20).unwrap(),
            e.measure("Hello", Font::Sans, 40).unwrap(),
        );
        assert!(
            big.w > small.w * 3 / 2 && big.w < small.w * 5 / 2,
            "{small:?} {big:?}"
        );
    }

    #[test]
    fn romanian_letters_become_real_glyphs_not_the_missing_glyph() {
        let mut e = engine();
        let laid = e.layout("ăâîșț ĂÂÎȘȚ", Font::Sans, 24).unwrap().clone();
        let glyphs: Vec<u32> = laid
            .runs
            .iter()
            .flat_map(|r| r.glyphs.iter().map(|g| g.id))
            .collect();
        assert!(glyphs.len() >= 10, "{} glyphs", glyphs.len());
        assert!(
            !glyphs.contains(&0),
            "glyph 0 is .notdef, the missing glyph: {glyphs:?}"
        );
    }

    #[test]
    fn bold_is_a_different_face_and_wider_than_regular() {
        let mut e = engine();
        let reg = e.measure("Settings", Font::Sans, 24).unwrap().w;
        let bold = e.measure("Settings", Font::SansBold, 24).unwrap().w;
        assert!(bold > reg, "{bold} against {reg}");
        let r = e.layout("Settings", Font::Sans, 24).unwrap().runs[0]
            .font
            .clone();
        let b = e.layout("Settings", Font::SansBold, 24).unwrap().runs[0]
            .font
            .clone();
        assert_ne!(r.data.id(), b.data.id(), "two font files");
    }

    #[test]
    fn the_mono_font_gives_every_character_the_same_width() {
        let mut e = engine();
        let w = |e: &mut TextEngine, s: &str| e.measure(s, Font::Mono, 20).unwrap().w;
        assert_eq!(w(&mut e, "iiii"), w(&mut e, "WWWW"));
        assert_eq!(w(&mut e, "abcdefgh"), 2 * w(&mut e, "abcd"));
    }

    #[test]
    fn a_role_without_a_family_is_refused_so_the_caller_uses_the_bitmap_font() {
        let mut e = engine();
        e.set_fonts(FontSet {
            mono: None,
            ..FontSet::default_set()
        });
        assert!(e.layout("x", Font::Mono, 20).is_none());
        assert!(e.measure("x", Font::Mono, 20).is_none());
        assert!(e.measure("x", Font::Sans, 20).is_some());
    }

    #[test]
    fn another_family_lays_text_out_in_its_own_glyphs_and_widths() {
        let mut e = engine();
        let inter = e.measure("Settings and more", Font::Sans, 24).unwrap();
        let font_of = |e: &mut TextEngine| {
            e.layout("Settings", Font::Sans, 24).unwrap().runs[0]
                .font
                .data
                .id()
        };
        let inter_file = font_of(&mut e);
        e.set_fonts(FontSet::choose(None, Some("roboto"), None).unwrap());
        let roboto = e.measure("Settings and more", Font::Sans, 24).unwrap();
        assert_ne!(font_of(&mut e), inter_file, "another file");
        assert_ne!(
            roboto.w, inter.w,
            "another width: Roboto is narrower than Inter"
        );
        assert_eq!(e.fonts().sans.unwrap().name, "Roboto");
    }

    #[test]
    fn pixels_round_up_and_odd_numbers_are_harmless() {
        assert_eq!(ceil_px(0.0), 0);
        assert_eq!(ceil_px(0.2), 1);
        assert_eq!(ceil_px(3.0), 3);
        assert_eq!(ceil_px(3.01), 4);
        assert_eq!(ceil_px(-5.0), 0);
        assert_eq!(ceil_px(f32::NAN), 0);
        assert_eq!(ceil_px(f32::INFINITY), u32::MAX);
    }

    #[test]
    fn a_bitmap_request_and_a_size_of_zero_are_refused() {
        let mut e = engine();
        assert!(e.layout("x", Font::Bitmap, 20).is_none());
        assert!(e.layout("x", Font::Sans, 0).is_none());
    }

    #[test]
    fn a_repeated_request_is_answered_from_the_cache_and_the_cache_is_bounded() {
        let mut e = engine();
        let a = e.layout("same", Font::Sans, 20).unwrap().size;
        let again = e.layout("same", Font::Sans, 20).unwrap().size;
        assert_eq!(a, again);
        assert_eq!(e.cache.len(), 1);
        for i in 0..(CACHE * 3) {
            let s = std::format!("text {i}");
            assert!(e.layout(&s, Font::Sans, 20).is_some());
        }
        assert_eq!(e.cache.len(), CACHE, "the oldest are forgotten");
        // A different size or font is a different entry.
        e.layout("same", Font::Sans, 21).unwrap();
        assert_eq!(e.cache.len(), CACHE);
    }

    #[test]
    fn glyph_positions_run_left_to_right_from_the_origin_on_a_common_baseline() {
        let mut e = engine();
        let laid = e.layout("Hello", Font::Sans, 30).unwrap().clone();
        let g = &laid.runs[0].glyphs;
        assert_eq!(g.len(), 5);
        assert!(g.windows(2).all(|w| w[0].x < w[1].x), "{g:?}");
        assert!(
            g.iter()
                .all(|p| p.y == g[0].y && p.y > 0.0 && p.y < laid.size.h as f32)
        );
        assert!(g[0].x >= 0.0 && g[4].x < laid.size.w as f32);
    }

    #[test]
    fn spaces_control_characters_and_long_text_do_not_break_the_engine() {
        let mut e = engine();
        assert!(e.measure("   ", Font::Sans, 20).is_some());
        assert!(e.measure("a\tb\u{0}c\n", Font::Sans, 20).is_some());
        let long = "w".repeat(2000);
        let size = e.measure(&long, Font::Sans, 20).unwrap();
        assert!(size.w > 10_000);
    }
}
