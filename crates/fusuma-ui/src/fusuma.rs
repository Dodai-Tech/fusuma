//! The `fusuma` design, named for the sliding paper door of a Japanese house: sumi ink (a warm near-black) and washi paper
//! (a warm off-white), one vermilion accent, small corners, no shadows, thin outlines. Two decorations belong to it: a
//! second, thin outline inside cards and dialogs like the frame of a shoji screen ([`Decor::lattice`]), and a bar on the
//! left edge of the chosen list item like a seal mark ([`Decor::bar`]). Tabs are underlined in the accent.
//!
//! The same four places as [`cyberdeck`](crate::cyberdeck): [`PALETTE`], [`THEME`] (shapes and sizes), [`DECOR`] and the
//! [`HOOKS`].

use fusuma_render::Color;

use crate::decor::Decor;
use crate::theme::{
    ButtonState, ButtonStyle, ButtonVariant, Class, Colors, Fonts, Hooks, ItemStyle, MotionTimes,
    Palette, PanelKind, PanelStyle, Radii, Spacing, TabStyle, TextSizes, Theme,
};

/// Sumi (dark) and washi (light), with vermilion (urushi red) as the main action.
pub(crate) const PALETTE: Palette = Palette {
    name: "washi",
    dark: Colors {
        background: Color::rgb(20, 17, 15),
        surface: Color::rgb(28, 24, 21),
        surface_raised: Color::rgb(40, 34, 30),
        border: Color::rgb(66, 58, 51),
        border_strong: Color::rgb(130, 118, 106),
        text: Color::rgb(239, 232, 221),
        text_muted: Color::rgb(168, 156, 143),
        text_disabled: Color::rgb(112, 103, 94),
        // Lifted a little from the light mode's red so it glows on ink.
        primary: Color::rgb(236, 84, 68),
        primary_hover: Color::rgb(244, 112, 96),
        primary_active: Color::rgb(224, 76, 60),
        on_primary: Color::rgb(16, 12, 10),
        // A deeper, pinker red than the accent, so an error is not mistaken for the main action.
        danger: Color::rgb(255, 104, 136),
        success: Color::rgb(138, 190, 104),
        warning: Color::rgb(226, 178, 72),
        info: Color::rgb(130, 150, 218),
    },
    light: Colors {
        background: Color::rgb(244, 238, 226),
        surface: Color::rgb(251, 247, 238),
        surface_raised: Color::rgb(238, 230, 215),
        border: Color::rgb(216, 205, 184),
        border_strong: Color::rgb(136, 122, 100),
        text: Color::rgb(42, 36, 32),
        text_muted: Color::rgb(104, 94, 84),
        text_disabled: Color::rgb(160, 150, 136),
        primary: Color::rgb(200, 55, 45),
        primary_hover: Color::rgb(176, 44, 36),
        primary_active: Color::rgb(148, 34, 28),
        on_primary: Color::rgb(255, 255, 255),
        danger: Color::rgb(168, 20, 64),
        success: Color::rgb(58, 108, 44),
        warning: Color::rgb(132, 92, 8),
        info: Color::rgb(52, 70, 150),
    },
};

/// This design with another accent: the main action's four colours for the dark and the light mode.
const fn with_accent(name: &'static str, dark: [Color; 4], light: [Color; 4]) -> Palette {
    Palette {
        name,
        dark: Colors {
            primary: dark[0],
            primary_hover: dark[1],
            primary_active: dark[2],
            on_primary: dark[3],
            ..PALETTE.dark
        },
        light: Colors {
            primary: light[0],
            primary_hover: light[1],
            primary_active: light[2],
            on_primary: light[3],
            ..PALETTE.light
        },
    }
}

const WHITE: Color = Color::rgb(255, 255, 255);
const INK: Color = Color::rgb(16, 12, 10);

