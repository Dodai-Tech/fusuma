//! Glyph atlases: the bundled fonts rasterised at build time (feature `atlas`), for a renderer that draws text by
//! blending coverage rather than filling outlines. No allocator, no floating point. The types are
//! [`fusuma_atlas`]'s.

pub use fusuma_atlas::{Atlas, AtlasGlyph, Kerning};

use crate::{Face, FontSet};

/// The atlas that draws `face` of `set` at a line of `size` pixels; `None` when the face has no family (draw the
/// bitmap font). A sans without a bold face draws bold with its regular atlas.
#[must_use]
pub fn for_face(set: &FontSet, face: Face, size: u32) -> Option<&'static Atlas> {
    let (family, file) = set.file(face)?;
    Atlas::pick(ATLASES, family.id, file.bold, size)
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
        for a in ATLASES {
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
        let bold = for_face(&set, Face::SansBold, 15).unwrap();
        assert_eq!((bold.family, bold.bold, bold.line), ("inter", true, 15));
        assert_eq!(
            for_face(&set, Face::Mono, 20).unwrap().family,
            "jetbrains-mono"
        );
        assert!(for_face(&FontSet::NONE, Face::Sans, 15).is_none());
    }
}
