//! Glyph atlases: the bundled fonts rasterised at build time (feature `atlas`), for a renderer that draws text by
//! blending coverage rather than filling outlines. No allocator, no floating point.

use crate::{Face, FontSet};

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

    /// The width of `text` in pixels, rounded up: the pen's travel.
    #[must_use]
    pub fn width(&self, text: &str) -> u32 {
        let total: u32 = text
            .chars()
            .filter_map(|c| self.glyph_or_replacement(c))
            .map(|g| u32::from(g.advance))
            .sum();
        total.div_ceil(64)
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

    /// The atlas that draws `face` of `set` at a line of `size` pixels; `None` when the face has no family (draw
    /// the bitmap font). A sans without a bold face draws bold with its regular atlas.
    #[must_use]
    pub fn for_face(set: &FontSet, face: Face, size: u32) -> Option<&'static Atlas> {
        let (family, file) = set.file(face)?;
        Self::pick(ATLASES, family.id, file.bold, size)
    }
}

include!(concat!(env!("OUT_DIR"), "/atlas.rs"));

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    fn inter(line: u32) -> &'static Atlas {
        Atlas::pick(ATLASES, "inter", false, line).unwrap()
    }

    #[test]
    fn every_atlas_has_the_whole_coverage_sorted_and_inside_its_data() {
        assert!(!ATLASES.is_empty());
        for a in ATLASES {
            assert!(a.glyphs.windows(2).all(|w| w[0].ch < w[1].ch), "sorted");
            let family = crate::family(a.family).unwrap();
            if family.role == crate::Role::Icons {
                // Every icon, standing on the bottom of the line and inside its square.
                for (name, c) in family.icons {
                    let g = a
                        .glyph(*c)
                        .unwrap_or_else(|| panic!("{} {} has {name}", a.family, a.line));
                    assert!(
                        i32::from(g.top) >= 0
                            && i32::from(g.top) + i32::from(g.height) <= i32::from(a.line),
                        "{name}"
                    );
                    assert!(
                        g.width > 0 && u32::from(g.width) <= u32::from(a.line) + 1,
                        "{name}"
                    );
                }
                assert_eq!(
                    a.baseline, a.line,
                    "an icon's baseline is the bottom of the line"
                );
                continue;
            }
            // What every family must have; arrows and the like only if the font does.
            for c in ['A', 'z', '0', 'é', 'ș', 'ț', 'Ă', 'ă', '€', '…'] {
                assert!(a.glyph(c).is_some(), "{} {} has {c}", a.family, a.line);
            }
            for g in a.glyphs {
                let stride = u32::from(g.width).div_ceil(2);
                assert!((g.at + stride * u32::from(g.height)) as usize <= a.coverage.len());
            }
            assert!(
                u32::from(a.baseline) < u32::from(a.line),
                "the baseline is inside the line"
            );
        }
    }

    #[test]
    fn glyphs_sit_on_the_line_and_have_ink() {
        for line in [10, 15, 20] {
            let a = inter(line);
            assert_eq!(u32::from(a.line), line);
            let h = a.glyph('H').unwrap();
            // A capital stands on the baseline: its bottom row is the baseline (within a pixel).
            let bottom = i32::from(h.top) + i32::from(h.height);
            assert!(
                (bottom - i32::from(a.baseline)).abs() <= 1,
                "line {line}: H ends at {bottom}"
            );
            assert!(h.top >= 0, "and starts inside the line");
            let ink: u32 = (0..u32::from(h.height))
                .flat_map(|y| (0..u32::from(h.width)).map(move |x| (x, y)))
                .map(|(x, y)| u32::from(a.coverage_at(h, x, y)))
                .sum();
            assert!(
                ink > 15 * u32::from(h.height),
                "line {line}: H has ink ({ink})"
            );
            let space = a.glyph(' ').unwrap();
            assert_eq!((space.width, space.height), (0, 0));
            assert!(space.advance > 0);
            // Anti-aliased: the edges of an O are partly covered.
            let o = a.glyph('O').unwrap();
            let partial = (0..u32::from(o.height))
                .flat_map(|y| (0..u32::from(o.width)).map(move |x| (x, y)))
                .filter(|&(x, y)| (1..15).contains(&a.coverage_at(o, x, y)))
                .count();
            assert!(
                partial > 4,
                "line {line}: {partial} partly covered pixels on an O"
            );
        }
    }

    #[test]
    fn the_nearest_size_not_above_is_picked_and_widths_add_up() {
        assert_eq!(inter(14).line, 10);
        assert_eq!(inter(15).line, 15);
        assert_eq!(inter(100).line, 20);
        assert_eq!(inter(4).line, 10, "below every atlas: the smallest");
        let a = inter(15);
        assert_eq!(a.width(""), 0);
        let one = u32::from(a.glyph('m').unwrap().advance);
        assert_eq!(a.width("mm"), (2 * one).div_ceil(64));
        assert!(a.width("mmmm") > a.width("iiii"), "proportional");
        assert_eq!(
            a.width("\u{4e2d}"),
            a.width("?"),
            "a character the subset lacks is drawn as '?'"
        );
        assert_eq!(a.coverage_at(a.glyph('H').unwrap(), 999, 0), 0);
    }

    #[test]
    fn every_glyph_is_inside_the_coverage_and_every_linked_face_has_its_atlases() {
        let ranges: std::vec::Vec<(u32, u32)> = crate::COVERAGE
            .split(", ")
            .map(|part| {
                let part = part.trim_start_matches("U+");
                let (lo, hi) = part.split_once('-').unwrap_or((part, part));
                (
                    u32::from_str_radix(lo, 16).unwrap(),
                    u32::from_str_radix(hi, 16).unwrap(),
                )
            })
            .collect();
        for a in ATLASES
            .iter()
            .filter(|a| crate::family(a.family).is_some_and(|f| f.role != crate::Role::Icons))
        {
            for g in a.glyphs {
                assert!(
                    ranges.iter().any(|&(lo, hi)| (lo..=hi).contains(&g.ch)),
                    "{:x} is listed",
                    g.ch
                );
            }
        }
        for f in crate::FAMILIES {
            for bold in [false, true].into_iter().filter(|b| !b || f.bold.is_some()) {
                for &line in crate::ATLAS_SIZES {
                    let a = Atlas::pick(ATLASES, f.id, bold, u32::from(line)).unwrap();
                    assert_eq!((a.line, a.bold), (line, bold), "{}", f.id);
                }
            }
        }
    }

    #[test]
    fn a_face_of_a_set_finds_its_atlas_and_a_role_without_a_family_none() {
        let set = FontSet::default_set();
        let bold = Atlas::for_face(&set, Face::SansBold, 15).unwrap();
        assert_eq!((bold.family, bold.bold, bold.line), ("inter", true, 15));
        assert_eq!(
            Atlas::for_face(&set, Face::Mono, 20).unwrap().family,
            "jetbrains-mono"
        );
        assert!(Atlas::for_face(&FontSet::NONE, Face::Sans, 15).is_none());
    }
}