/// Ai-iro, indigo.
pub(crate) const INDIGO: Palette = with_accent(
    "washi-indigo",
    [
        Color::rgb(132, 152, 238),
        Color::rgb(160, 176, 246),
        Color::rgb(118, 138, 228),
        INK,
    ],
    [
        Color::rgb(48, 66, 156),
        Color::rgb(40, 56, 134),
        Color::rgb(32, 46, 112),
        WHITE,
    ],
);
/// Matcha, powdered green tea.
pub(crate) const MATCHA: Palette = with_accent(
    "washi-matcha",
    [
        Color::rgb(144, 196, 108),
        Color::rgb(166, 210, 134),
        Color::rgb(128, 182, 94),
        INK,
    ],
    [
        Color::rgb(58, 108, 44),
        Color::rgb(48, 92, 36),
        Color::rgb(38, 76, 28),
        WHITE,
    ],
);
/// Kincha, gold.
pub(crate) const KINCHA: Palette = with_accent(
    "washi-kincha",
    [
        Color::rgb(230, 184, 76),
        Color::rgb(240, 202, 110),
        Color::rgb(216, 168, 58),
        INK,
    ],
    [
        Color::rgb(126, 88, 6),
        Color::rgb(106, 74, 4),
        Color::rgb(88, 60, 2),
        WHITE,
    ],
);
/// Sakura, cherry blossom.
pub(crate) const SAKURA: Palette = with_accent(
    "washi-sakura",
    [
        Color::rgb(244, 150, 176),
        Color::rgb(248, 176, 196),
        Color::rgb(236, 130, 160),
        INK,
    ],
    [
        Color::rgb(168, 40, 92),
        Color::rgb(144, 32, 78),
        Color::rgb(120, 26, 64),
        WHITE,
    ],
);

/// The decoration: a lattice line 4 pixels inside cards and dialogs, a 4 pixel bar on the chosen item.
pub(crate) const DECOR: Decor = Decor {
    ticks: 0,
    hatch: 0,
    dots: 0,
    frame: false,
    lattice: 4,
    bar: 4,
};

/// The hooks that put the palette on the widgets.
pub(crate) const HOOKS: Hooks = Hooks {
    button: Some(button),
    item: Some(item),
    tab: Some(tab),
    panel: Some(panel),
    ..Hooks::NONE
};

/// The theme: [`Theme::DARK`]'s sizes with small corners, thin outlines, no shadow and this design's hooks.
pub(crate) const THEME: Theme = Theme {
    colors: PALETTE.dark,
    radii: Radii {
        sm: 2,
        md: 3,
        lg: 4,
    },
    spacing: Spacing {
        xs: 2,
        sm: 6,
        md: 12,
        lg: 20,
    },
    text: TextSizes {
        caption: 11,
        body: 15,
        title: 21,
        icon: 20,
    },
    fonts: Fonts::DEFAULT,
    control_height: 44,
    elevation: 0,
    border_width: 1,
    indicator: 20,
    track_height: 6,
    motion: MotionTimes {
        fast_ms: 100,
        normal_ms: 240,
        slow_ms: 420,
        bar_hold_ms: 300,
        bar_fade_ms: 250,
    },
    hooks: HOOKS,
    decor: DECOR,
    ..Theme::DARK
};

fn button(t: &Theme, _: Class, variant: ButtonVariant, state: ButtonState) -> ButtonStyle {
    let c = &t.colors;
    let radius = t.radii.sm;
    match (state, variant) {
        (ButtonState::Disabled, _) => ButtonStyle {
            background: c.surface,
            border: Some(c.border),
            text: c.text_disabled,
            radius,
        },
        (s, ButtonVariant::Primary) => ButtonStyle {
            background: match s {
                ButtonState::Hovered => c.primary_hover,
                ButtonState::Pressed => c.primary_active,
                _ => c.primary,
            },
            border: None,
            text: c.on_primary,
            radius,
        },
        (s, ButtonVariant::Secondary) => ButtonStyle {
            background: match s {
                ButtonState::Hovered => c.surface_raised,
                ButtonState::Pressed => c.border,
                _ => c.surface,
            },
            border: Some(c.border_strong),
            text: c.text,
            radius,
        },
        (s, ButtonVariant::Ghost) => ButtonStyle {
            background: match s {
                ButtonState::Hovered => c.surface_raised,
                ButtonState::Pressed => c.border,
                _ => c.surface_raised.with_alpha(0),
            },
            border: None,
            text: c.text,
            radius,
        },
    }
}

/// The chosen row is a step lighter than the page with the bar of [`Decor::bar`] on its edge.
fn item(t: &Theme, _: Class, state: ButtonState, selected: bool) -> ItemStyle {
    let c = &t.colors;
    let (fill, text) = match (state, selected) {
        (ButtonState::Disabled, _) => (None, c.text_disabled),
        (_, true) => (Some(c.surface_raised), c.text),
        (ButtonState::Pressed, _) => (Some(c.border), c.text),
        (ButtonState::Hovered, _) => (Some(c.surface_raised), c.text),
        (ButtonState::Idle, _) => (None, c.text),
    };
    ItemStyle {
        fill,
        text,
        radius: t.radii.sm,
    }
}

