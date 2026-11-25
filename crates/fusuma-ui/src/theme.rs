//! Theme tokens: everything a widget needs to look like something, as plain data.

use fusuma_render::{Color, Font, TextStyle};

/// Colours as semantic roles, never as brand values: a widget asks for "the text colour", not for
/// a grey.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    /// The page behind everything.
    pub background: Color,
    /// Cards and panels.
    pub surface: Color,
    /// Things lifted above a surface, such as a secondary button.
    pub surface_raised: Color,
    /// Quiet lines.
    pub border: Color,
    /// Stronger lines, such as a hovered outline.
    pub border_strong: Color,
    /// Body text.
    pub text: Color,
    /// Secondary text.
    pub text_muted: Color,
    /// Text of something that cannot be used.
    pub text_disabled: Color,
    /// The brand colour of the main action.
    pub primary: Color,
    /// The main action under the pointer.
    pub primary_hover: Color,
    /// The main action while pressed.
    pub primary_active: Color,
    /// Text and icons on a primary background.
    pub on_primary: Color,
    /// Destructive or erroneous.
    pub danger: Color,
}

/// Distances in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spacing {
    /// Hairline gaps.
    pub xs: u32,
    /// Between related items.
    pub sm: u32,
    /// Around content.
    pub md: u32,
    /// Between sections.
    pub lg: u32,
}

/// Corner radii in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Radii {
    /// Small controls.
    pub sm: u32,
    /// Buttons and cards.
    pub md: u32,
    /// Large panels.
    pub lg: u32,
}

/// Line heights of the text roles, in pixels. A backend picks the closest font it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSizes {
    /// Captions and hints.
    pub caption: u32,
    /// Body text and button labels.
    pub body: u32,
    /// Headings.
    pub title: u32,
    /// Icons ([`TextRole::Icon`]): the side of their square.
    pub icon: u32,
}

/// Durations of motion, in milliseconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MotionTimes {
    /// Hovers and presses.
    pub fast_ms: u32,
    /// Most transitions.
    pub normal_ms: u32,
    /// Large moves and page changes.
    pub slow_ms: u32,
    /// How long a scrollbar stays after the content stopped moving, before it fades.
    pub bar_hold_ms: u32,
    /// How long a scrollbar takes to fade out. 0 hides it at once.
    pub bar_fade_ms: u32,
}

/// How much motion a screen can afford: a chip that redraws slowly looks worse animating than standing still.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MotionBudget {
    /// Every transition at its full length.
    #[default]
    Full,
    /// Transitions at half the length: fewer frames to draw.
    Reduced,
    /// No transitions: everything is there at once.
    Off,
}

/// A complete theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    /// Colour roles.
    pub colors: Colors,
    /// Spacing scale.
    pub spacing: Spacing,
    /// Corner radii.
    pub radii: Radii,
    /// Text sizes.
    pub text: TextSizes,
    /// Which font each kind of text uses.
    pub fonts: Fonts,
    /// Height of a button or an input, in pixels; also the smallest touch target.
    pub control_height: u32,
    /// Width of outlines, in pixels.
    pub border_width: u32,
    /// Side of the box of a checkbox and the diameter of a radio button, in pixels.
    pub indicator: u32,
    /// Height of a progress bar, in pixels.
    pub track_height: u32,
    /// Motion durations.
    pub motion: MotionTimes,
    /// Functions that replace how a kind of widget looks (see [`Hooks`]).
    pub hooks: Hooks,
}

/// Where a design system plugs in. Tokens (colours, radii, sizes) restyle most of what a theme says; a hook is a
/// function that decides one kind of widget's look itself, in every state, and takes the place of the default. A hook
/// gets the theme it is called for, so it can still use the tokens. Each is `None` in the defaults.
#[derive(Clone, Copy, Debug, Default)]
pub struct Hooks {
    /// How a button looks in a variant and a state.
    pub button: Option<fn(&Theme, ButtonVariant, ButtonState) -> ButtonStyle>,
    /// The colour of a container background.
    pub surface: Option<fn(&Theme, Surface) -> Option<Color>>,
    /// How a list item or a menu row looks in a state, chosen or not.
    pub item: Option<fn(&Theme, ButtonState, bool) -> ItemStyle>,
    /// How a tab looks in a state, chosen or not.
    pub tab: Option<fn(&Theme, ButtonState, bool) -> TabStyle>,
    /// How a dialog, a menu or a toast panel looks.
    pub panel: Option<fn(&Theme, PanelKind) -> PanelStyle>,
}

impl Hooks {
    /// No hooks: every widget looks as the theme's tokens say.
    pub const NONE: Self = Self {
        button: None,
        surface: None,
        item: None,
        tab: None,
        panel: None,
    };
}

/// Two sets of hooks are the same when they hold the same functions.
impl PartialEq for Hooks {
    fn eq(&self, other: &Self) -> bool {
        self.button.map(|f| f as usize) == other.button.map(|f| f as usize)
            && self.surface.map(|f| f as usize) == other.surface.map(|f| f as usize)
            && self.item.map(|f| f as usize) == other.item.map(|f| f as usize)
            && self.tab.map(|f| f as usize) == other.tab.map(|f| f as usize)
            && self.panel.map(|f| f as usize) == other.panel.map(|f| f as usize)
    }
}

impl Eq for Hooks {}

/// How a list item or a menu row looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemStyle {
    /// Fill, if any.
    pub fill: Option<Color>,
    /// Text colour.
    pub text: Color,
    /// Corner radius in pixels.
    pub radius: u32,
}

/// How a tab looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStyle {
    /// Fill, if any.
    pub fill: Option<Color>,
    /// Text and icon colour.
    pub text: Color,
    /// The bar along the bottom edge of the chosen tab, if any.
    pub indicator: Option<Color>,
    /// Corner radius of the fill in pixels.
    pub radius: u32,
}

/// Which floating panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelKind {
    /// A modal dialog.
    Dialog,
    /// A menu.
    Menu,
    /// A toast.
    Toast,
}

