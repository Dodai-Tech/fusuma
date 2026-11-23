//! Reading the `[render]` settings from TOML.
//!
//! ```toml
//! [render]
//! backend = "auto"      # auto (the default), integer, skia, vello-cpu or vello-gpu
//! fallback = true       # step down to a lighter backend if the chosen one does not fit; default true
//! min_free_kb = 100     # internal RAM that must stay free after the frame; default 100 (a guess)
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    render: Option<RawRender>,
}

impl RenderLayer {
    /// The layer a settings file (`fusuma.toml`) makes: its `[render]` table, or an empty layer if it has
    /// none. Nothing else is allowed in the file yet, so a mistyped table name is an error, not silence.
    ///
    /// # Errors
    /// [`SettingsError`] for text that is not valid TOML, an unknown key or table, or an unknown backend.
    pub fn from_toml(text: &str) -> Result<Self, SettingsError> {
        let file: File = toml::from_str(text)
            .map_err(|e| SettingsError(std::string::ToString::to_string(&e)))?;
        file.render.map_or(Ok(Self::EMPTY), RawRender::into_layer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Render;

    #[test]
    fn a_full_table_is_read() {
        let l = RenderLayer::from_toml(
            "[render]\nbackend = \"skia\"\nfallback = false\nmin_free_kb = 64\n",
        )
        .unwrap();
        assert_eq!(l.backend, Some(BackendSetting::Fixed(Backend::Skia)));
        assert_eq!((l.fallback, l.min_free_kb), (Some(false), Some(64)));
    }

    #[test]
    fn what_is_left_out_says_nothing() {
        assert_eq!(RenderLayer::from_toml("").unwrap(), RenderLayer::EMPTY);
        assert_eq!(
            RenderLayer::from_toml("[render]\n").unwrap(),
            RenderLayer::EMPTY
        );
        let l = RenderLayer::from_toml("[render]\nfallback = false\n").unwrap();
        assert_eq!(l.backend, None);
        assert_eq!(
            Render::layered(&[l]).min_free_kb,
            crate::DEFAULT_MIN_FREE_KB
        );
    }

    #[test]
    fn auto_is_written_out_and_every_backend_name_works() {
        let auto = RenderLayer::from_toml("[render]\nbackend = \"auto\"\n").unwrap();
        assert_eq!(auto.backend, Some(BackendSetting::Auto));
        for b in Backend::ALL {
            let l = RenderLayer::from_toml(&std::format!("[render]\nbackend = \"{}\"\n", b.name()))
                .unwrap();
            assert_eq!(l.backend, Some(BackendSetting::Fixed(b)));
        }
    }

    #[test]
    fn mistakes_are_errors_that_name_the_problem() {
        let err = |t: &str| RenderLayer::from_toml(t).unwrap_err().to_string();
        assert!(err("[render]\nbackend = \"vello\"\n").contains("unknown render backend 'vello'"));
        assert!(err("[render]\nbackend = \"vello\"\n").contains("auto, "));
        assert!(err("[render]\nbogus = 1\n").contains("bogus"));
        assert!(
            err("[rendr]\nbackend = \"skia\"\n").contains("rendr"),
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