/// Tabs have no fill; the chosen one is in full ink with an underline in the accent.
fn tab(t: &Theme, _: Class, state: ButtonState, selected: bool) -> TabStyle {
    let c = &t.colors;
    let (fill, text) = match (state, selected) {
        (ButtonState::Disabled, _) => (None, c.text_disabled),
        (_, true) => (None, c.text),
        (ButtonState::Pressed, _) => (Some(c.border), c.text),
        (ButtonState::Hovered, _) => (Some(c.surface_raised), c.text),
        (ButtonState::Idle, _) => (None, c.text_muted),
    };
    TabStyle {
        fill,
        text,
        indicator: (selected && state != ButtonState::Disabled).then_some(c.primary),
        radius: t.radii.sm,
    }
}

/// Panels are paper: a step lighter than the page, thin outline, no shadow.
fn panel(t: &Theme, _: Class, kind: PanelKind) -> PanelStyle {
    let c = &t.colors;
    match kind {
        PanelKind::Dialog | PanelKind::Menu => PanelStyle {
            fill: c.surface,
            border: Some(c.border_strong),
            text: c.text,
            radius: t.radii.md,
            shadow: None,
        },
        PanelKind::Toast => PanelStyle {
            fill: c.surface_raised,
            border: Some(c.primary),
            text: c.text,
            radius: t.radii.md,
            shadow: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lum(c: Color) -> f64 {
        let f = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b)
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn every_washi_palette_reads_in_both_modes() {
        for p in [PALETTE, INDIGO, MATCHA, KINCHA, SAKURA] {
            for (mode, c) in [("dark", p.dark), ("light", p.light)] {
                for (what, ink, bg) in [
                    ("text", c.text, c.background),
                    ("text on a card", c.text, c.surface),
                    ("text on a raised card", c.text, c.surface_raised),
                    ("muted", c.text_muted, c.surface),
                    ("on primary", c.on_primary, c.primary),
                    ("on primary hovered", c.on_primary, c.primary_hover),
                    ("on primary pressed", c.on_primary, c.primary_active),
                    ("danger", c.danger, c.surface),
                    ("success", c.success, c.surface),
                    ("warning", c.warning, c.surface),
                    ("info", c.info, c.surface),
                ] {
                    let k = contrast(ink, bg);
                    assert!(k >= 4.5, "{} {mode}: {what} {k:.2}", p.name);
                }
                assert!(
                    contrast(c.border_strong, c.surface) >= 3.0,
                    "{} {mode} outline",
                    p.name
                );
            }
        }
    }

    #[test]
    fn the_accents_are_washi_palettes_and_only_the_main_action_differs() {
        for p in [INDIGO, MATCHA, KINCHA, SAKURA] {
            assert!(Palette::named(p.name) == Some(&p), "{}", p.name);
            assert_eq!(p.dark.background, PALETTE.dark.background);
            assert_ne!(p.dark.primary, PALETTE.dark.primary);
        }
    }

    #[test]
    fn the_accent_is_vermilion_and_the_error_is_not() {
        for c in [PALETTE.dark, PALETTE.light] {
            assert!(c.primary.r > 190 && c.primary.g < 100 && c.primary.b < 100);
            assert_ne!(c.danger, c.primary);
            assert!(c.danger.b > c.primary.b, "the error is pinker");
        }
    }

    #[test]
    fn the_shapes_are_small_flat_and_big_enough_to_touch() {
        assert_eq!((THEME.radii.sm, THEME.radii.md, THEME.radii.lg), (2, 3, 4));
        assert_eq!((THEME.elevation, THEME.border_width), (0, 1));
        const { assert!(THEME.control_height >= 44) };
        assert_eq!((THEME.decor.lattice, THEME.decor.bar), (4, 4));
        assert_eq!((THEME.decor.ticks, THEME.decor.hatch), (0, 0));
        assert!(
            THEME.hooks.panel.unwrap()(&THEME, 0, PanelKind::Dialog)
                .shadow
                .is_none()
        );
    }
}