/// How a floating panel looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelStyle {
    /// Fill.
    pub fill: Color,
    /// Outline, if any.
    pub border: Option<Color>,
    /// Text colour (a toast's message).
    pub text: Color,
    /// Corner radius in pixels.
    pub radius: u32,
}

/// The font of each kind of text. A backend without that font (or built without vector fonts) draws its bitmap
/// font instead, so the choice is safe everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fonts {
    /// Ordinary and small text, and what widgets show: buttons, lists, fields.
    pub body: Font,
    /// Headings.
    pub title: Font,
    /// Fixed-width text: [`TextRole::Code`].
    pub mono: Font,
    /// Icons: [`TextRole::Icon`].
    pub icons: Font,
}

impl Fonts {
    /// Inter for text, Inter Bold for headings, JetBrains Mono for code, Material Symbols for icons (each the
    /// family the build linked for that role).
    pub const DEFAULT: Self = Self {
        body: Font::Sans,
        title: Font::SansBold,
        mono: Font::Mono,
        icons: Font::Icons,
    };
}

impl Theme {
    /// This theme with its motion cut to `budget`: half the durations, or none. Apply it to the base theme.
    #[must_use]
    pub const fn with_motion(self, budget: MotionBudget) -> Self {
        let f = match budget {
            MotionBudget::Full => 100,
            MotionBudget::Reduced => 50,
            MotionBudget::Off => 0,
        };
        let m = self.motion;
        Self {
            motion: MotionTimes {
                fast_ms: m.fast_ms * f / 100,
                normal_ms: m.normal_ms * f / 100,
                slow_ms: m.slow_ms * f / 100,
                ..m
            },
            ..self
        }
    }

    /// This theme for a display at `percent` scale (100 is one pixel per logical pixel, 200 a 2x screen): every
    /// length is multiplied and rounded, and nothing that was visible disappears (a 1 pixel outline stays at
    /// least 1). Colours and motion are unchanged. `percent` is held between 50 and 800, and 100 returns the
    /// theme as it is. Apply it to the base theme each time, not to an already scaled one.
    #[must_use]
    pub const fn scaled(self, percent: u32) -> Self {
        let p = if percent < 50 {
            50
        } else if percent > 800 {
            800
        } else {
            percent
        };
        if p == 100 {
            return self;
        }
        // Rounded to the nearest pixel, and never below `min`.
        const fn s(v: u32, p: u32, min: u32) -> u32 {
            let r = (v as u64 * p as u64 + 50) / 100;
            let r = if r > u32::MAX as u64 {
                u32::MAX
            } else {
                r as u32
            };
            if r < min { min } else { r }
        }
        Self {
            spacing: Spacing {
                xs: s(self.spacing.xs, p, 1),
                sm: s(self.spacing.sm, p, 1),
                md: s(self.spacing.md, p, 1),
                lg: s(self.spacing.lg, p, 1),
            },
            radii: Radii {
                sm: s(self.radii.sm, p, 0),
                md: s(self.radii.md, p, 0),
                lg: s(self.radii.lg, p, 0),
            },
            text: TextSizes {
                caption: s(self.text.caption, p, 1),
                body: s(self.text.body, p, 1),
                title: s(self.text.title, p, 1),
                icon: s(self.text.icon, p, 1),
            },
            control_height: s(self.control_height, p, 1),
            border_width: s(self.border_width, p, 1),
            indicator: s(self.indicator, p, 1),
            track_height: s(self.track_height, p, 1),
            ..self
        }
    }
}

/// The dark theme drawn with the built-in bitmap font, whatever fonts the build bundles: tests that count
/// pixels or check text metrics must not change with a cargo feature another crate turns on.
/// The four colours of the main action: at rest, hovered, pressed, and the text on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccentColors {
    /// At rest.
    pub primary: Color,
    /// Under the pointer.
    pub hover: Color,
    /// Pressed.
    pub active: Color,
    /// Text and icons on it.
    pub on: Color,
}

/// A main-action colour for the dark and the light theme, to put on any palette ([`Palette::with_accent`]). Every
/// palette's own main action is one ([`Accent::ALL`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accent {
    /// The name settings use (`"forest"`).
    pub name: &'static str,
    /// For the dark theme.
    pub dark: AccentColors,
    /// For the light theme.
    pub light: AccentColors,
}

/// A whole set of colours, for the dark and the light theme: what a theme switcher switches. Named, so settings
/// (`[theme] palette` in `fusuma.toml`) can pick one; every one keeps text readable on every surface and in every
/// state of the main action (the tests check the contrast).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    /// The name settings use (`"forest"`).
    pub name: &'static str,
    /// The dark theme's colours.
    pub dark: Colors,
    /// The light theme's colours.
    pub light: Colors,
}

impl Palette {
    /// Fusuma's neutral blue-grey with a blue main action: the colours of [`Theme::DARK`] and [`Theme::LIGHT`].
    pub const FUSUMA: Self = Self {
        name: "fusuma",
        dark: Theme::DARK.colors,
        light: Theme::LIGHT.colors,
    };

