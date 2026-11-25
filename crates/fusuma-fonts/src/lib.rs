#![doc = include_str!("../README.md")]
#![no_std]

/// Glyph atlases of the enabled fonts (feature `atlas`).
#[cfg(feature = "atlas")]
pub mod atlas;

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

/// What a family is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// Running text and titles; comes with a bold face.
    Sans,
    /// Fixed-width text (code, logs, numbers in columns).
    Mono,
    /// Icons: each character is an icon, looked up by name ([`FontSet::icon`]).
    Icons,
}

/// A family the build linked: its files and licence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Family {
    /// The id the catalog, the cargo feature and the settings use (`"roboto"`).
    pub id: &'static str,
    /// The family name inside its files (`"Roboto"`).
    pub name: &'static str,
    /// What it is for.
    pub role: Role,
    /// One line about it.
    pub about: &'static str,
    /// The regular face.
    pub regular: FontFile,
    /// The bold face (a sans has one).
    pub bold: Option<FontFile>,
    /// Its licence text (SIL Open Font License 1.1, or Apache 2.0 for Material Symbols): ship it with any binary
    /// that bundles the family.
    pub licence: &'static str,
    /// An icon family's icons, by name, sorted by name; empty for text.
    pub icons: &'static [(&'static str, char)],
}

/// A sans and a mono that go together, by family id. Every preset of the catalog is listed, linked or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    /// Its id (`"material"`): what `[fonts] preset` names and the cargo feature `preset-<id>` links.
    pub id: &'static str,
    /// One line about it.
    pub about: &'static str,
    /// The sans family's id.
    pub sans: &'static str,
    /// The mono family's id.
    pub mono: &'static str,
}

/// A face a renderer asks for: the roles a theme uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// The sans, regular.
    Sans,
    /// The sans, bold.
    SansBold,
    /// The mono.
    Mono,
    /// The icons.
    Icons,
}

/// Which linked family fills each role. Built once from the settings ([`FontSet::choose`]); a renderer draws a
/// role whose family is `None` with its bitmap font.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FontSet {
    /// The sans family.
    pub sans: Option<&'static Family>,
    /// The mono family.
    pub mono: Option<&'static Family>,
    /// The icon family.
    pub icons: Option<&'static Family>,
}

/// Why a font choice cannot be met: it names the setting and the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError<'a> {
    /// No preset has this id.
    UnknownPreset(&'a str),
    /// No family of the catalog has this id.
    UnknownFamily(&'a str),
    /// The family is in the catalog but its cargo feature is off.
    NotLinked(&'a str),
    /// The family is a mono where a sans is asked for, or the other way round.
    WrongRole(&'a str),
}

impl core::fmt::Display for FontError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownPreset(id) => write!(f, "no font preset '{id}'"),
            Self::UnknownFamily(id) => write!(f, "no font family '{id}' in the catalog"),
            Self::NotLinked(id) => write!(
                f,
                "font family '{id}' is not linked: turn on the cargo feature fusuma-fonts/{id}"
            ),
            Self::WrongRole(id) => {
                write!(
                    f,
                    "font family '{id}' is not for that role (sans, mono or icons)"
                )
            }
        }
    }
}

/// The linked family `id`.
#[must_use]
pub fn family(id: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|f| f.id == id)
}

/// Every font file linked, regular and bold: what a text engine registers.
pub fn files() -> impl Iterator<Item = &'static FontFile> {
    FAMILIES
        .iter()
        .flat_map(|f| core::iter::once(&f.regular).chain(f.bold.as_ref()))
}

impl FontSet {
    /// No family: every role is drawn with the bitmap font.
    pub const NONE: Self = Self {
        sans: None,
        mono: None,
        icons: None,
    };

    /// The default: the first preset whose families are both linked, else the first linked sans and mono; and the
    /// first linked icon family.
    #[must_use]
    pub fn default_set() -> Self {
        let first = |role| FAMILIES.iter().find(|f| f.role == role);
        let text = PRESETS
            .iter()
            .find_map(|p| Some((family(p.sans)?, family(p.mono)?)));
        Self {
            sans: text.map(|t| t.0).or_else(|| first(Role::Sans)),
            mono: text.map(|t| t.1).or_else(|| first(Role::Mono)),
            icons: first(Role::Icons),
        }
    }

