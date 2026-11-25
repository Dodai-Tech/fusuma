//! Reading a settings file (`fusuma.toml`).
//!
//! ```toml
//! [render]
//! backend = "auto"      # auto (the default), integer, vello-cpu or vello-gpu
//! fallback = true       # step down to a lighter backend if the chosen one does not fit; default true
//! min_free_kb = 100     # internal RAM that must stay free after the frame; default 100 (a guess)
//!
//! [fonts]
//! preset = "material"   # a preset of fusuma-fonts' catalog; default: the first one the build linked
//! sans = "roboto"       # or name the families one by one, overriding the preset's
//! mono = "roboto-mono"
//! sizes = [10, 15, 20]  # line heights of the glyph atlases (the integer renderer), read at build time
//!
//! [theme]
//! palette = "forest"    # fusuma (default), forest, ocean, petrol or violet; the app can switch at run time
//! accent = "violet"     # another palette's main-action colour on it
//! ```
//!
//! Every key is optional, and a key left out says nothing, so the layer below it decides. An unknown key
//! or backend name is an error that names it. [`SETTINGS_SCHEMA`] is the JSON schema of a settings file.

use serde::Deserialize;

use crate::{Backend, BackendSetting, RenderLayer};

/// The JSON schema of a settings file, for editors and for a wizard to validate against.
pub const SETTINGS_SCHEMA: &str = include_str!("../fusuma.schema.json");

/// Why settings could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsError(pub std::string::String);

impl core::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SettingsError {}

/// The `[render]` table as written. Another crate whose file has a `[render]` table of its own (a board
/// file) embeds this as the field's type and turns it into a layer with [`into_layer`](Self::into_layer).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RawRender {
    /// `backend`.
    pub backend: Option<std::string::String>,
    /// `fallback`.
    pub fallback: Option<bool>,
    /// `min_free_kb`.
    pub min_free_kb: Option<u32>,
}

impl RawRender {
    /// The layer these settings make.
    ///
    /// # Errors
    /// [`SettingsError`] for a backend name that is not `auto` or one of [`Backend::ALL`].
    pub fn into_layer(self) -> Result<RenderLayer, SettingsError> {
        let backend = match self.backend.as_deref() {
            None => None,
            Some("auto") => Some(BackendSetting::Auto),
            Some(name) => Some(BackendSetting::Fixed(Backend::from_name(name).ok_or_else(
                || {
                    let known: std::vec::Vec<&str> =
                        Backend::ALL.iter().map(|b| b.name()).collect();
                    SettingsError(std::format!(
                        "unknown render backend '{name}'; known: auto, {}",
                        known.join(", ")
                    ))
                },
            )?)),
        };
        Ok(RenderLayer {
            backend,
            fallback: self.fallback,
            min_free_kb: self.min_free_kb,
        })
    }
}

/// The `[fonts]` table: which of the linked font families draw. The names are checked against the families the
/// build linked by the platform that applies them (`fusuma::fonts::FontSet::choose`); this crate knows no fonts.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FontChoice {
    /// A preset's id (`material`).
    pub preset: Option<std::string::String>,
    /// The sans family's id, over the preset's.
    pub sans: Option<std::string::String>,
    /// The mono family's id, over the preset's.
    pub mono: Option<std::string::String>,
    /// Line heights the glyph atlases are made at. Read at build time (`fusuma-fonts` reads the file
    /// `FUSUMA_SETTINGS` names); at run time only compared with what the build made.
    pub sizes: Option<std::vec::Vec<u16>>,
}

/// The `[theme]` table: which palette and accent the app starts with. Names are checked by the platform against
/// `fusuma::Palette::named` and `fusuma::Accent::named`; this crate knows no colours.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ThemeChoice {
    /// A palette's name (`forest`).
    pub palette: Option<std::string::String>,
    /// An accent's name (`violet`), put on the palette.
    pub accent: Option<std::string::String>,
}

/// A settings file (`fusuma.toml`), read and checked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// The `[render]` table as a layer (empty when the file has none).
    pub render: RenderLayer,
    /// The `[fonts]` table (all `None` when the file has none).
    pub fonts: FontChoice,
    /// The `[theme]` table (all `None` when the file has none).
    pub theme: ThemeChoice,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    render: Option<RawRender>,
    fonts: Option<FontChoice>,
    theme: Option<ThemeChoice>,
}