    /// Greens from dark pine to sage, on a warm cream (light) or a near-black green (dark).
    pub const FOREST: Self = Self {
        name: "forest",
        dark: Colors {
            background: Color::rgb(12, 23, 12),
            surface: Color::rgb(19, 36, 19),
            surface_raised: Color::rgb(30, 58, 30),
            border: Color::rgb(44, 95, 45),
            border_strong: Color::rgb(90, 127, 82),
            text: Color::rgb(250, 248, 245),
            text_muted: Color::rgb(197, 213, 176),
            text_disabled: Color::rgb(110, 138, 102),
            primary: Color::rgb(135, 168, 120),
            primary_hover: Color::rgb(165, 191, 151),
            primary_active: Color::rgb(122, 156, 110),
            on_primary: Color::rgb(15, 36, 15),
            danger: Color::rgb(240, 122, 110),
        },
        light: Colors {
            background: Color::rgb(250, 248, 245),
            surface: Color::rgb(255, 255, 255),
            surface_raised: Color::rgb(238, 241, 232),
            border: Color::rgb(197, 213, 176),
            border_strong: Color::rgb(90, 127, 82),
            text: Color::rgb(26, 61, 26),
            text_muted: Color::rgb(75, 107, 68),
            text_disabled: Color::rgb(151, 168, 143),
            primary: Color::rgb(44, 95, 45),
            primary_hover: Color::rgb(52, 107, 53),
            primary_active: Color::rgb(26, 61, 26),
            on_primary: Color::rgb(255, 255, 255),
            danger: Color::rgb(179, 38, 30),
        },
    };
    /// A bright, clear blue on blue-tinted surfaces.
    pub const OCEAN: Self = Self {
        name: "ocean",
        dark: Colors {
            background: Color::rgb(7, 17, 31),
            surface: Color::rgb(13, 27, 48),
            surface_raised: Color::rgb(21, 40, 74),
            border: Color::rgb(36, 64, 107),
            border_strong: Color::rgb(90, 127, 184),
            text: Color::rgb(238, 244, 255),
            text_muted: Color::rgb(169, 190, 220),
            text_disabled: Color::rgb(94, 115, 150),
            primary: Color::rgb(77, 163, 255),
            primary_hover: Color::rgb(116, 184, 255),
            primary_active: Color::rgb(47, 140, 240),
            on_primary: Color::rgb(4, 16, 31),
            danger: Color::rgb(255, 122, 122),
        },
        light: Colors {
            background: Color::rgb(244, 248, 252),
            surface: Color::rgb(255, 255, 255),
            surface_raised: Color::rgb(230, 238, 248),
            border: Color::rgb(195, 212, 234),
            border_strong: Color::rgb(91, 122, 163),
            text: Color::rgb(11, 31, 58),
            text_muted: Color::rgb(63, 88, 120),
            text_disabled: Color::rgb(154, 170, 192),
            primary: Color::rgb(10, 102, 224),
            primary_hover: Color::rgb(10, 92, 208),
            primary_active: Color::rgb(8, 72, 168),
            on_primary: Color::rgb(255, 255, 255),
            danger: Color::rgb(192, 38, 45),
        },
    };
    /// Deep teal, between blue and green.
    pub const PETROL: Self = Self {
        name: "petrol",
        dark: Colors {
            background: Color::rgb(6, 20, 22),
            surface: Color::rgb(11, 31, 34),
            surface_raised: Color::rgb(18, 50, 54),
            border: Color::rgb(31, 76, 81),
            border_strong: Color::rgb(79, 138, 143),
            text: Color::rgb(236, 247, 247),
            text_muted: Color::rgb(166, 201, 203),
            text_disabled: Color::rgb(90, 126, 129),
            primary: Color::rgb(63, 184, 191),
            primary_hover: Color::rgb(99, 200, 206),
            primary_active: Color::rgb(42, 162, 169),
            on_primary: Color::rgb(3, 24, 26),
            danger: Color::rgb(242, 120, 111),
        },
        light: Colors {
            background: Color::rgb(243, 248, 248),
            surface: Color::rgb(255, 255, 255),
            surface_raised: Color::rgb(227, 239, 239),
            border: Color::rgb(184, 211, 212),
            border_strong: Color::rgb(79, 127, 130),
            text: Color::rgb(11, 46, 49),
            text_muted: Color::rgb(60, 97, 100),
            text_disabled: Color::rgb(147, 171, 173),
            primary: Color::rgb(11, 110, 116),
            primary_hover: Color::rgb(19, 121, 127),
            primary_active: Color::rgb(7, 85, 90),
            on_primary: Color::rgb(255, 255, 255),
            danger: Color::rgb(184, 50, 43),
        },
    };
    /// Purple, from deep plum to lavender.
    pub const VIOLET: Self = Self {
        name: "violet",
        dark: Colors {
            background: Color::rgb(18, 13, 28),
            surface: Color::rgb(26, 20, 40),
            surface_raised: Color::rgb(38, 29, 59),
            border: Color::rgb(60, 47, 92),
            border_strong: Color::rgb(125, 106, 168),
            text: Color::rgb(243, 239, 251),
            text_muted: Color::rgb(196, 184, 222),
            text_disabled: Color::rgb(111, 99, 144),
            primary: Color::rgb(180, 155, 255),
            primary_hover: Color::rgb(199, 180, 255),
            primary_active: Color::rgb(157, 128, 245),
            on_primary: Color::rgb(23, 13, 46),
            danger: Color::rgb(255, 122, 138),
        },
        light: Colors {
            background: Color::rgb(248, 246, 252),
            surface: Color::rgb(255, 255, 255),
            surface_raised: Color::rgb(239, 233, 248),
            border: Color::rgb(212, 200, 234),
            border_strong: Color::rgb(122, 104, 163),
            text: Color::rgb(36, 22, 61),
            text_muted: Color::rgb(90, 74, 120),
            text_disabled: Color::rgb(169, 159, 190),
            primary: Color::rgb(107, 63, 209),
            primary_hover: Color::rgb(119, 80, 218),
            primary_active: Color::rgb(85, 48, 173),
            on_primary: Color::rgb(255, 255, 255),
            danger: Color::rgb(184, 38, 58),
        },
    };

    /// Every palette, the default first: the order a switcher steps through.
    pub const ALL: [Self; 5] = [
        Self::FUSUMA,
        Self::FOREST,
        Self::OCEAN,
        Self::PETROL,
        Self::VIOLET,
    ];

