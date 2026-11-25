//! The look a settings file asks for: the `[cells]` table as a [`fusuma::CellTheme`].

use fusuma::CellTheme;
use fusuma::platform::CellsChoice;

/// `choice` (the `[cells]` table of `fusuma.toml`) as a cell theme: the preset, then the glyphs and the emphasis it
/// names over it. `fallback` is the theme when the table names no preset (ASCII where the locale is not UTF-8).
///
/// The glyphs live for the rest of the program: a theme is made once at start.
///
/// # Errors
/// A message that names the key, for a preset, glyph, state or attribute that does not exist, or a glyph of a width the
/// layout cannot use.
pub fn cell_theme(choice: &CellsChoice, fallback: CellTheme) -> Result<CellTheme, String> {
    let mut theme = match choice.preset.as_deref() {
        None => fallback,
        Some(name) => CellTheme::choose(name).ok_or_else(|| {
            format!("cells.preset: no preset '{name}'; known: unicode, ascii, mono")
        })?,
    };
    for (name, value) in choice.glyphs.iter().flatten() {
        let value: &'static str = Box::leak(value.clone().into_boxed_str());
        theme
            .set_glyph(name, value)
            .map_err(|e| format!("cells.glyphs.{name}: {}", describe(e)))?;
    }
    for (state, attrs) in choice.emphasis.iter().flatten() {
        theme
            .set_emphasis(state, attrs.iter().map(String::as_str))
            .map_err(|e| format!("cells.emphasis.{state}: {}", describe(e)))?;
    }
    Ok(theme)
}

fn describe(e: fusuma::CellThemeError) -> String {
    use fusuma::CellThemeError::{
        UnknownAttr, UnknownGlyph, UnknownPreset, UnknownState, WrongSize,
    };
    match e {
        UnknownPreset => "no such preset".into(),
        UnknownGlyph => "no such glyph".into(),
        UnknownState => "no such state".into(),
        UnknownAttr => "no such attribute (bold, dim, italic, underline, reverse)".into(),
        WrongSize(n) => format!("must be {n} wide"),
    }
}

#[cfg(test)]
mod tests {
    use fusuma::platform::Settings;

    use super::*;

    fn choice(toml: &str) -> CellsChoice {
        Settings::from_toml(toml).unwrap().cells
    }

    #[test]
    fn a_settings_file_picks_the_preset_and_changes_what_it_names() {
        let t = cell_theme(&choice(""), CellTheme::MONO).unwrap();
        assert_eq!(t, CellTheme::MONO, "nothing named: the fallback");
        let t = cell_theme(
            &choice("[cells]\npreset = \"ascii\"\nglyphs = { check_on = \"[X]\", bar_full = \"=\" }\nemphasis = { hover = [\"reverse\"] }\n"),
            CellTheme::UNICODE,
        )
        .unwrap();
        assert_eq!(t.glyphs.check_on, "[X]");
        assert_eq!(t.glyphs.bar_full, '=');
        assert_eq!(t.glyphs.radio_on, "(*)", "the ascii preset's, untouched");
        assert_eq!(t.emphasis.hover, fusuma::cells::Attrs::REVERSE);
    }

    #[test]
    fn a_mistake_names_the_key() {
        let err = |toml: &str| cell_theme(&choice(toml), CellTheme::UNICODE).unwrap_err();
        assert!(err("[cells]\npreset = \"fancy\"\n").starts_with("cells.preset"));
        assert!(err("[cells]\nglyphs = { blob = \"x\" }\n").starts_with("cells.glyphs.blob"));
        assert!(err("[cells]\nglyphs = { check_on = \"[xx]\" }\n").contains("3 wide"));
        assert!(err("[cells]\nemphasis = { hover = [\"blink\"] }\n").contains("hover"));
        assert!(err("[cells]\nemphasis = { angry = [\"bold\"] }\n").contains("no such state"));
    }
}