impl Settings {
    /// Reads a settings file. A table or key it does not know is an error, so a typo is not silence.
    ///
    /// # Errors
    /// [`SettingsError`] for text that is not valid TOML, an unknown key or table, an unknown backend, or a line
    /// height outside 6 to 96.
    pub fn from_toml(text: &str) -> Result<Self, SettingsError> {
        let file: File = toml::from_str(text)
            .map_err(|e| SettingsError(std::string::ToString::to_string(&e)))?;
        let fonts = file.fonts.unwrap_or_default();
        if let Some(bad) = fonts
            .sizes
            .iter()
            .flatten()
            .find(|s| !(6..=96).contains(*s))
        {
            return Err(SettingsError(std::format!(
                "fonts.sizes: {bad} is not a line height of 6 to 96"
            )));
        }
        Ok(Self {
            render: file
                .render
                .map_or(Ok(RenderLayer::EMPTY), RawRender::into_layer)?,
            fonts,
            theme: file.theme.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Render;

    #[test]
    fn a_full_table_is_read() {
        let l = Settings::from_toml(
            "[render]\nbackend = \"vello-cpu\"\nfallback = false\nmin_free_kb = 64\n",
        )
        .unwrap()
        .render;
        assert_eq!(l.backend, Some(BackendSetting::Fixed(Backend::VelloCpu)));
        assert_eq!((l.fallback, l.min_free_kb), (Some(false), Some(64)));
    }

    #[test]
    fn what_is_left_out_says_nothing() {
        assert_eq!(Settings::from_toml("").unwrap().render, RenderLayer::EMPTY);
        assert_eq!(
            Settings::from_toml("[render]\n").unwrap().render,
            RenderLayer::EMPTY
        );
        let l = Settings::from_toml("[render]\nfallback = false\n")
            .unwrap()
            .render;
        assert_eq!(l.backend, None);
        assert_eq!(
            Render::layered(&[l]).min_free_kb,
            crate::DEFAULT_MIN_FREE_KB
        );
    }

    #[test]
    fn auto_is_written_out_and_every_backend_name_works() {
        let auto = Settings::from_toml("[render]\nbackend = \"auto\"\n")
            .unwrap()
            .render;
        assert_eq!(auto.backend, Some(BackendSetting::Auto));
        for b in Backend::ALL {
            let l = Settings::from_toml(&std::format!("[render]\nbackend = \"{}\"\n", b.name()))
                .unwrap()
                .render;
            assert_eq!(l.backend, Some(BackendSetting::Fixed(b)));
        }
    }

    #[test]
    fn the_fonts_table_is_read_and_checked() {
        let s = Settings::from_toml(
            "[fonts]\npreset = \"material\"\nmono = \"jetbrains-mono\"\nsizes = [12, 18]\n",
        )
        .unwrap();
        assert_eq!(
            s.fonts,
            FontChoice {
                preset: Some("material".into()),
                sans: None,
                mono: Some("jetbrains-mono".into()),
                sizes: Some(std::vec![12, 18]),
            }
        );
        assert_eq!(s.render, RenderLayer::EMPTY);
        assert_eq!(
            Settings::from_toml("").unwrap().fonts,
            FontChoice::default()
        );
        let err = |t: &str| Settings::from_toml(t).unwrap_err().to_string();
        assert!(err("[fonts]\nsizes = [4]\n").contains("fonts.sizes: 4"));
        assert!(err("[fonts]\nfamily = \"x\"\n").contains("family"));
        assert!(err("[font]\npreset = \"x\"\n").contains("font"));
        for key in ["preset", "sans", "mono", "sizes"] {
            assert!(
                SETTINGS_SCHEMA.contains(&std::format!("\"{key}\"")),
                "the schema has fonts.{key}"
            );
        }
    }

    #[test]
    fn the_theme_table_is_read() {
        let s =
            Settings::from_toml("[theme]\npalette = \"forest\"\naccent = \"violet\"\n").unwrap();
        assert_eq!(
            s.theme,
            ThemeChoice {
                palette: Some("forest".into()),
                accent: Some("violet".into()),
            }
        );
        assert_eq!(
            Settings::from_toml("").unwrap().theme,
            ThemeChoice::default()
        );
        let e = Settings::from_toml("[theme]\ncolour = \"red\"\n")
            .unwrap_err()
            .to_string();
        assert!(e.contains("colour"), "{e}");
        for key in ["\"theme\"", "\"palette\"", "\"accent\""] {
            assert!(SETTINGS_SCHEMA.contains(key), "the schema has {key}");
        }
    }

    #[test]
    fn mistakes_are_errors_that_name_the_problem() {
        let err = |t: &str| Settings::from_toml(t).unwrap_err().to_string();
        assert!(err("[render]\nbackend = \"vello\"\n").contains("unknown render backend 'vello'"));
        assert!(err("[render]\nbackend = \"vello\"\n").contains("auto, "));
        assert!(err("[render]\nbogus = 1\n").contains("bogus"));
        assert!(
            err("[rendr]\nbackend = \"integer\"\n").contains("rendr"),
            "a typo in the table name"
        );
        assert!(
            err("[render]\nfallback = \"yes\"\n").contains("fallback")
                || err("[render]\nfallback = \"yes\"\n").contains("bool")
        );
        assert!(
            err("[render]\nmin_free_kb = -1\n").contains("min_free_kb")
                || err("[render]\nmin_free_kb = -1\n").contains("u32")
        );
        assert!(!err("[render\n").is_empty(), "broken TOML");
    }

    #[test]
    fn the_schema_names_exactly_the_keys_and_backends_the_parser_takes() {
        let v: serde_json_free::Names = serde_json_free::names(SETTINGS_SCHEMA);
        for key in ["backend", "fallback", "min_free_kb"] {
            assert!(v.has(key), "the schema lacks {key}");
        }
        for b in Backend::ALL {
            assert!(v.has(b.name()), "the schema lacks {}", b.name());
        }
        assert!(v.has("auto") && v.has("additionalProperties"));
        assert!(!v.has("ram_share_percent"));
    }

    /// Just enough to ask whether a quoted word appears in the schema, without a JSON dependency.
    mod serde_json_free {
        pub struct Names(std::string::String);
        pub fn names(s: &str) -> Names {
            Names(s.to_owned())
        }
        impl Names {
            pub fn has(&self, word: &str) -> bool {
                self.0.contains(&std::format!("\"{word}\""))
            }
        }
    }
}