    /// The palette called `name`.
    #[must_use]
    pub fn named(name: &str) -> Option<&'static Self> {
        Self::ALL.iter().find(|p| p.name == name)
    }

    /// The palette after this one in [`ALL`](Self::ALL), back to the first after the last: a switcher's next step.
    /// One that is not in the list (made by an app) steps to the first.
    #[must_use]
    pub fn next(&self) -> &'static Self {
        let at = Self::ALL.iter().position(|p| p.name == self.name);
        &Self::ALL[at.map_or(0, |i| (i + 1) % Self::ALL.len())]
    }

    /// The colours of the dark or the light theme.
    #[must_use]
    pub const fn colors(&self, dark: bool) -> Colors {
        if dark { self.dark } else { self.light }
    }

    /// This palette's own main action, as an accent.
    #[must_use]
    pub const fn accent(&self) -> Accent {
        const fn of(c: &Colors) -> AccentColors {
            AccentColors {
                primary: c.primary,
                hover: c.primary_hover,
                active: c.primary_active,
                on: c.on_primary,
            }
        }
        Accent {
            name: self.name,
            dark: of(&self.dark),
            light: of(&self.light),
        }
    }

    /// This palette with `accent`'s main action instead of its own.
    #[must_use]
    pub const fn with_accent(self, accent: &Accent) -> Self {
        const fn put(mut c: Colors, a: &AccentColors) -> Colors {
            c.primary = a.primary;
            c.primary_hover = a.hover;
            c.primary_active = a.active;
            c.on_primary = a.on;
            c
        }
        Self {
            name: self.name,
            dark: put(self.dark, &accent.dark),
            light: put(self.light, &accent.light),
        }
    }
}

/// A palette or accent name that does not exist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeError<'a> {
    /// No palette has this name.
    UnknownPalette(&'a str),
    /// No accent has this name.
    UnknownAccent(&'a str),
}

impl core::fmt::Display for ThemeError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let names = |f: &mut core::fmt::Formatter<'_>| {
            for (i, p) in Palette::ALL.iter().enumerate() {
                write!(f, "{}{}", if i == 0 { "" } else { ", " }, p.name)?;
            }
            Ok(())
        };
        match self {
            Self::UnknownPalette(n) => write!(f, "no palette '{n}'; there are ")?,
            Self::UnknownAccent(n) => write!(f, "no accent '{n}'; there are ")?,
        }
        names(f)
    }
}

impl Palette {
    /// The palette the settings ask for (`[theme] palette`, default [`FUSUMA`](Self::FUSUMA)) with the accent
    /// they ask for (`[theme] accent`) on it.
    ///
    /// # Errors
    /// [`ThemeError`] naming an unknown palette or accent.
    pub fn choose<'a>(
        palette: Option<&'a str>,
        accent: Option<&'a str>,
    ) -> Result<Self, ThemeError<'a>> {
        let p = match palette {
            None => Self::FUSUMA,
            Some(n) => *Self::named(n).ok_or(ThemeError::UnknownPalette(n))?,
        };
        Ok(match accent {
            None => p,
            Some(n) => p.with_accent(Accent::named(n).ok_or(ThemeError::UnknownAccent(n))?),
        })
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::FUSUMA
    }
}

impl Accent {
    /// Every palette's main action, in the palettes' order.
    pub const ALL: [Self; 5] = [
        Palette::FUSUMA.accent(),
        Palette::FOREST.accent(),
        Palette::OCEAN.accent(),
        Palette::PETROL.accent(),
        Palette::VIOLET.accent(),
    ];

    /// The accent called `name`.
    #[must_use]
    pub fn named(name: &str) -> Option<&'static Self> {
        Self::ALL.iter().find(|a| a.name == name)
    }
}

#[cfg(test)]
pub(crate) const BITMAP_TEST: Theme = Theme {
    fonts: Fonts {
        body: Font::Bitmap,
        title: Font::Bitmap,
        mono: Font::Bitmap,
        icons: Font::Bitmap,
    },
    ..Theme::DARK
};

/// What a text is for; decides its size and colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextRole {
    /// A heading.
    Title,
    /// Ordinary text.
    #[default]
    Body,
    /// Small, muted text.
    Caption,
    /// Fixed-width text, for a log or code.
    Code,
    /// An icon: a character of the icon font ([`Tree::icon`](crate::Tree::icon)).
    Icon,
}

/// The background of a container.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Surface {
    /// Transparent: whatever is behind shows.
    #[default]
    None,
    /// The page colour.
    Background,
    /// A card.
    Surface,
    /// Something lifted above a card.
    Raised,
}

/// How round a container's corners are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Round {
    /// Square.
    #[default]
    None,
    /// The small radius.
    Small,
    /// The medium radius.
    Medium,
    /// The large radius.
    Large,
}

/// Which look a button has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// The main action: filled with the brand colour.
    #[default]
    Primary,
    /// An ordinary action: a raised surface with an outline.
    Secondary,
}

/// The state of a button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonState {
    /// At rest.
    Idle,
    /// Under the pointer.
    Hovered,
    /// Being pressed.
    Pressed,
    /// Cannot be used.
    Disabled,
}

/// How a button looks in one state and variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonStyle {
    /// Fill.
    pub background: Color,
    /// Outline, if any.
    pub border: Option<Color>,
    /// Label colour.
    pub text: Color,
    /// Corner radius in pixels.
    pub radius: u32,
}

impl Theme {
    /// The dark default: a neutral blue-grey with a blue main action.
    pub const DARK: Self = Self {
        colors: Colors {
            background: Color::rgb(16, 18, 24),
            surface: Color::rgb(26, 29, 36),
            surface_raised: Color::rgb(38, 42, 52),
            border: Color::rgb(60, 66, 80),
            border_strong: Color::rgb(110, 120, 140),
            text: Color::rgb(235, 238, 245),
            text_muted: Color::rgb(160, 168, 185),
            text_disabled: Color::rgb(100, 107, 122),
            primary: Color::rgb(96, 165, 250),
            primary_hover: Color::rgb(125, 185, 252),
            primary_active: Color::rgb(70, 140, 235),
            on_primary: Color::rgb(8, 12, 24),
            danger: Color::rgb(240, 90, 90),
        },
        spacing: Spacing {
            xs: 2,
            sm: 6,
            md: 12,
            lg: 20,
        },
        radii: Radii {
            sm: 4,
            md: 10,
            lg: 16,
        },
        text: TextSizes {
            caption: 10,
            body: 15,
            title: 20,
            icon: 20,
        },
        fonts: Fonts::DEFAULT,
        control_height: 44,
        border_width: 1,
        indicator: 20,
        track_height: 8,
        motion: MotionTimes {
            fast_ms: 100,
            normal_ms: 220,
            slow_ms: 400,
            // Android's scrollbar delay and fade (ViewConfiguration), the behaviour Material shows.
            bar_hold_ms: 300,
            bar_fade_ms: 250,
        },
        hooks: Hooks::NONE,
    };

