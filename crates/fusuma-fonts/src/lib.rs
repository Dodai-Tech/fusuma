#![doc = include_str!("../README.md")]
#![no_std]

/// One bundled font file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontFile {
    /// The family name inside the file, which is what a layout asks for.
    pub family: &'static str,
    /// Whether this is the bold face of the family.
    pub bold: bool,
    /// The TrueType data.
    pub data: &'static [u8],
}

/// Inter Regular, a Latin subset.
#[cfg(feature = "inter")]
pub const INTER_REGULAR: FontFile = FontFile {
    family: "Inter",
    bold: false,
    data: include_bytes!("../fonts/Inter-Regular.ttf"),
};

/// Inter Bold, a Latin subset.
#[cfg(feature = "inter-bold")]
pub const INTER_BOLD: FontFile = FontFile {
    family: "Inter",
    bold: true,
    data: include_bytes!("../fonts/Inter-Bold.ttf"),
};

/// JetBrains Mono Regular, a Latin subset.
#[cfg(feature = "jetbrains-mono")]
pub const JETBRAINS_MONO: FontFile = FontFile {
    family: "JetBrains Mono",
    bold: false,
    data: include_bytes!("../fonts/JetBrainsMono-Regular.ttf"),
};

/// Every font the enabled features bundle.
pub const ALL: &[FontFile] = &[
    #[cfg(feature = "inter")]
    INTER_REGULAR,
    #[cfg(feature = "inter-bold")]
    INTER_BOLD,
    #[cfg(feature = "jetbrains-mono")]
    JETBRAINS_MONO,
];

/// The licence text of Inter (SIL Open Font License 1.1). Ship it with any binary that bundles Inter.
pub const INTER_LICENSE: &str = include_str!("../fonts/Inter-OFL.txt");

/// The licence text of JetBrains Mono (SIL Open Font License 1.1).
pub const JETBRAINS_MONO_LICENSE: &str = include_str!("../fonts/JetBrainsMono-OFL.txt");

/// The characters the subsets cover: ASCII, Latin-1 and Latin Extended-A (so Romanian ă â î and Central and
/// Eastern European letters), the Romanian comma-below ș ț, common punctuation, the euro sign and four arrows.
/// Greek, Cyrillic, CJK and emoji are not included.
pub const COVERAGE: &str = "U+0020-007E, U+00A0-017F, U+0218-021B, U+2013-2014, U+2018-201E, U+2022, U+2026, U+20AC, U+2190-2193, U+2212";

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn every_enabled_font_is_a_truetype_file_of_a_sensible_size() {
        assert!(!ALL.is_empty(), "the default feature bundles Inter");
        for f in ALL {
            assert_eq!(&f.data[..4], &[0, 1, 0, 0], "{} is not TrueType", f.family);
            assert!(
                (30_000..150_000).contains(&f.data.len()),
                "{}: {} bytes",
                f.family,
                f.data.len()
            );
        }
    }

    #[test]
    fn the_licences_are_the_ofl_and_name_their_authors() {
        for (text, who) in [
            (INTER_LICENSE, "Inter Project Authors"),
            (JETBRAINS_MONO_LICENSE, "JetBrains Mono Project Authors"),
        ] {
            assert!(text.contains("SIL OPEN FONT LICENSE Version 1.1"), "{who}");
            assert!(text.contains(who));
        }
    }

    #[cfg(all(feature = "inter", feature = "inter-bold", feature = "jetbrains-mono"))]
    #[test]
    fn the_whole_set_is_about_two_hundred_kilobytes() {
        let total: usize = ALL.iter().map(|f| f.data.len()).sum();
        assert!(total < 260_000, "{total} bytes");
        assert_eq!(ALL.len(), 3);
        let bolds: std::vec::Vec<bool> = ALL.iter().map(|f| f.bold).collect();
        assert_eq!(
            bolds.iter().filter(|&&b| b).count(),
            1,
            "exactly one bold face: Inter Bold"
        );
    }
}
