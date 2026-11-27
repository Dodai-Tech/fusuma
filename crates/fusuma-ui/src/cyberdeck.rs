//! The `cyberdeck` design: near-black, white mono text, one orange accent, 1 pixel outlines, square corners, no shadows,
//! and four decorations drawn from straight lines (corner ticks, a hatch on the chosen row, a dotted track, a progress bar
//! round the frame).
//!
//! Everything the design says is in this file in four places, so a generator can write it from a definition kept elsewhere
//! (`id = "cyberdeck"`): [`PALETTE`] (the colours), [`FONTS`] and the tokens in [`THEME`] (shapes and sizes), [`DECOR`] (the
//! decoration), and last the [`hooks`](HOOKS) that map those to widgets.

use fusuma_render::{Color, Font};

use crate::decor::Decor;
use crate::theme::{
    ButtonState, ButtonStyle, ButtonVariant, Class, Colors, Fonts, Hooks, ItemStyle, MotionTimes,
    Palette, PanelKind, PanelStyle, Radii, Spacing, TabStyle, TextSizes, Theme, mix,
};

/// The colours, dark and light.
pub(crate) const PALETTE: Palette = Palette {
    name: "cyberdeck",
    dark: Colors {
        background: Color::rgb(10, 10, 11),
        surface: Color::rgb(17, 17, 19),
        surface_raised: Color::rgb(30, 30, 33),
        border: Color::rgb(60, 60, 66),
        border_strong: Color::rgb(150, 150, 158),
        text: Color::rgb(246, 246, 246),
        text_muted: Color::rgb(166, 166, 172),
        text_disabled: Color::rgb(104, 104, 110),
        // The one accent.
        primary: Color::rgb(255, 90, 31),
        primary_hover: Color::rgb(255, 122, 72),
        primary_active: Color::rgb(226, 76, 20),
        on_primary: Color::rgb(10, 10, 11),
        danger: Color::rgb(255, 84, 112),
        success: Color::rgb(76, 214, 122),
        warning: Color::rgb(255, 196, 0),
        // The small blue tick.
        info: Color::rgb(70, 140, 255),
    },
    light: Colors {
        background: Color::rgb(244, 243, 239),
        surface: Color::rgb(255, 255, 255),
        surface_raised: Color::rgb(234, 232, 227),
        border: Color::rgb(206, 204, 198),
        border_strong: Color::rgb(100, 98, 94),
        text: Color::rgb(16, 16, 18),
        text_muted: Color::rgb(84, 82, 80),
        text_disabled: Color::rgb(150, 148, 144),
        // Darker than the dark theme's orange: white text has to read on it, and a hover goes darker, not lighter.
        primary: Color::rgb(196, 60, 6),
        primary_hover: Color::rgb(170, 50, 4),
        primary_active: Color::rgb(142, 40, 2),
        on_primary: Color::rgb(255, 255, 255),
        danger: Color::rgb(186, 22, 58),
        success: Color::rgb(0, 118, 54),
        warning: Color::rgb(134, 92, 0),
        info: Color::rgb(0, 92, 196),
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

pub(crate) const CYAN: Palette = with_accent(
    "cyberdeck-cyan",
    [
        Color::rgb(0, 229, 255),
        Color::rgb(92, 238, 255),
        Color::rgb(0, 196, 222),
        Color::rgb(4, 12, 14),
    ],
    [
        Color::rgb(0, 100, 124),
        Color::rgb(0, 84, 104),
        Color::rgb(0, 68, 86),
        WHITE,
    ],
);
pub(crate) const LIME: Palette = with_accent(
    "cyberdeck-lime",
    [
        Color::rgb(182, 255, 0),
        Color::rgb(206, 255, 72),
        Color::rgb(150, 222, 0),
        Color::rgb(10, 14, 0),
    ],
    [
        Color::rgb(62, 104, 0),
        Color::rgb(52, 88, 0),
        Color::rgb(42, 72, 0),
        WHITE,
    ],
);
pub(crate) const MAGENTA: Palette = with_accent(
    "cyberdeck-magenta",
    [
        Color::rgb(255, 42, 157),
        Color::rgb(255, 96, 186),
        Color::rgb(240, 36, 146),
        Color::rgb(10, 10, 11),
    ],
    [
        Color::rgb(172, 0, 94),
        Color::rgb(146, 0, 80),
        Color::rgb(120, 0, 66),
        WHITE,
    ],
);
pub(crate) const AMBER: Palette = with_accent(
    "cyberdeck-amber",
    [
        Color::rgb(255, 176, 0),
        Color::rgb(255, 200, 70),
        Color::rgb(238, 158, 0),
        Color::rgb(20, 12, 0),
    ],
    [
        Color::rgb(140, 80, 0),
        Color::rgb(118, 66, 0),
        Color::rgb(96, 54, 0),
        WHITE,
    ],
);
pub(crate) const VIOLET: Palette = with_accent(
    "cyberdeck-violet",
    [
        Color::rgb(160, 112, 255),
        Color::rgb(186, 148, 255),
        Color::rgb(148, 98, 250),
        Color::rgb(10, 8, 20),
    ],
    [
        Color::rgb(88, 36, 196),
        Color::rgb(74, 28, 170),
        Color::rgb(60, 20, 146),
        WHITE,
    ],
);

/// Mono for every kind of text. One place to change it for a proportional body.
pub(crate) const FONTS: Fonts = Fonts {
    body: Font::Mono,
    title: Font::Mono,
    mono: Font::Mono,
    icons: Font::Icons,
};

/// The decoration: ticks of 6 pixels, a hatch line every 5, a dot every 3.
pub(crate) const DECOR: Decor = Decor {
    ticks: 8,
    hatch: 5,
    dots: 3,
    frame: true,
};

/// The hooks that put the palette on the widgets.
pub(crate) const HOOKS: Hooks = Hooks {
    button: Some(button),
    item: Some(item),
    tab: Some(tab),
    panel: Some(panel),
    ..Hooks::NONE
};

/// The theme: [`Theme::DARK`] with this design's shapes, sizes and hooks (the colours come from [`PALETTE`]).
pub(crate) const THEME: Theme = Theme {
    colors: PALETTE.dark,
    spacing: Spacing {
        xs: 2,
        sm: 4,
        md: 8,
        lg: 14,
    },
    radii: Radii {
        sm: 0,
        md: 2,
        lg: 3,
    },
    text: TextSizes {
        caption: 10,
        body: 14,
        title: 18,
        icon: 18,
    },
    fonts: FONTS,
    control_height: 44,
    elevation: 0,
    border_width: 1,
    indicator: 18,
    track_height: 4,
    motion: MotionTimes {
        fast_ms: 80,
        normal_ms: 160,
        slow_ms: 300,
        bar_hold_ms: 300,
        bar_fade_ms: 250,
    },
    hooks: HOOKS,
    decor: DECOR,
    ..Theme::DARK
};

fn button(t: &Theme, _: Class, variant: ButtonVariant, state: ButtonState) -> ButtonStyle {
    let c = &t.colors;
    let r = t.radii.sm;
    match (state, variant) {
        (ButtonState::Disabled, _) => ButtonStyle {
            background: c.surface,
            border: Some(c.border),
            text: c.text_disabled,
            radius: r,
        },
        (s, ButtonVariant::Primary) => ButtonStyle {
            background: match s {
                ButtonState::Hovered => c.primary_hover,
                ButtonState::Pressed => c.primary_active,
                _ => c.primary,
            },
            border: None,
            text: c.on_primary,
            radius: r,
        },
        (s, ButtonVariant::Secondary) => ButtonStyle {
            background: match s {
                ButtonState::Hovered => c.surface_raised,
                ButtonState::Pressed => c.border,
                _ => c.surface,
            },
            border: Some(c.border_strong),
            text: c.text,
            radius: r,
        },
    }
}

/// The chosen row or tab is hatched; where the hatch is not drawn (a glass without greys) it is solid.
fn item(t: &Theme, _: Class, state: ButtonState, selected: bool) -> ItemStyle {
    let c = &t.colors;
    let r = t.radii.sm;
    let (fill, text) = match (state, selected) {
        (ButtonState::Disabled, _) => (None, c.text_disabled),
        (_, true) if t.decor.hatch == 0 => (Some(c.primary), c.on_primary),
        (_, true) => (Some(c.surface), c.text),
        (ButtonState::Pressed, _) => (Some(c.border), c.text),
        (ButtonState::Hovered, _) => (Some(c.surface_raised), c.text),
        (ButtonState::Idle, _) => (None, c.text),
    };
    ItemStyle {
        fill,
        text,
        radius: r,
    }
}

fn tab(t: &Theme, _: Class, state: ButtonState, selected: bool) -> TabStyle {
    let c = &t.colors;
    let hatched = t.decor.hatch != 0;
    let (fill, text) = match (state, selected) {
        (ButtonState::Disabled, _) => (None, c.text_disabled),
        (_, true) if !hatched => (Some(c.primary), c.on_primary),
        (_, true) => (Some(c.surface), c.text),
        (ButtonState::Pressed, _) => (Some(c.border), c.text),
        (ButtonState::Hovered, _) => (Some(c.surface_raised), c.text),
        (ButtonState::Idle, _) => (None, c.text_muted),
    };
    TabStyle {
        fill,
        text,
        indicator: (selected && state != ButtonState::Disabled && hatched).then_some(c.primary),
        radius: t.radii.sm,
    }
}

/// Panels are outlined, flat, and a step lighter than the page.
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

/// The colour of the hatch lines on a chosen row or tab: the accent, thinned so the text on top stays clear.
pub(crate) fn hatch_color(t: &Theme) -> Color {
    mix(t.colors.surface, t.colors.primary, 620)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::{Style, Tree};
    use fusuma_render::{Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    const T: Theme = Theme {
        fonts: Fonts {
            body: Font::Bitmap,
            title: Font::Bitmap,
            mono: Font::Bitmap,
            icons: Font::Bitmap,
        },
        ..THEME
    };

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
    fn the_palette_reads_in_both_modes_and_is_listed() {
        assert!(Palette::ALL.iter().any(|p| p.name == "cyberdeck"));
        assert_eq!(Palette::named("cyberdeck"), Some(&PALETTE));
        for (mode, c) in [PALETTE, CYAN, LIME, MAGENTA, AMBER, VIOLET]
            .into_iter()
            .flat_map(|p| [(p.name, p.dark), (p.name, p.light)])
        {
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
                assert!(k >= 4.5, "{mode}: {what} {k:.2}");
            }
            assert!(
                contrast(c.border_strong, c.surface) >= 3.0,
                "{mode} outline"
            );
        }
    }

    #[test]
    fn the_accents_are_cyberdeck_palettes_and_listed() {
        for p in [CYAN, LIME, MAGENTA, AMBER, VIOLET] {
            assert!(
                p.is_cyberdeck() && Palette::named(p.name) == Some(&p),
                "{}",
                p.name
            );
            assert_eq!(
                p.dark.background, PALETTE.dark.background,
                "only the accent differs"
            );
            assert_ne!(p.dark.primary, PALETTE.dark.primary);
        }
        assert!(!Palette::EMBER.is_cyberdeck());
    }

    #[test]
    fn the_shapes_are_flat_square_and_big_enough_to_touch() {
        assert_eq!((T.radii.sm, T.radii.md, T.radii.lg), (0, 2, 3));
        assert_eq!((T.elevation, T.border_width), (0, 1));
        const { assert!(THEME.control_height >= 44) };
        assert_eq!(
            (THEME.fonts.body, THEME.fonts.title),
            (Font::Mono, Font::Mono)
        );
        assert_ne!(T, Theme::DARK);
        assert_eq!(Theme::DARK.decor, Decor::NONE, "the default is untouched");
        let (p, f) = (T.hooks.panel.unwrap(), T.hooks.item.unwrap());
        assert!(p(&T, 0, PanelKind::Dialog).shadow.is_none());
        assert!(f(&T, 0, ButtonState::Idle, true).fill.is_some());
    }

    #[test]
    fn a_reflective_glass_keeps_the_lines_and_loses_the_patterns() {
        let flat = THEME.decor.flat();
        assert_eq!((flat.hatch, flat.dots), (0, 0));
        assert_eq!((flat.ticks, flat.frame), (THEME.decor.ticks, true));
        // Without the hatch a chosen row is a solid fill, not a grey pattern.
        let t = Theme {
            decor: flat,
            ..THEME
        };
        let look = t.item_in(0, ButtonState::Idle, true);
        assert_eq!(
            (look.fill, look.text),
            (Some(t.colors.primary), t.colors.on_primary)
        );
    }

    fn picture(
        permille: u16,
        w: u32,
        h: u32,
        theme: &Theme,
    ) -> (VelloCpu, Tree<8>, crate::tree::NodeId) {
        let mut tree = Tree::<8>::new();
        let root = tree
            .column(
                None,
                Style::new()
                    .padding(4)
                    .align(crate::tree::CrossAlign::Stretch)
                    .background(crate::theme::Surface::Background),
            )
            .unwrap();
        let bar = tree.progress_bar(Some(root), permille).unwrap();
        let mut s = fusuma_render_vello_cpu::new(w, h).unwrap();
        tree.update(&s, theme, Rect::new(0, 0, w, h));
        s.clear(theme.colors.background).unwrap();
        tree.paint(&mut s, theme).unwrap();
        (s, tree, bar)
    }

    #[test]
    fn a_bar_is_solid_then_dots_and_a_box_is_a_frame() {
        let c = T.colors;
        // A bar: the first half is the accent, the rest is dotted (some columns lit, some dark).
        let (s, tree, bar) = picture(500, 100, 30, &T);
        let r = tree.rect(bar).unwrap();
        let y = r.y + (r.h / 2) as i32;
        assert_eq!(s.pixel(r.x + 5, y), Some(c.primary));
        let lit = (r.x + (r.w / 2) as i32 + 2..r.right())
            .filter(|&x| s.pixel(x, y) == Some(c.border_strong))
            .count();
        assert!(lit > 5 && lit < (r.w / 2) as usize, "dots: {lit}");
        // A frame: nothing in the middle, the accent along the top edge, the track along the bottom.
        let mut tree = Tree::<8>::new();
        let root = tree
            .column(
                None,
                Style::new()
                    .align(crate::tree::CrossAlign::Stretch)
                    .background(crate::theme::Surface::Background),
            )
            .unwrap();
        let framed = tree.progress_bar(Some(root), 400).unwrap();
        tree.set_style(framed, Style::new().height(crate::tree::Length::Fixed(60)));
        let mut s = fusuma_render_vello_cpu::new(100, 60).unwrap();
        tree.update(&s, &T, Rect::new(0, 0, 100, 60));
        s.clear(c.background).unwrap();
        tree.paint(&mut s, &T).unwrap();
        let r = tree.rect(framed).unwrap();
        assert!(r.h > T.track_height * 2 && r.w > 50, "{r:?}");
        {
            assert_eq!(
                s.pixel(r.x + 3, r.y),
                Some(c.primary),
                "the top edge fills first"
            );
            assert_eq!(
                s.pixel(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32),
                Some(c.background)
            );
            assert_eq!(
                s.pixel(r.x + 3, r.bottom() - 1),
                Some(c.border),
                "the end is still track"
            );
        }
    }

    #[test]
    fn ticks_sit_in_the_four_corners_and_nowhere_else() {
        let mut tree = Tree::<8>::new();
        let card = tree
            .column(
                None,
                Style::new()
                    .padding(10)
                    .background(crate::theme::Surface::Surface),
            )
            .unwrap();
        tree.label(Some(card), "x", crate::theme::TextRole::Body)
            .unwrap();
        let mut s = fusuma_render_vello_cpu::new(80, 60).unwrap();
        tree.update(&s, &T, Rect::new(0, 0, 80, 60));
        s.clear(T.colors.background).unwrap();
        tree.paint(&mut s, &T).unwrap();
        let (r, hi) = (tree.rect(card).unwrap(), T.colors.border_strong);
        for (x, y) in [
            (r.x + 2, r.y),
            (r.x, r.y + 2),
            (r.right() - 3, r.bottom() - 1),
            (r.right() - 1, r.bottom() - 3),
        ] {
            assert_eq!(s.pixel(x, y), Some(hi), "tick at {x},{y}");
        }
        // Mid-edge is the quiet outline.
        assert_eq!(s.pixel(r.x + (r.w / 2) as i32, r.y), Some(T.colors.border));
    }

    /// The page painted on the integer renderer of a microcontroller (RGB565) and on a 1 bit glass.
    fn card_on<R: Renderer + Probe>(s: &mut R, theme: &Theme, w: u32, h: u32) {
        let mut tree = Tree::<8>::new();
        let root = tree
            .column(
                None,
                Style::new()
                    .padding(6)
                    .align(crate::tree::CrossAlign::Stretch)
                    .background(crate::theme::Surface::Surface),
            )
            .unwrap();
        tree.list_item(Some(root), "Chosen", true).unwrap();
        tree.progress_bar(Some(root), 500).unwrap();
        tree.update(s, theme, Rect::new(0, 0, w, h));
        s.clear(theme.colors.background).unwrap();
        tree.paint(s, theme).unwrap();
    }

    #[test]
    fn it_draws_on_the_integer_renderer_of_a_microcontroller() {
        use fusuma_render_integer::{Embedded, Frame};
        let (w, h) = (120u32, 90u32);
        let frame = Frame::new(w as u16, h as u16, std::vec![0u16; (w * h) as usize]).unwrap();
        let mut s = Embedded::new(frame);
        card_on(&mut s, &T, w, h);
        let lit = (0..h as i32)
            .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                // RGB565 keeps the top 5 or 6 bits of a channel.
                s.pixel(x, y).is_some_and(|p| {
                    let c = T.colors.primary;
                    p.r.abs_diff(c.r) < 9 && p.g.abs_diff(c.g) < 5 && p.b.abs_diff(c.b) < 9
                })
            })
            .count();
        assert!(lit > 50, "the accent is drawn: {lit}");
    }

    #[test]
    fn on_a_one_bit_glass_the_patterns_are_gone_and_only_ink_and_paper_remain() {
        use fusuma_render_integer::{Dither, MonoFrame};
        let (w, h) = (120u32, 90u32);
        let paper = Palette::PAPER.dark;
        let flat = Theme {
            colors: paper,
            decor: THEME.decor.flat(),
            fonts: T.fonts,
            ..THEME
        };
        let bits = std::vec![0u8; MonoFrame::<std::vec::Vec<u8>>::stride_of(w as u16) * h as usize];
        let mut s = fusuma_render_integer::Embedded::new(
            MonoFrame::new(w as u16, h as u16, bits, Dither::Threshold(128)).unwrap(),
        );
        card_on(&mut s, &flat, w, h);
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let p = s.pixel(x, y).unwrap();
                assert!(
                    p == Color::rgb(0, 0, 0) || p == Color::rgb(255, 255, 255),
                    "({x},{y}) is a grey {p:?}"
                );
            }
        }
    }
}