    /// The light default: the same shapes with light colours.
    pub const LIGHT: Self = Self {
        colors: Colors {
            background: Color::rgb(245, 246, 250),
            surface: Color::rgb(255, 255, 255),
            surface_raised: Color::rgb(236, 238, 244),
            border: Color::rgb(200, 205, 216),
            border_strong: Color::rgb(120, 128, 145),
            text: Color::rgb(20, 24, 34),
            text_muted: Color::rgb(88, 96, 112),
            text_disabled: Color::rgb(150, 156, 170),
            primary: Color::rgb(30, 100, 220),
            primary_hover: Color::rgb(38, 110, 225),
            primary_active: Color::rgb(22, 80, 190),
            on_primary: Color::rgb(255, 255, 255),
            danger: Color::rgb(190, 40, 40),
        },
        ..Self::DARK
    };

    /// This theme with `colors`: how a palette is put on a theme (`theme.with_colors(palette.colors(dark))`).
    #[must_use]
    pub const fn with_colors(self, colors: Colors) -> Self {
        Self { colors, ..self }
    }

    /// The colour of a container background; `None` for [`Surface::None`].
    #[must_use]
    pub fn surface(&self, surface: Surface) -> Option<Color> {
        match self.hooks.surface {
            Some(f) => f(self, surface),
            None => self.default_surface(surface),
        }
    }

    /// What [`surface`](Self::surface) gives without a hook.
    #[must_use]
    pub const fn default_surface(&self, surface: Surface) -> Option<Color> {
        match surface {
            Surface::None => None,
            Surface::Background => Some(self.colors.background),
            Surface::Surface => Some(self.colors.surface),
            Surface::Raised => Some(self.colors.surface_raised),
        }
    }

    /// The radius of a [`Round`], in pixels.
    #[must_use]
    pub const fn radius(&self, round: Round) -> u32 {
        match round {
            Round::None => 0,
            Round::Small => self.radii.sm,
            Round::Medium => self.radii.md,
            Round::Large => self.radii.lg,
        }
    }

    /// The text style of a role.
    #[must_use]
    pub const fn text_style(&self, role: TextRole) -> TextStyle {
        match role {
            TextRole::Title => {
                TextStyle::new(self.text.title, self.colors.text).font(self.fonts.title)
            }
            TextRole::Body => {
                TextStyle::new(self.text.body, self.colors.text).font(self.fonts.body)
            }
            TextRole::Caption => {
                TextStyle::new(self.text.caption, self.colors.text_muted).font(self.fonts.body)
            }
            TextRole::Code => {
                TextStyle::new(self.text.body, self.colors.text).font(self.fonts.mono)
            }
            TextRole::Icon => {
                TextStyle::new(self.text.icon, self.colors.text).font(self.fonts.icons)
            }
        }
    }

    /// How a button looks.
    #[must_use]
    pub fn button(&self, variant: ButtonVariant, state: ButtonState) -> ButtonStyle {
        match self.hooks.button {
            Some(f) => f(self, variant, state),
            None => self.default_button(variant, state),
        }
    }

    /// How a list item or a menu row looks in `state`, chosen (`selected`) or not.
    #[must_use]
    pub fn item(&self, state: ButtonState, selected: bool) -> ItemStyle {
        if let Some(f) = self.hooks.item {
            return f(self, state, selected);
        }
        let c = &self.colors;
        let (fill, text) = match (state, selected) {
            (ButtonState::Disabled, sel) => (sel.then_some(c.border), c.text_disabled),
            (_, true) => (Some(c.primary), c.on_primary),
            (ButtonState::Pressed, _) => (Some(c.border), c.text),
            (ButtonState::Hovered, _) => (Some(c.surface_raised), c.text),
            (ButtonState::Idle, _) => (None, c.text),
        };
        ItemStyle {
            fill,
            text,
            radius: self.radii.sm,
        }
    }

    /// How a tab looks in `state`, chosen (`selected`) or not.
    #[must_use]
    pub fn tab(&self, state: ButtonState, selected: bool) -> TabStyle {
        if let Some(f) = self.hooks.tab {
            return f(self, state, selected);
        }
        let c = &self.colors;
        let (fill, text) = match state {
            ButtonState::Disabled => (None, c.text_disabled),
            ButtonState::Pressed => (Some(c.border), c.text),
            ButtonState::Hovered => (
                Some(c.surface_raised),
                if selected { c.primary } else { c.text },
            ),
            ButtonState::Idle => (None, if selected { c.primary } else { c.text_muted }),
        };
        let indicator = selected.then_some(if state == ButtonState::Disabled {
            c.border
        } else {
            c.primary
        });
        TabStyle {
            fill,
            text,
            indicator,
            radius: self.radii.sm,
        }
    }

    /// How a floating panel looks.
    #[must_use]
    pub fn panel(&self, kind: PanelKind) -> PanelStyle {
        if let Some(f) = self.hooks.panel {
            return f(self, kind);
        }
        let c = &self.colors;
        match kind {
            PanelKind::Dialog => PanelStyle {
                fill: c.surface_raised,
                border: Some(c.border_strong),
                text: c.text,
                radius: self.radius(Round::Large),
            },
            PanelKind::Menu => PanelStyle {
                fill: c.surface_raised,
                border: Some(c.border_strong),
                text: c.text,
                radius: self.radius(Round::Medium),
            },
            // The reverse of the page, so it stands out on either.
            PanelKind::Toast => PanelStyle {
                fill: c.text,
                border: None,
                text: c.background,
                radius: self.radius(Round::Medium),
            },
        }
    }

