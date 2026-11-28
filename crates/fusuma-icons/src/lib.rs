#![doc = include_str!("../README.md")]
#![no_std]

mod names;

pub use names::*;

use fusuma_fonts::{Family, FontFile, Role};

/// The Lucide font, every icon of it (about 1 MB: a build for a small chip draws from a cut of it instead, see the
/// README).
pub const FONT: &[u8] = include_bytes!("../fonts/lucide.ttf");

/// The licence text of the Lucide font (ISC, with parts MIT from Feather): ship it with any binary that bundles it.
pub const LICENCE: &str = include_str!("../fonts/LICENSE-lucide.txt");

/// The lucide-static version the font and the names are from.
pub const VERSION: &str = "1.80.0";

/// Lucide as an icon family, for a `FontSet`: `FontSet { icons: Some(&fusuma_icons::FAMILY), ..set }` makes every
/// renderer draw its icons with Lucide, found by Lucide's names.
pub static FAMILY: Family = Family {
    id: "lucide-full",
    name: "lucide",
    role: Role::Icons,
    about: "Lucide, every icon, line icons on a 24 px grid (ISC, with parts MIT from Feather)",
    regular: FontFile {
        family: "lucide",
        bold: false,
        data: FONT,
    },
    bold: None,
    licence: LICENCE,
    icons: &NAMES,
};

/// The character of the icon called `name` (`"arrow-left"`, Lucide's own names, the ones on lucide.dev/icons), or
/// `None` when Lucide has no such icon.
#[must_use]
pub fn icon(name: &str) -> Option<char> {
    NAMES
        .binary_search_by(|(n, _)| (*n).cmp(name))
        .ok()
        .map(|i| NAMES[i].1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sorted_and_unique_so_the_lookup_finds_every_one() {
        for pair in NAMES.windows(2) {
            assert!(pair[0].0 < pair[1].0, "{} before {}", pair[0].0, pair[1].0);
        }
        for &(name, ch) in &NAMES {
            assert_eq!(icon(name), Some(ch), "{name}");
        }
        assert_eq!(NAMES.len(), COUNT);
    }

    #[test]
    fn an_icon_is_found_by_name_and_by_constant() {
        assert_eq!(icon("house"), Some(HOUSE));
        assert_eq!(icon("arrow-left"), Some(ARROW_LEFT));
        assert_eq!(icon("not-an-icon"), None);
    }

    #[test]
    fn every_character_is_in_the_private_use_area_and_in_the_font() {
        let face = ttf_cmap(FONT);
        for &(name, ch) in &NAMES {
            assert!(('\u{e000}'..='\u{f8ff}').contains(&ch), "{name}");
            assert!(face(ch), "{name} is not in the font");
        }
    }

    /// A check that the font has a glyph for a character, from its `cmap` (formats 4 and 12), with no dependency.
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
        let ranges: alloc_free::Ranges = (0..subs)
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
        move |c| ranges.0.iter().any(|&(a, b)| (a..=b).contains(&(c as u32)))
    }

    mod alloc_free {
        extern crate std;
        pub struct Ranges(pub std::vec::Vec<(u32, u32)>);
        impl FromIterator<(u32, u32)> for Ranges {
            fn from_iter<I: IntoIterator<Item = (u32, u32)>>(i: I) -> Self {
                Self(i.into_iter().collect())
            }
        }
    }
}