    /// The set the settings ask for: a `preset`, and/or a `sans`, a `mono` and an `icons` family by id, each
    /// overriding the default's. Nothing asked: [`default_set`](Self::default_set).
    ///
    /// # Errors
    /// [`FontError`] naming an unknown preset or family, one whose feature is off, or one in the wrong role.
    pub fn choose<'a>(
        preset: Option<&'a str>,
        sans: Option<&'a str>,
        mono: Option<&'a str>,
        icons: Option<&'a str>,
    ) -> Result<Self, FontError<'a>> {
        let mut set = Self::default_set();
        if let Some(id) = preset {
            let p = PRESETS
                .iter()
                .find(|p| p.id == id)
                .ok_or(FontError::UnknownPreset(id))?;
            set.sans = Some(pick(p.sans, Role::Sans)?);
            set.mono = Some(pick(p.mono, Role::Mono)?);
        }
        if let Some(id) = icons {
            set.icons = Some(pick(id, Role::Icons)?);
        }
        if let Some(id) = sans {
            set.sans = Some(pick(id, Role::Sans)?);
        }
        if let Some(id) = mono {
            set.mono = Some(pick(id, Role::Mono)?);
        }
        Ok(set)
    }

    /// The family and file that draw `face`, if its family is set (a sans without a bold face draws bold with its
    /// regular one).
    #[must_use]
    pub fn file(&self, face: Face) -> Option<(&'static Family, &'static FontFile)> {
        match face {
            Face::Sans => self.sans.map(|f| (f, &f.regular)),
            Face::SansBold => self
                .sans
                .map(|f| (f, f.bold.as_ref().unwrap_or(&f.regular))),
            Face::Mono => self.mono.map(|f| (f, &f.regular)),
            Face::Icons => self.icons.map(|f| (f, &f.regular)),
        }
    }

    /// The character of the icon called `name` (`"settings"`) in this set's icon family; `None` when there is no
    /// icon family or it has no such icon.
    #[must_use]
    pub fn icon(&self, name: &str) -> Option<char> {
        let icons = self.icons?.icons;
        icons
            .binary_search_by(|(n, _)| (*n).cmp(name))
            .ok()
            .map(|i| icons[i].1)
    }
}

impl Default for FontSet {
    fn default() -> Self {
        Self::default_set()
    }
}

/// The linked family `id` in `role`, or why not.
fn pick(id: &str, role: Role) -> Result<&'static Family, FontError<'_>> {
    // The catalog's ids, linked or not, are the presets' names plus the linked families; an id in neither is unknown.
    let known = CATALOG_IDS.contains(&id);
    let f = family(id).ok_or(if known {
        FontError::NotLinked(id)
    } else {
        FontError::UnknownFamily(id)
    })?;
    if f.role == role {
        Ok(f)
    } else {
        Err(FontError::WrongRole(id))
    }
}