    /// What [`button`](Self::button) gives without a hook.
    #[must_use]
    pub const fn default_button(&self, variant: ButtonVariant, state: ButtonState) -> ButtonStyle {
        let c = &self.colors;
        let radius = self.radii.md;
        if matches!(state, ButtonState::Disabled) {
            return ButtonStyle {
                background: c.surface,
                border: Some(c.border),
                text: c.text_disabled,
                radius,
            };
        }
        match variant {
            ButtonVariant::Primary => ButtonStyle {
                background: match state {
                    ButtonState::Hovered => c.primary_hover,
                    ButtonState::Pressed => c.primary_active,
                    _ => c.primary,
                },
                border: None,
                text: c.on_primary,
                radius,
            },
            ButtonVariant::Secondary => ButtonStyle {
                background: match state {
                    ButtonState::Pressed => c.surface,
                    _ => c.surface_raised,
                },
                border: Some(match state {
                    ButtonState::Hovered | ButtonState::Pressed => c.border_strong,
                    _ => c.border,
                }),
                text: c.text,
                radius,
            },
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::DARK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance of an sRGB colour.
    fn luminance(c: Color) -> f64 {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
    }

    /// Every palette in both modes, and every palette with every other's accent on it.
    fn every_palette() -> std::vec::Vec<(std::string::String, Colors)> {
        let mut out = std::vec::Vec::new();
        for p in Palette::ALL {
            for a in Accent::ALL {
                let q = p.with_accent(&a);
                out.push((std::format!("{} with {} dark", p.name, a.name), q.dark));
                out.push((std::format!("{} with {} light", p.name, a.name), q.light));
            }
        }
        out
    }

    #[test]
    fn the_main_action_stands_out_from_the_page() {
        for (name, c) in every_palette() {
            assert!(
                contrast(c.primary, c.background) >= 3.0,
                "{name}: {:.2}",
                contrast(c.primary, c.background)
            );
        }
    }

    #[test]
    fn a_choice_from_the_settings_names_a_mistake() {
        assert_eq!(Palette::choose(None, None), Ok(Palette::FUSUMA));
        assert_eq!(Palette::choose(Some("ocean"), None), Ok(Palette::OCEAN));
        let p = Palette::choose(Some("petrol"), Some("violet")).unwrap();
        assert_eq!(
            (p.light.background, p.light.primary),
            (
                Palette::PETROL.light.background,
                Palette::VIOLET.light.primary
            )
        );
        assert_eq!(
            Palette::choose(Some("green"), None),
            Err(ThemeError::UnknownPalette("green"))
        );
        assert_eq!(
            Palette::choose(None, Some("red")),
            Err(ThemeError::UnknownAccent("red"))
        );
        let e = std::format!("{}", ThemeError::UnknownPalette("green"));
        assert_eq!(
            e,
            "no palette 'green'; there are fusuma, forest, ocean, petrol, violet"
        );
    }

    #[test]
    fn palettes_are_named_step_round_and_take_an_accent() {
        assert_eq!(Palette::default(), Palette::FUSUMA);
        assert_eq!(Palette::FUSUMA.colors(true), Theme::DARK.colors);
        assert_eq!(Palette::FUSUMA.colors(false), Theme::LIGHT.colors);
        assert_eq!(Palette::named("forest"), Some(&Palette::FOREST));
        assert_eq!(Palette::named("Forest"), None);
        let names: std::vec::Vec<&str> = Palette::ALL.iter().map(|p| p.name).collect();
        assert_eq!(names, ["fusuma", "forest", "ocean", "petrol", "violet"]);
        let mut p = &Palette::ALL[0];
        for want in ["forest", "ocean", "petrol", "violet", "fusuma"] {
            p = p.next();
            assert_eq!(p.name, want);
        }
        let own = Palette {
            name: "mine",
            ..Palette::OCEAN
        };
        assert_eq!(
            own.next().name,
            "fusuma",
            "an app's own palette steps to the first"
        );
        let green = Palette::FUSUMA.with_accent(Accent::named("forest").unwrap());
        assert_eq!(green.dark.primary, Palette::FOREST.dark.primary);
        assert_eq!(green.light.on_primary, Palette::FOREST.light.on_primary);
        assert_eq!(
            green.dark.background,
            Palette::FUSUMA.dark.background,
            "only the main action changes"
        );
        assert_eq!(
            Palette::VIOLET.with_accent(&Palette::VIOLET.accent()),
            Palette::VIOLET
        );
        let t = Theme::LIGHT.with_colors(Palette::PETROL.light);
        assert_eq!(
            (t.colors, t.control_height),
            (Palette::PETROL.light, Theme::LIGHT.control_height)
        );
        assert_eq!(
            Accent::ALL.map(|a| a.name),
            ["fusuma", "forest", "ocean", "petrol", "violet"]
        );
    }

    fn contrast(a: Color, b: Color) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn text_is_readable_on_every_surface_in_both_themes() {
        for (name, c) in every_palette() {
            let c = &c;
            for (what, bg) in [
                ("background", c.background),
                ("surface", c.surface),
                ("raised", c.surface_raised),
            ] {
                assert!(
                    contrast(c.text, bg) >= 7.0,
                    "{name}: text on {what}: {:.2}",
                    contrast(c.text, bg)
                );
                assert!(
                    contrast(c.text_muted, bg) >= 4.5,
                    "{name}: muted text on {what}: {:.2}",
                    contrast(c.text_muted, bg)
                );
            }
            assert!(
                contrast(c.danger, c.surface) >= 3.0,
                "{name}: danger on surface"
            );
        }
    }

    #[test]
    fn text_on_the_main_action_is_readable_in_every_state() {
        for (name, c) in every_palette() {
            let c = &c;
            for (state, bg) in [
                ("rest", c.primary),
                ("hover", c.primary_hover),
                ("active", c.primary_active),
            ] {
                assert!(
                    contrast(c.on_primary, bg) >= 4.5,
                    "{name}: on_primary on {state}: {:.2}",
                    contrast(c.on_primary, bg)
                );
            }
        }
    }

    #[test]
    fn disabled_text_is_visibly_weaker_than_body_text() {
        for (_, c) in every_palette() {
            let c = &c;
            assert!(contrast(c.text_disabled, c.surface) < contrast(c.text_muted, c.surface));
            assert_ne!(c.text_disabled, c.surface);
        }
    }

    #[test]
    fn the_touch_target_is_at_least_44_pixels_and_scales_are_ordered() {
        for t in [Theme::DARK, Theme::LIGHT] {
            assert!(t.control_height >= 44);
            assert!(
                t.spacing.xs < t.spacing.sm
                    && t.spacing.sm < t.spacing.md
                    && t.spacing.md < t.spacing.lg
            );
            assert!(t.radii.sm < t.radii.md && t.radii.md < t.radii.lg);
            assert!(t.text.caption < t.text.body && t.text.body < t.text.title);
            assert!(
                t.text.icon >= t.text.body,
                "an icon is not smaller than the text beside it"
            );
            assert!(t.motion.fast_ms < t.motion.normal_ms && t.motion.normal_ms < t.motion.slow_ms);
        }
        assert_eq!(Theme::default(), Theme::DARK);
    }

    #[test]
    fn roles_resolve_to_tokens() {
        let t = Theme::DARK;
        assert_eq!(t.surface(Surface::None), None);
        assert_eq!(t.surface(Surface::Surface), Some(t.colors.surface));
        assert_eq!(t.radius(Round::None), 0);
        assert_eq!(t.radius(Round::Large), t.radii.lg);
        assert_eq!(t.text_style(TextRole::Title).size, t.text.title);
        assert_eq!(t.text_style(TextRole::Caption).color, t.colors.text_muted);
        assert_eq!(t.text_style(TextRole::Body).color, t.colors.text);
    }

    #[test]
    fn button_looks_follow_state_and_variant() {
        let t = Theme::DARK;
        let p = |s| t.button(ButtonVariant::Primary, s);
        assert_eq!(p(ButtonState::Idle).background, t.colors.primary);
        assert_eq!(p(ButtonState::Hovered).background, t.colors.primary_hover);
        assert_eq!(p(ButtonState::Pressed).background, t.colors.primary_active);
        assert_eq!(p(ButtonState::Idle).border, None);
        let s = |st| t.button(ButtonVariant::Secondary, st);
        assert_eq!(s(ButtonState::Idle).background, t.colors.surface_raised);
        assert_eq!(s(ButtonState::Pressed).background, t.colors.surface);
        assert_eq!(s(ButtonState::Hovered).border, Some(t.colors.border_strong));
        for variant in [ButtonVariant::Primary, ButtonVariant::Secondary] {
            let d = t.button(variant, ButtonState::Disabled);
            assert_eq!(
                (d.text, d.background),
                (t.colors.text_disabled, t.colors.surface),
                "disabled looks the same for every variant"
            );
        }
    }

    #[test]
    fn scaling_multiplies_lengths_keeps_colours_and_never_loses_an_outline() {
        let base = Theme::DARK;
        assert_eq!(base.scaled(100), base);
        let big = base.scaled(200);
        assert_eq!(big.control_height, base.control_height * 2);
        assert_eq!(big.spacing.md, base.spacing.md * 2);
        assert_eq!(big.text.body, base.text.body * 2);
        assert_eq!(big.border_width, 2);
        assert_eq!((big.colors, big.motion), (base.colors, base.motion));
        let small = base.scaled(50);
        assert_eq!(small.border_width, 1, "a 1 px outline stays visible");
        assert!(small.spacing.xs >= 1 && small.text.caption >= 1);
        assert_eq!(base.scaled(150).control_height, 66);
    }

    #[test]
    fn scaling_is_held_to_a_sane_range_and_does_not_overflow() {
        let base = Theme::DARK;
        assert_eq!(base.scaled(0), base.scaled(50));
        assert_eq!(base.scaled(u32::MAX), base.scaled(800));
        let mut huge = base;
        huge.control_height = u32::MAX;
        assert_eq!(huge.scaled(800).control_height, u32::MAX);
        assert!(base.scaled(125).radii.sm <= base.scaled(125).radii.md);
    }

    #[test]
    fn each_text_role_has_its_font_and_scaling_keeps_the_fonts() {
        use fusuma_render::Font;
        let th = Theme::DARK;
        assert_eq!(th.text_style(TextRole::Title).font, Font::SansBold);
        assert_eq!(th.text_style(TextRole::Body).font, Font::Sans);
        assert_eq!(th.text_style(TextRole::Caption).font, Font::Sans);
        assert_eq!(th.text_style(TextRole::Code).font, Font::Mono);
        assert_eq!(th.text_style(TextRole::Code).size, th.text.body);
        assert_eq!(
            th.scaled(200).fonts,
            th.fonts,
            "scaling changes lengths, not fonts"
        );
        assert_eq!(Theme::LIGHT.fonts, Fonts::DEFAULT);
        let mut mono_body = th;
        mono_body.fonts.body = Font::Mono;
        assert_eq!(
            mono_body.text_style(TextRole::Body).font,
            Font::Mono,
            "a theme can pick any font"
        );
    }
}

#[cfg(test)]
mod hook_tests {
    use fusuma_render::{Color, Point, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::tree::{CrossAlign, Style, Tree};

    const PINK: Color = Color::rgb(250, 40, 160);
    const LIME: Color = Color::rgb(160, 250, 40);
    const TEAL: Color = Color::rgb(30, 200, 210);
    const GOLD: Color = Color::rgb(240, 190, 20);

    fn button(_: &Theme, v: ButtonVariant, s: ButtonState) -> ButtonStyle {
        ButtonStyle {
            background: match (v, s) {
                (_, ButtonState::Pressed) => LIME,
                (ButtonVariant::Primary, _) => PINK,
                _ => TEAL,
            },
            border: None,
            text: Color::WHITE,
            radius: 3,
        }
    }

    fn item(t: &Theme, s: ButtonState, selected: bool) -> ItemStyle {
        ItemStyle {
            fill: if selected { Some(GOLD) } else { None },
            text: if s == ButtonState::Disabled {
                t.colors.text_disabled
            } else {
                Color::BLACK
            },
            radius: 0,
        }
    }

    fn tab(_: &Theme, _: ButtonState, selected: bool) -> TabStyle {
        TabStyle {
            fill: None,
            text: Color::WHITE,
            indicator: selected.then_some(LIME),
            radius: 0,
        }
    }

    fn panel(t: &Theme, kind: PanelKind) -> PanelStyle {
        PanelStyle {
            fill: GOLD,
            border: None,
            text: Color::BLACK,
            radius: t.radii.sm,
        }
        .tap(kind)
    }

    trait Tap: Sized {
        fn tap(self, _: PanelKind) -> Self {
            self
        }
    }
    impl Tap for PanelStyle {}

    fn surface(_: &Theme, s: Surface) -> Option<Color> {
        match s {
            Surface::None => None,
            _ => Some(PINK),
        }
    }

    fn hooked() -> Theme {
        let mut t = crate::theme::BITMAP_TEST;
        t.hooks = Hooks {
            button: Some(button),
            surface: Some(surface),
            item: Some(item),
            tab: Some(tab),
            panel: Some(panel),
        };
        t
    }

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(220, 260).unwrap()
    }

    #[test]
    fn without_hooks_the_defaults_are_the_tokens_and_a_hook_set_compares_by_its_functions() {
        let plain = crate::theme::BITMAP_TEST;
        assert_eq!(plain.hooks, Hooks::NONE);
        assert_eq!(
            plain.button(ButtonVariant::Primary, ButtonState::Idle),
            plain.default_button(ButtonVariant::Primary, ButtonState::Idle)
        );
        assert_eq!(
            plain.surface(Surface::Raised),
            plain.default_surface(Surface::Raised)
        );
        assert_eq!(
            plain.panel(PanelKind::Toast).fill,
            plain.colors.text,
            "the reverse of the page"
        );
        assert_ne!(hooked(), plain);
        assert_eq!(hooked(), hooked());
        // The theme keeps its hooks through the changes an app makes to it.
        assert_eq!(hooked().scaled(200).hooks, hooked().hooks);
        assert_eq!(
            hooked().with_motion(MotionBudget::Off).hooks,
            hooked().hooks
        );
        let big = hooked().scaled(200);
        assert_eq!(
            big.panel(PanelKind::Menu).radius,
            big.radii.sm,
            "a hook sees the scaled tokens"
        );
    }

    #[test]
    fn a_hook_replaces_how_each_kind_of_widget_is_drawn_in_every_state() {
        let t = hooked();
        let mut tree = Tree::<24>::new();
        let root = tree
            .column(
                None,
                Style::new()
                    .gap(6)
                    .padding(6)
                    .align(CrossAlign::Start)
                    .background(Surface::Surface),
            )
            .unwrap();
        let primary = tree
            .button(Some(root), "Go", ButtonVariant::Primary)
            .unwrap();
        let secondary = tree
            .button(Some(root), "No", ButtonVariant::Secondary)
            .unwrap();
        let row = tree.list_item(Some(root), "Chosen", true).unwrap();
        let plain_row = tree.list_item(Some(root), "Other", false).unwrap();
        let bar = tree.tab_bar(Some(root), Style::new()).unwrap();
        let on = tree.tab(Some(bar), None, "A", true).unwrap();
        let off = tree.tab(Some(bar), None, "B", false).unwrap();
        let dialog = tree.dialog(Some(root), Style::new().padding(10)).unwrap();
        tree.label(Some(dialog), "Hello", crate::theme::TextRole::Body)
            .unwrap();
        let toast = tree.toast(Some(root)).unwrap();
        let mut s = screen();
        let b = Rect::new(0, 0, 220, 260);
        tree.update(&s, &t, b);
        s.clear(t.colors.background).unwrap();
        tree.paint(&mut s, &t).unwrap();
        let at = |tree: &Tree<24>, id, dx: i32, dy: i32| {
            let r = tree.rect(id).unwrap();
            Point::new(r.x + dx, r.y + dy)
        };
        // The page itself: the container's surface comes from the hook.
        assert_eq!(s.pixel(2, 2), Some(PINK), "a container");
        let p = at(&tree, primary, 4, 4);
        assert_eq!(s.pixel(p.x, p.y), Some(PINK));
        let p = at(&tree, secondary, 4, 4);
        assert_eq!(s.pixel(p.x, p.y), Some(TEAL), "the other variant");
        let p = at(&tree, row, 3, 3);
        assert_eq!(s.pixel(p.x, p.y), Some(GOLD), "the chosen row");
        let p = at(&tree, plain_row, 3, 3);
        assert_eq!(
            s.pixel(p.x, p.y),
            Some(PINK),
            "an unchosen row has no fill: the container shows"
        );
        let (ron, roff) = (tree.rect(on).unwrap(), tree.rect(off).unwrap());
        assert_eq!(
            s.pixel(ron.x + (ron.w / 2) as i32, ron.bottom() - 1),
            Some(LIME),
            "the chosen tab's bar"
        );
        assert_eq!(
            s.pixel(roff.x + (roff.w / 2) as i32, roff.bottom() - 1),
            Some(PINK),
            "none under the other"
        );
        // A press shows the hook's pressed look.
        let c = at(&tree, primary, 4, 4);
        tree.pointer(Pointer::Down(c));
        tree.update(&s, &t, b);
        s.clear(t.colors.background).unwrap();
        tree.paint(&mut s, &t).unwrap();
        assert_eq!(s.pixel(c.x, c.y), Some(LIME), "pressed");
        tree.pointer(Pointer::Up(Point::new(0, 250)));
        tree.pointer(Pointer::Leave);
        // The panels: a dialog and a toast, from the panel hook.
        tree.open_dialog(dialog);
        tree.show_toast(toast, "Saved", 1000);
        tree.advance(&t, 500);
        tree.update(&s, &t, b);
        s.clear(t.colors.background).unwrap();
        tree.paint(&mut s, &t).unwrap();
        let d = tree.rect(dialog).unwrap();
        assert_eq!(
            s.pixel(d.x + (d.w / 2) as i32, d.y + 4),
            Some(GOLD),
            "the dialog panel"
        );
        assert_ne!(
            s.pixel(d.x, d.y + (d.h / 2) as i32),
            Some(t.colors.border_strong),
            "and it has no outline"
        );
        let r = tree.rect(toast).unwrap();
        assert_eq!(
            s.pixel(r.x + (r.w / 2) as i32, r.y + 2),
            Some(GOLD),
            "the toast panel"
        );
    }
}
