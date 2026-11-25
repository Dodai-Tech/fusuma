//! What a screen is, for a layout: its size in pixels, its display scale, its shape, and from those the class it
//! belongs to (a watch, a phone, a tablet, a desktop window) and the part of it a layout can use. The classes are
//! decided by logical width, which is the width in pixels divided by the scale, so a 2x phone and a 1x phone are the
//! same class. Differences between devices are layout, never different widgets: an app asks for the class and
//! arranges the same widgets for it.

use fusuma_render::Rect;

use crate::shape::{Insets, Shape};
use crate::theme::Theme;

/// Screens narrower than this, in logical pixels, are a watch: a grid of big tiles, one small page at a time.
pub const WATCH_BELOW: u32 = 480;

/// Screens narrower than this are a phone: no sidebar, pages in one column.
pub const PHONE_BELOW: u32 = 640;

/// Screens narrower than this are a tablet (a 7-inch panel): a sidebar and one page. Wider is a desktop window.
pub const TABLET_BELOW: u32 = 1100;

/// The class of screen, by logical width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A watch: a grid of tiles, then one small page with a back button.
    Watch,
    /// A phone or a narrow window.
    Phone,
    /// A tablet or a small window.
    Tablet,
    /// A desktop window.
    Desktop,
}

impl Class {
    /// The class of a screen `logical_width` pixels wide.
    #[must_use]
    pub const fn of(logical_width: u32) -> Self {
        if logical_width < WATCH_BELOW {
            Self::Watch
        } else if logical_width < PHONE_BELOW {
            Self::Phone
        } else if logical_width < TABLET_BELOW {
            Self::Tablet
        } else {
            Self::Desktop
        }
    }

    /// Whether the screen is too small for a sidebar.
    #[must_use]
    pub const fn is_compact(self) -> bool {
        matches!(self, Self::Watch | Self::Phone)
    }
}

/// A screen: its pixels, its scale and its shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Display scale in percent: 100 is one pixel per logical pixel, 200 a 2x screen.
    pub scale_percent: u32,
    /// The outline of the visible part.
    pub shape: Shape,
}

impl Screen {
    /// A rectangular screen of `width` x `height` pixels at scale 100.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            scale_percent: 100,
            shape: Shape::Rect,
        }
    }

    /// With a display scale in percent (held between 50 and 800).
    #[must_use]
    pub const fn with_scale(self, percent: u32) -> Self {
        Self {
            scale_percent: clamp_scale(percent),
            ..self
        }
    }

    /// With a shape.
    #[must_use]
    pub const fn with_shape(self, shape: Shape) -> Self {
        Self { shape, ..self }
    }

    /// The scale, held between 50 and 800 percent.
    #[must_use]
    pub const fn scale(&self) -> u32 {
        clamp_scale(self.scale_percent)
    }

    /// The width in logical pixels (pixels divided by the scale).
    #[must_use]
    pub const fn logical_width(&self) -> u32 {
        let w = self.width as u64 * 100 / self.scale() as u64;
        if w > u32::MAX as u64 {
            u32::MAX
        } else {
            w as u32
        }
    }

    /// The class of this screen.
    #[must_use]
    pub const fn class(&self) -> Class {
        Class::of(self.logical_width())
    }

    /// The whole screen as a rectangle.
    #[must_use]
    pub const fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.width, self.height)
    }

    /// How far each edge is cut off by the shape (in pixels), so content placed inside stays visible.
    #[must_use]
    pub const fn insets(&self) -> Insets {
        self.shape.safe_insets(self.width, self.height)
    }

    /// The part of the screen a layout can use: the bounds less the insets (never smaller than nothing).
    #[must_use]
    pub fn safe_bounds(&self) -> Rect {
        let i = self.insets();
        let b = self.bounds();
        let w = b.w.saturating_sub(i.left.saturating_add(i.right));
        let h = b.h.saturating_sub(i.top.saturating_add(i.bottom));
        Rect::new(
            i32::try_from(i.left).unwrap_or(i32::MAX),
            i32::try_from(i.top).unwrap_or(i32::MAX),
            w,
            h,
        )
    }

    /// `base` for this screen's scale: every length multiplied (see [`Theme::scaled`]).
    #[must_use]
    pub const fn theme(&self, base: &Theme) -> Theme {
        base.scaled(self.scale())
    }
}

