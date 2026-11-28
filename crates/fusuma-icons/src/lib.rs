#![doc = include_str!("../README.md")]
#![no_std]

mod names;

use core::fmt;

pub use names::{COUNT, Icon};

/// The Lucide font, every icon of it (about 1 MB, linked only where it is used: a vector renderer draws icons from
/// it, a microcontroller build draws from the [`atlas`] instead).
pub const FONT: &[u8] = include_bytes!("../fonts/lucide.ttf");

/// The family name the font calls itself, which is what a text layout asks for.
pub const FONT_FAMILY: &str = "lucide";

/// The licence text of the Lucide font (ISC, with parts MIT from Feather): ship it with any binary that bundles it.
pub const LICENCE: &str = include_str!("../fonts/LICENSE-lucide.txt");

/// The lucide-static version the font and the names are from.
pub const VERSION: &str = "1.80.0";

impl Icon {
    /// The character of the icon in [`FONT`]: drawn as text in that font, it is the icon.
    #[must_use]
    pub fn glyph(self) -> char {
        names::GLYPHS[self as usize]
    }

    /// The name on lucide.dev (`"arrow-left"`).
    #[must_use]
    pub fn name(self) -> &'static str {
        names::NAMES[self as usize]
    }

    /// The icon called `name` on lucide.dev (`"arrow-left"`), for a name read from a file; `None` when Lucide has no
    /// such icon. In code, use the variant (`Icon::ArrowLeft`), which the compiler checks.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        names::NAMES.binary_search(&name).ok().map(|i| Self::ALL[i])
    }
}

impl fmt::Display for Icon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Glyph atlases of the icons the app uses (feature `atlas`).
#[cfg(feature = "atlas")]
pub mod atlas {
    pub use fusuma_atlas::{Atlas, AtlasGlyph, Kerning};

    include!(concat!(env!("OUT_DIR"), "/atlas.rs"));

    /// The atlas that fits a line of `size` pixels: the largest not above it, else the smallest.
    #[must_use]
    pub fn for_size(size: u32) -> Option<&'static Atlas> {
        Atlas::pick(ATLASES, "lucide", false, size)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn the_names_are_sorted_and_every_icon_finds_itself_by_its_name() {
        for pair in names::NAMES.windows(2) {
            assert!(pair[0] < pair[1], "{} before {}", pair[0], pair[1]);
        }
        assert_eq!(Icon::ALL.len(), COUNT);
        for (i, icon) in Icon::ALL.into_iter().enumerate() {
            assert_eq!(icon as usize, i, "{icon}");
            assert_eq!(Icon::from_name(icon.name()), Some(icon), "{icon}");
        }
    }

    #[test]
    fn an_icon_is_a_variant_with_a_name_and_a_character() {
        assert_eq!(Icon::House.name(), "house");
        assert_eq!(Icon::ArrowLeft.name(), "arrow-left");
        assert_eq!(Icon::from_name("arrow-left"), Some(Icon::ArrowLeft));
        assert_eq!(Icon::from_name("not-an-icon"), None);
        assert_ne!(Icon::House.glyph(), Icon::Settings.glyph());
        assert_eq!(std::format!("{}", Icon::ArrowDown0_1), "arrow-down-0-1");
    }

    #[test]
    fn every_character_is_in_the_private_use_area_and_in_the_font() {
        let has = ttf_cmap(FONT);
        for icon in Icon::ALL {
            let c = icon.glyph();
            assert!(('\u{e000}'..='\u{f8ff}').contains(&c), "{icon}");
            assert!(has(c), "{icon} is not in the font");
        }
    }

    /// A check that the font has a glyph for a character, from its `cmap` (format 12 groups), with no dependency.
    fn ttf_cmap(data: &'static [u8]) -> impl Fn(char) -> bool {
        let u16_at = move |o: usize| u32::from(u16::from_be_bytes([data[o], data[o + 1]]));
        let u32_at =
            move |o: usize| u32::from_be_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]);
        let tables = u16_at(4) as usize;
        let cmap = (0..tables)
            .map(|i| 12 + i * 16)
            .find(|&r| &data[r..r + 4] == b"cmap")
            .map(|r| u32_at(r + 8) as usize)
            .expect("a cmap table");
        let subs = u16_at(cmap + 2) as usize;
        let ranges: std::vec::Vec<(u32, u32)> = (0..subs)
            .map(|i| cmap + u32_at(cmap + 4 + i * 8 + 4) as usize)
            .filter(|&s| u16_at(s) == 12)
            .flat_map(|s| {
                let groups = u32_at(s + 12) as usize;
                (0..groups).map(move |g| {
                    let o = s + 16 + g * 12;
                    (u32_at(o), u32_at(o + 4))
                })
            })
            .collect();
        move |c| ranges.iter().any(|&(a, b)| (a..=b).contains(&(c as u32)))
    }

    #[cfg(feature = "atlas")]
    #[test]
    fn the_atlas_has_the_toolkits_icons_inside_their_square() {
        for a in atlas::ATLASES {
            assert!(a.glyphs.windows(2).all(|w| w[0].ch < w[1].ch), "sorted");
            assert_eq!(a.baseline, a.line);
            for icon in [Icon::House, Icon::Settings, Icon::ChevronDown, Icon::X] {
                let g = a.glyph(icon.glyph()).unwrap_or_else(|| panic!("{icon}"));
                assert!(
                    i32::from(g.top) >= 0
                        && i32::from(g.top) + i32::from(g.height) <= i32::from(a.line),
                    "{} {icon}: top {} height {}",
                    a.line,
                    g.top,
                    g.height
                );
                assert!(g.width > 0 && u32::from(g.width) <= u32::from(a.line) + 1);
            }
        }
        assert_eq!(atlas::for_size(15).unwrap().line, 15);
    }
}
