//! Theme tokens: everything a widget needs to look like something, as plain data.

use fusuma_render::{Color, TextStyle};

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
    /// Height of a button or an input, in pixels; also the smallest touch target.
    pub control_height: u32,
    /// Width of outlines, in pixels.
    pub border_width: u32,
    /// Motion durations.
    pub motion: MotionTimes,
}

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
        },
        control_height: 44,
        border_width: 1,
        motion: MotionTimes {
            fast_ms: 100,
            normal_ms: 220,
            slow_ms: 400,
        },
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

    /// The colour of a container background; `None` for [`Surface::None`].
    #[must_use]
    pub const fn surface(&self, surface: Surface) -> Option<Color> {
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
            TextRole::Title => TextStyle::new(self.text.title, self.colors.text),
            TextRole::Body => TextStyle::new(self.text.body, self.colors.text),
            TextRole::Caption => TextStyle::new(self.text.caption, self.colors.text_muted),
        }
    }

    /// How a button looks.
    #[must_use]
    pub const fn button(&self, variant: ButtonVariant, state: ButtonState) -> ButtonStyle {
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

    fn contrast(a: Color, b: Color) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn text_is_readable_on_every_surface_in_both_themes() {
        for (name, t) in [("dark", Theme::DARK), ("light", Theme::LIGHT)] {
            let c = &t.colors;
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
        for (name, t) in [("dark", Theme::DARK), ("light", Theme::LIGHT)] {
            let c = &t.colors;
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
        for t in [Theme::DARK, Theme::LIGHT] {
            let c = &t.colors;
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
}