include!(concat!(env!("OUT_DIR"), "/fonts.rs"));

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;

    #[test]
    fn the_default_features_link_the_fusuma_preset_and_it_is_the_default() {
        let set = FontSet::default_set();
        assert_eq!(set.sans.map(|f| f.id), Some("inter"));
        assert_eq!(set.mono.map(|f| f.id), Some("jetbrains-mono"));
        let (fam, file) = set.file(Face::SansBold).unwrap();
        assert_eq!((fam.name, file.bold), ("Inter", true));
        assert_eq!(set.file(Face::Mono).unwrap().1.family, "JetBrains Mono");
        assert_eq!(FontSet::default(), set);
        assert_eq!(FontSet::NONE.file(Face::Sans), None);
    }

    #[test]
    fn every_linked_file_is_truetype_with_its_licence() {
        assert!(!FAMILIES.is_empty());
        for f in files() {
            assert_eq!(&f.data[..4], &[0, 1, 0, 0], "{} is not TrueType", f.family);
            assert!(
                (15_000..150_000).contains(&f.data.len()),
                "{}: {} bytes",
                f.family,
                f.data.len()
            );
        }
        for f in FAMILIES {
            let licence = if f.role == Role::Icons {
                "Apache License"
            } else {
                "SIL OPEN FONT LICENSE Version 1.1"
            };
            assert!(f.licence.contains(licence), "{}", f.id);
            assert_eq!(
                f.bold.is_some(),
                f.role == Role::Sans,
                "{}: a sans has a bold, a mono not",
                f.id
            );
        }
    }

    #[cfg(feature = "material-symbols")]
    #[test]
    fn icons_are_found_by_name_and_every_one_has_a_glyph() {
        let set = FontSet::default_set();
        let icons = set.icons.unwrap();
        assert_eq!(icons.id, "material-symbols");
        assert!(
            icons.icons.windows(2).all(|w| w[0].0 < w[1].0),
            "sorted for the search"
        );
        // That each one has a glyph is checked on its atlas (`atlas::tests`), rasterised from the file.
        for (name, c) in icons.icons {
            assert_eq!(set.icon(name), Some(*c));
        }
        assert!(set.icon("settings").is_some() && set.icon("home") != set.icon("settings"));
        assert_eq!(set.icon("nope"), None);
        assert_eq!(FontSet::NONE.icon("settings"), None);
        assert_eq!(
            set.file(Face::Icons).unwrap().1.family,
            "Material Symbols Rounded"
        );
        assert_eq!(
            FontSet::choose(None, None, None, Some("inter")),
            Err(FontError::WrongRole("inter"))
        );
        assert_eq!(
            FontSet::choose(None, None, None, Some("material-symbols")),
            Ok(set)
        );
    }

    #[test]
    fn a_choice_names_what_is_wrong() {
        assert_eq!(
            FontSet::choose(None, None, None, None),
            Ok(FontSet::default_set())
        );
        assert_eq!(
            FontSet::choose(Some("fusuma"), None, None, None),
            Ok(FontSet::default_set())
        );
        assert_eq!(
            FontSet::choose(Some("nope"), None, None, None),
            Err(FontError::UnknownPreset("nope"))
        );
        assert_eq!(
            FontSet::choose(None, Some("comic"), None, None),
            Err(FontError::UnknownFamily("comic"))
        );
        assert_eq!(
            FontSet::choose(None, Some("jetbrains-mono"), None, None),
            Err(FontError::WrongRole("jetbrains-mono"))
        );
        assert_eq!(
            FontSet::choose(None, None, Some("inter"), None),
            Err(FontError::WrongRole("inter"))
        );
        let e = std::format!("{}", FontError::NotLinked("roboto"));
        assert!(e.contains("fusuma-fonts/roboto"), "{e}");
    }

    #[cfg(not(feature = "roboto"))]
    #[test]
    fn a_family_that_is_not_linked_says_which_feature_links_it() {
        assert_eq!(
            FontSet::choose(Some("material"), None, None, None),
            Err(FontError::NotLinked("roboto"))
        );
        assert_eq!(
            FontSet::choose(None, Some("roboto"), None, None),
            Err(FontError::NotLinked("roboto"))
        );
    }

    #[cfg(all(feature = "roboto", feature = "roboto-mono"))]
    #[test]
    fn a_preset_and_a_family_override_combine() {
        let set = FontSet::choose(Some("material"), None, Some("jetbrains-mono"), None).unwrap();
        assert_eq!(
            (set.sans.unwrap().name, set.mono.unwrap().name),
            ("Roboto", "JetBrains Mono")
        );
    }

    /// The cargo features of this crate are the catalog's families and presets, so a family cannot be listed in one
    /// and forgotten in the other.
    #[test]
    fn the_cargo_features_match_the_catalog() {
        let manifest = include_str!("../Cargo.toml");
        for id in CATALOG_IDS {
            assert!(
                manifest.contains(&std::format!("\n{id} = [")),
                "Cargo.toml has the feature {id}"
            );
        }
        for p in PRESETS {
            assert!(
                manifest.contains(&std::format!(
                    "\npreset-{} = [\"{}\", \"{}\"]",
                    p.id,
                    p.sans,
                    p.mono
                )),
                "Cargo.toml: preset-{}",
                p.id
            );
            assert!(
                manifest.contains(&std::format!("\"preset-{}\"", p.id)),
                "all-families has preset-{}",
                p.id
            );
        }
    }
}
