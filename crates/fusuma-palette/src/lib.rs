#![doc = include_str!("../README.md")]

use fusuma_render::Color;
use fusuma_ui::{Colors, Palette};

/// A colour in OKLCH: lightness 0 to 1, chroma (about 0 to 0.37) and hue in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Oklch {
    /// Perceptual lightness, 0 (black) to 1 (white).
    pub l: f64,
    /// Colourfulness; 0 is a grey.
    pub c: f64,
    /// The hue, in degrees.
    pub h: f64,
}

fn to_linear(v: u8) -> f64 {
    let v = f64::from(v) / 255.0;
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

fn byte(v: f64) -> u8 {
    (from_linear(v) * 255.0).round().clamp(0.0, 255.0) as u8
}

impl Oklch {
    /// The OKLCH of an sRGB colour.
    #[must_use]
    pub fn of(color: Color) -> Self {
        let (r, g, b) = (to_linear(color.r), to_linear(color.g), to_linear(color.b));
        let l = (0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b).cbrt();
        let m = (0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b).cbrt();
        let s = (0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b).cbrt();
        let lab = (
            0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
            1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
            0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
        );
        Self {
            l: lab.0,
            c: lab.1.hypot(lab.2),
            h: lab.2.atan2(lab.1).to_degrees().rem_euclid(360.0),
        }
    }

    /// The linear sRGB of this colour, which may be outside 0 to 1 (outside the gamut).
    fn linear(self) -> (f64, f64, f64) {
        let (a, b) = (
            self.c * self.h.to_radians().cos(),
            self.c * self.h.to_radians().sin(),
        );
        let l = (self.l + 0.396_337_777_4 * a + 0.215_803_757_3 * b).powi(3);
        let m = (self.l - 0.105_561_345_8 * a - 0.063_854_172_8 * b).powi(3);
        let s = (self.l - 0.089_484_177_5 * a - 1.291_485_548_0 * b).powi(3);
        (
            4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s,
            -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s,
            -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701_0 * s,
        )
    }

    /// Whether the colour is inside the sRGB gamut.
    #[must_use]
    pub fn in_gamut(self) -> bool {
        let (r, g, b) = self.linear();
        [r, g, b].iter().all(|v| (-0.000_5..=1.000_5).contains(v))
    }

    /// The colour as sRGB, with its chroma lowered until it fits the gamut (never clipped, so the hue stays).
    #[must_use]
    pub fn to_color(self) -> Color {
        let mut c = self;
        while !c.in_gamut() && c.c > 0.0 {
            c.c = (c.c - 0.002).max(0.0);
        }
        let (r, g, b) = c.linear();
        Color::rgb(byte(r), byte(g), byte(b))
    }
}

/// The relative luminance of a colour, as WCAG defines it.
#[must_use]
pub fn luminance(c: Color) -> f64 {
    0.2126 * to_linear(c.r) + 0.7152 * to_linear(c.g) + 0.0722 * to_linear(c.b)
}

/// The WCAG contrast ratio of two colours, 1 to 21.
#[must_use]
pub fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Which way to look for a lightness: up for a colour on a dark page, down for one on a light page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Toward {
    Lighter,
    Darker,
}

/// The colour of `hue` and `chroma` nearest `start` (in lightness, going `toward`) whose contrast with every one of
/// `against` is at least `ratio`.
fn readable(
    start: f64,
    hue: f64,
    chroma: f64,
    ratio: f64,
    against: &[Color],
    toward: Toward,
) -> Color {
    let step = if toward == Toward::Lighter {
        0.004
    } else {
        -0.004
    };
    let mut l = start;
    loop {
        let c = Oklch {
            l,
            c: chroma,
            h: hue,
        }
        .to_color();
        if against.iter().all(|b| contrast(c, *b) >= ratio) || !(0.0..=1.0).contains(&l) {
            return c;
        }
        l += step;
    }
}

fn at(l: f64, c: f64, h: f64) -> Color {
    Oklch { l, c, h }.to_color()
}

/// The success, warning and info colours for a page of `pages` (its background, surface and raised colours): a green, an
/// amber and a blue, each as near the usual lightness as the contrast of 4.5 on every one of the pages allows.
#[must_use]
pub fn tones(pages: &[Color], dark: bool) -> (Color, Color, Color) {
    let (start, toward) = if dark {
        (0.74, Toward::Lighter)
    } else {
        (0.55, Toward::Darker)
    };
    (
        readable(start, 150.0, 0.15, 4.5, pages, toward),
        readable(start + 0.06, 80.0, 0.15, 4.5, pages, toward),
        readable(start, 250.0, 0.15, 4.5, pages, toward),
    )
}

/// A dark and a light `Colors` from one seed colour. The seed's hue tints the neutrals (by `tint`, a chroma: 0.012 is a
/// whisper, 0 is a pure grey) and colours the main action, at no more chroma than the seed has.
#[must_use]
pub fn generate(seed: Color, tint: f64) -> (Colors, Colors) {
    let s = Oklch::of(seed);
    let hue = s.h;
    // A seed with no colour of its own (a grey) makes a grey palette whose main action is a soft white or near black.
    let colourful = s.c > 0.02;
    let accent_c = if colourful { s.c.min(0.16) } else { 0.0 };
    let tint = if colourful { tint } else { 0.0 };

    // Dark.
    let bg = at(0.16, tint, hue);
    let surface = at(0.20, tint, hue);
    let raised = at(0.26, tint * 1.2, hue);
    let pages = [bg, surface, raised];
    let text = readable(0.96, hue, tint * 0.4, 7.0, &pages, Toward::Lighter);
    let muted = readable(0.76, hue, tint * 1.5, 4.5, &pages, Toward::Lighter);
    // Disabled is weaker than muted but still a text colour, not a ghost.
    let disabled = {
        let mut l = 0.52;
        let mut c = at(l, tint, hue);
        while contrast(c, surface) >= contrast(muted, surface) - 0.5 && l > 0.3 {
            l -= 0.01;
            c = at(l, tint, hue);
        }
        c
    };
    // The main action: the seed's hue, light enough to stand out of the page; its text is whichever of near-black and near-white reads
    // on it in all three states.
    let mut pl = 0.76;
    let (primary, hover, active, on_primary) = loop {
        let primary = at(pl, accent_c, hue);
        let hover = at(pl + 0.05, accent_c, hue);
        let active = at(pl - 0.05, accent_c, hue);
        let dark_ink = at(0.18, accent_c * 0.3, hue);
        let all = [primary, hover, active];
        let ink = [dark_ink, Color::rgb(255, 255, 255)]
            .into_iter()
            .find(|i| all.iter().all(|b| contrast(*i, *b) >= 4.5));
        if let Some(ink) = ink.filter(|_| contrast(primary, bg) >= 3.0) {
            break (primary, hover, active, ink);
        }
        pl += 0.01;
        if pl > 0.95 {
            break (primary, hover, active, dark_ink);
        }
    };
    let dark = Colors {
        background: bg,
        surface,
        surface_raised: raised,
        border: at(0.34, tint, hue),
        border_strong: at(0.58, tint * 1.2, hue),
        text,
        text_muted: muted,
        text_disabled: disabled,
        primary,
        primary_hover: hover,
        primary_active: active,
        on_primary,
        danger: readable(0.72, 25.0, 0.17, 4.5, &[surface, raised], Toward::Lighter),
        success: tones(&pages, true).0,
        warning: tones(&pages, true).1,
        info: tones(&pages, true).2,
    };

    // Light.
    let bg = at(0.975, tint * 0.5, hue);
    let surface = Color::rgb(255, 255, 255);
    let raised = at(0.94, tint * 0.8, hue);
    let pages = [bg, surface, raised];
    let text = readable(0.22, hue, tint * 0.8, 7.0, &pages, Toward::Darker);
    let muted = readable(0.46, hue, tint * 1.2, 4.5, &pages, Toward::Darker);
    let disabled = {
        let mut l = 0.72;
        let mut c = at(l, tint, hue);
        while contrast(c, surface) >= contrast(muted, surface) - 0.5 && l < 0.9 {
            l += 0.01;
            c = at(l, tint, hue);
        }
        c
    };
    let mut pl = 0.52;
    let (primary, hover, active) = loop {
        let primary = at(pl, accent_c, hue);
        let hover = at(pl - 0.03, accent_c, hue);
        let active = at(pl - 0.08, accent_c, hue);
        let all = [primary, hover, active];
        let white = Color::rgb(255, 255, 255);
        if all.iter().all(|b| contrast(white, *b) >= 4.5) && contrast(primary, bg) >= 3.0 {
            break (primary, hover, active);
        }
        pl -= 0.01;
        if pl < 0.2 {
            break (primary, hover, active);
        }
    };
    let light = Colors {
        background: bg,
        surface,
        surface_raised: raised,
        border: at(0.89, tint, hue),
        border_strong: at(0.58, tint * 1.2, hue),
        text,
        text_muted: muted,
        text_disabled: disabled,
        primary,
        primary_hover: hover,
        primary_active: active,
        on_primary: Color::rgb(255, 255, 255),
        danger: readable(
            0.56,
            25.0,
            0.17,
            4.5,
            &[bg, surface, raised],
            Toward::Darker,
        ),
        success: tones(&pages, false).0,
        warning: tones(&pages, false).1,
        info: tones(&pages, false).2,
    };
    (dark, light)
}

/// A `Palette` called `name` made from `seed` (a name that lives for ever: a constant, or one leaked on purpose).
#[must_use]
pub fn palette(name: &'static str, seed: Color, tint: f64) -> Palette {
    let (dark, light) = generate(seed, tint);
    Palette { name, dark, light }
}

fn rgb(c: Color) -> String {
    format!("Color::rgb({}, {}, {})", c.r, c.g, c.b)
}

fn colors_source(c: &Colors, indent: &str) -> String {
    let rows = [
        ("background", c.background),
        ("surface", c.surface),
        ("surface_raised", c.surface_raised),
        ("border", c.border),
        ("border_strong", c.border_strong),
        ("text", c.text),
        ("text_muted", c.text_muted),
        ("text_disabled", c.text_disabled),
        ("primary", c.primary),
        ("primary_hover", c.primary_hover),
        ("primary_active", c.primary_active),
        ("on_primary", c.on_primary),
        ("danger", c.danger),
        ("success", c.success),
        ("warning", c.warning),
        ("info", c.info),
    ];
    rows.iter()
        .map(|(n, v)| format!("{indent}{n}: {},\n", rgb(*v)))
        .collect()
}

/// The Rust of a `Palette` constant for `name`, to commit as a table.
#[must_use]
pub fn rust_source(name: &str, seed: Color, tint: f64) -> String {
    let (dark, light) = generate(seed, tint);
    let upper = name.to_uppercase();
    format!(
        "    pub const {upper}: Self = Self {{\n        name: \"{name}\",\n        dark: Colors {{\n{}        }},\n        light: Colors {{\n{}        }},\n    }};\n",
        colors_source(&dark, "            "),
        colors_source(&light, "            "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeds() -> Vec<(String, Color)> {
        let mut v: Vec<(String, Color)> = Palette::ALL
            .iter()
            .map(|p| (p.name.to_owned(), p.dark.primary))
            .collect();
        // A sweep of hues at a few strengths, and the greys.
        for h in (0..360).step_by(15) {
            for (c, l) in [(0.05, 0.6), (0.12, 0.7), (0.2, 0.55)] {
                v.push((
                    format!("hue {h} chroma {c}"),
                    Oklch {
                        l,
                        c,
                        h: f64::from(h),
                    }
                    .to_color(),
                ));
            }
        }
        for g in [0u8, 40, 128, 220, 255] {
            v.push((format!("grey {g}"), Color::rgb(g, g, g)));
        }
        v
    }

    #[test]
    fn oklch_round_trips_and_pulls_colours_into_the_gamut_without_changing_their_hue() {
        for c in [
            Color::rgb(255, 0, 0),
            Color::rgb(10, 200, 120),
            Color::rgb(96, 165, 250),
            Color::rgb(0, 0, 0),
            Color::rgb(255, 255, 255),
        ] {
            let back = Oklch::of(c).to_color();
            for (a, b) in [(c.r, back.r), (c.g, back.g), (c.b, back.b)] {
                assert!(a.abs_diff(b) <= 1, "{c:?} came back as {back:?}");
            }
        }
        // A colour far outside the gamut keeps its hue.
        let wild = Oklch {
            l: 0.7,
            c: 0.4,
            h: 140.0,
        };
        assert!(!wild.in_gamut());
        let tamed = Oklch::of(wild.to_color());
        assert!((tamed.h - 140.0).abs() < 4.0, "hue {}", tamed.h);
        assert!(tamed.c < 0.4 && tamed.c > 0.1);
        // White is 21 to 1 against black, and nothing contrasts with itself.
        assert!((contrast(Color::rgb(255, 255, 255), Color::rgb(0, 0, 0)) - 21.0).abs() < 0.01);
        assert!((contrast(Color::rgb(9, 9, 9), Color::rgb(9, 9, 9)) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn every_generated_palette_is_readable_where_text_sits_in_both_themes() {
        for (name, seed) in seeds() {
            let (dark, light) = generate(seed, 0.012);
            for (mode, c) in [("dark", dark), ("light", light)] {
                for (what, bg) in [
                    ("background", c.background),
                    ("surface", c.surface),
                    ("raised", c.surface_raised),
                ] {
                    assert!(
                        contrast(c.text, bg) >= 7.0,
                        "{name} {mode}: text on {what} {:.2}",
                        contrast(c.text, bg)
                    );
                    assert!(
                        contrast(c.text_muted, bg) >= 4.5,
                        "{name} {mode}: muted on {what} {:.2}",
                        contrast(c.text_muted, bg)
                    );
                }
                for (state, bg) in [
                    ("rest", c.primary),
                    ("hover", c.primary_hover),
                    ("pressed", c.primary_active),
                ] {
                    assert!(
                        contrast(c.on_primary, bg) >= 4.5,
                        "{name} {mode}: text on the main action at {state} {:.2}",
                        contrast(c.on_primary, bg)
                    );
                }
                assert!(
                    contrast(c.primary, c.background) >= 3.0,
                    "{name} {mode}: the main action against the page {:.2}",
                    contrast(c.primary, c.background)
                );
                assert!(
                    contrast(c.danger, c.surface) >= 3.0,
                    "{name} {mode}: danger {:.2}",
                    contrast(c.danger, c.surface)
                );
                assert!(
                    contrast(c.text_disabled, c.surface) < contrast(c.text_muted, c.surface),
                    "{name} {mode}: disabled is weaker than muted"
                );
                assert_ne!(c.text_disabled, c.surface);
            }
        }
    }

    #[test]
    fn the_neutrals_take_the_seeds_hue_and_the_main_action_is_the_seed() {
        let blue = Color::rgb(60, 110, 230);
        let (dark, light) = generate(blue, 0.012);
        let seed_hue = Oklch::of(blue).h;
        for c in [dark.primary, light.primary] {
            let o = Oklch::of(c);
            assert!(
                (o.h - seed_hue).abs() < 6.0,
                "the main action keeps the hue: {} against {seed_hue}",
                o.h
            );
            assert!(o.c > 0.08, "and is coloured: {}", o.c);
        }
        let bg = Oklch::of(dark.background);
        assert!(
            (bg.h - seed_hue).abs() < 25.0 && bg.c > 0.003 && bg.c < 0.03,
            "tinted neutral: {bg:?}"
        );
        // Dark pages are dark and light pages are light, and the dark main action is lighter than the light one.
        assert!(Oklch::of(dark.background).l < 0.25 && Oklch::of(light.background).l > 0.9);
        assert!(Oklch::of(dark.primary).l > Oklch::of(light.primary).l);
    }

    #[test]
    fn a_grey_seed_makes_a_grey_palette_and_no_tint_makes_pure_greys() {
        let (dark, light) = generate(Color::rgb(128, 128, 128), 0.012);
        for c in [
            dark.background,
            dark.surface,
            dark.text,
            light.background,
            dark.primary,
            light.primary,
        ] {
            assert!(Oklch::of(c).c < 0.01, "{c:?} is a grey");
        }
        let (d, _) = generate(Color::rgb(60, 110, 230), 0.0);
        assert!(Oklch::of(d.background).c < 0.004);
        assert!(
            Oklch::of(d.primary).c > 0.05,
            "the main action is still coloured"
        );
    }

    #[test]
    fn the_rust_of_a_palette_names_every_colour_and_fits_the_palette_type() {
        let src = rust_source("seagreen", Color::rgb(46, 139, 87), 0.012);
        assert!(src.starts_with("    pub const SEAGREEN: Self = Self {"));
        for field in [
            "background",
            "surface_raised",
            "text_disabled",
            "primary_active",
            "on_primary",
            "danger",
        ] {
            assert_eq!(
                src.matches(&format!("{field}:")).count(),
                2,
                "{field} in dark and light"
            );
        }
        assert!(src.contains("name: \"seagreen\""));
        // A palette made at run time is the same colours.
        let p = palette("seagreen", Color::rgb(46, 139, 87), 0.012);
        let (d, l) = generate(Color::rgb(46, 139, 87), 0.012);
        assert_eq!((p.dark, p.light, p.name), (d, l, "seagreen"));
    }
}