const fn clamp_scale(p: u32) -> u32 {
    if p < 50 {
        50
    } else if p > 800 {
        800
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_class_follows_the_logical_width() {
        for (w, c) in [
            (0, Class::Watch),
            (410, Class::Watch),
            (479, Class::Watch),
            (480, Class::Phone),
            (639, Class::Phone),
            (640, Class::Tablet),
            (1024, Class::Tablet),
            (1099, Class::Tablet),
            (1100, Class::Desktop),
            (u32::MAX, Class::Desktop),
        ] {
            assert_eq!(Class::of(w), c, "{w}");
        }
        assert!(Class::Watch.is_compact() && Class::Phone.is_compact());
        assert!(!Class::Tablet.is_compact() && !Class::Desktop.is_compact());
    }

    #[test]
    fn the_same_pixels_are_another_class_at_another_scale() {
        // 1080 pixels at 2x are 540 logical: a phone; at 1x they are a tablet.
        assert_eq!(Screen::new(1080, 1920).class(), Class::Tablet);
        assert_eq!(
            Screen::new(1080, 1920).with_scale(200).class(),
            Class::Phone
        );
        assert_eq!(
            Screen::new(1080, 1920).with_scale(300).class(),
            Class::Watch
        );
        assert_eq!(Screen::new(410, 502).class(), Class::Watch);
        assert_eq!(Screen::new(1024, 600).class(), Class::Tablet);
        assert_eq!(Screen::new(2560, 1440).class(), Class::Desktop);
        assert_eq!(
            Screen::new(2560, 1440).with_scale(300).class(),
            Class::Tablet
        );
    }

    #[test]
    fn the_scale_is_held_to_a_sane_range_and_nothing_divides_by_zero() {
        let s = Screen::new(1000, 100);
        assert_eq!(s.with_scale(0).scale(), 50);
        assert_eq!(s.with_scale(10_000).scale(), 800);
        assert_eq!(s.with_scale(0).logical_width(), 2000);
        let raw = Screen {
            scale_percent: 0,
            ..s
        };
        assert_eq!(raw.scale(), 50, "even set directly");
        assert_eq!(raw.logical_width(), 2000);
        assert_eq!(
            Screen::new(u32::MAX, 1).with_scale(50).logical_width(),
            u32::MAX
        );
        assert_eq!(Screen::new(0, 0).class(), Class::Watch);
    }

    #[test]
    fn the_safe_area_is_the_screen_less_what_the_corners_cut() {
        let rect = Screen::new(410, 502);
        assert_eq!(rect.safe_bounds(), Rect::new(0, 0, 410, 502));
        assert_eq!(rect.insets(), Insets::all(0));
        let watch = Screen::new(410, 502).with_shape(Shape::Rounded(70));
        let i = watch.insets();
        assert_eq!(i, Shape::Rounded(70).safe_insets(410, 502));
        assert!(i.top > 15 && i.top < 25, "0.293 of 70: {i:?}");
        let safe = watch.safe_bounds();
        assert_eq!(
            safe,
            Rect::new(
                i.left as i32,
                i.top as i32,
                410 - 2 * i.left,
                502 - 2 * i.top
            )
        );
        // A circle: the inscribed square of content.
        let round = Screen::new(466, 466).with_shape(Shape::Circle);
        let safe = round.safe_bounds();
        assert!(safe.w < 466 && safe.w > 300 && safe.w == safe.h, "{safe:?}");
        assert_eq!(safe.x, safe.y);
        // Insets bigger than the screen leave nothing, not a wrap-around.
        let tiny = Screen::new(4, 4).with_shape(Shape::Rounded(1000));
        let safe = tiny.safe_bounds();
        assert!(safe.w <= 4 && safe.h <= 4);
    }

    #[test]
    fn the_theme_is_scaled_by_the_screens_scale() {
        let base = Theme::DARK;
        assert_eq!(Screen::new(800, 600).theme(&base), base);
        let big = Screen::new(800, 600).with_scale(200).theme(&base);
        assert_eq!(big.control_height, base.control_height * 2);
    }
}
