//! What a screen is, for a layout: its size in pixels, its display scale, its shape, and from those the class it
//! belongs to (a watch, a phone, a tablet, a desktop window) and the part of it a layout can use. The classes are
//! decided by logical width, which is the width in pixels divided by the scale, so a 2x phone and a 1x phone are the
//! same class. Differences between devices are layout, never different widgets: an app asks for the class and
//! arranges the same widgets for it.

use fusuma_render::Rect;

use crate::shape::{Insets, Shape};
use crate::theme::{MotionBudget, Palette, Theme};

/// Screens narrower than this, in logical pixels, are a watch: a grid of big tiles, one small page at a time.
pub const WATCH_BELOW: u32 = 480;

/// Screens narrower than this are a phone: no sidebar, pages in one column.
pub const PHONE_BELOW: u32 = 640;

/// Screens narrower than this are a tablet (a 7-inch panel): a sidebar and one page. Wider is a desktop window.
pub const TABLET_BELOW: u32 = 1100;

/// Physical short side, in millimetres, under which a screen is a watch (a watch face is 30 to 50 mm across).
pub const WATCH_SHORT_SIDE_MM: u32 = 55;

/// Physical short side under which a screen (that is not a watch) is a phone. A 7-inch tablet is about 90 mm on its short
/// side, the biggest phones about 80.
pub const PHONE_SHORT_SIDE_MM: u32 = 85;

/// Screens narrower than this, in logical pixels, and told nothing else, are guessed to be a [`FormFactor::Micro`] board.
pub const MICRO_BELOW: u32 = 260;

/// What kind of device a screen is. Unlike [`Class`] it does not change when a window is resized: a watch stays a watch.
/// A host that knows says so ([`Screen::with_form_factor`], `form` in a board file); else the physical size decides
/// ([`Screen::with_size_mm`]); else the logical width does. Size alone cannot tell a small round board from a watch, so
/// the small ones are declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormFactor {
    /// A tiny display, often without touch (128x64 and 240x135 boards, e-paper tags): one value or a short list per
    /// screen, driven by a button or an encoder.
    Micro,
    /// A small touch board that is not worn (a 1.28 to 2.8 inch dev board, a smart knob).
    Panel,
    /// A watch: a grid of tiles, one small page at a time.
    Watch,
    /// A phone.
    Phone,
    /// A tablet or a small window.
    Tablet,
    /// A desktop window.
    Desktop,
}

impl FormFactor {
    /// The layout class that belongs to this form factor: the small ones (micro, panel, watch) are all the watch layout.
    #[must_use]
    pub const fn class(self) -> Class {
        match self {
            Self::Micro | Self::Panel | Self::Watch => Class::Watch,
            Self::Phone => Class::Phone,
            Self::Tablet => Class::Tablet,
            Self::Desktop => Class::Desktop,
        }
    }

    /// Whether the screen is too small for a sidebar (everything up to a phone).
    #[must_use]
    pub const fn is_compact(self) -> bool {
        self.class().is_compact()
    }

    /// The form factor named in a board file (`micro`, `panel`, `watch`, `phone`, `tablet`, `desktop`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "micro" => Self::Micro,
            "panel" => Self::Panel,
            "watch" => Self::Watch,
            "phone" => Self::Phone,
            "tablet" => Self::Tablet,
            "desktop" => Self::Desktop,
            _ => return None,
        })
    }

    /// The form factor of a physical short side in millimetres, if that alone says: under [`WATCH_SHORT_SIDE_MM`] a watch,
    /// under [`PHONE_SHORT_SIDE_MM`] a phone. Bigger screens (a tablet, a laptop, a monitor) are told apart by their window,
    /// not their glass, so they give `None`; so does 0, which is "unknown". A micro board or a panel is never found this
    /// way (it is declared).
    #[must_use]
    pub const fn of_short_side_mm(mm: u32) -> Option<Self> {
        if mm == 0 {
            None
        } else if mm < WATCH_SHORT_SIDE_MM {
            Some(Self::Watch)
        } else if mm < PHONE_SHORT_SIDE_MM {
            Some(Self::Phone)
        } else {
            None
        }
    }
}

/// How the glass makes its picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayKind {
    /// It gives off light (LCD, OLED, AMOLED): fast, any colour.
    #[default]
    Emissive,
    /// It reflects light (e-ink, memory LCD): slow to refresh, few levels, no animation.
    Reflective,
}

impl DisplayKind {
    /// The kind named in a board file (`emissive`, `reflective`; `eink` too).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "emissive" => Some(Self::Emissive),
            "reflective" | "eink" | "epaper" => Some(Self::Reflective),
            _ => None,
        }
    }
}

/// How the person drives the device: a set of these (a micro board with touch and a button has two).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs(u8);

impl Inputs {
    /// A touch screen.
    pub const TOUCH: Self = Self(1);
    /// Keys or buttons that move the focus and press.
    pub const KEYS: Self = Self(2);
    /// A rotary encoder or a crown.
    pub const ENCODER: Self = Self(4);
    /// A mouse or a trackpad.
    pub const POINTER: Self = Self(8);
    /// Nothing known.
    pub const NONE: Self = Self(0);

    /// These and `other`.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether all of `other` is in this.
    #[must_use]
    pub const fn has(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether nothing is in it.
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == 0
    }

    /// One input named in a board file (`touch`, `keys`, `encoder`, `pointer`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "touch" => Self::TOUCH,
            "keys" | "buttons" => Self::KEYS,
            "encoder" | "crown" => Self::ENCODER,
            "pointer" | "mouse" => Self::POINTER,
            _ => return None,
        })
    }
}

/// Size class by window width, for resizing: unlike the form factor it follows the window (Android's window size classes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeClass {
    /// Under 600 logical px: one column.
    Compact,
    /// 600 to 839: a phone in landscape, a small tablet; two columns at most.
    Medium,
    /// 840 and wider: a sidebar and a page.
    Expanded,
}

impl SizeClass {
    /// Wider than this is not compact.
    pub const MEDIUM_FROM: u32 = 600;
    /// Wider than this is expanded.
    pub const EXPANDED_FROM: u32 = 840;

    /// The size class of `logical_width`.
    #[must_use]
    pub const fn of(logical_width: u32) -> Self {
        if logical_width < Self::MEDIUM_FROM {
            Self::Compact
        } else if logical_width < Self::EXPANDED_FROM {
            Self::Medium
        } else {
            Self::Expanded
        }
    }
}

/// What a layout needs to know to size its parts for a device, in **pixels** (already scaled).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metrics {
    /// The smallest a thing that is pressed should be, on both sides.
    pub min_target: u32,
    /// The gap to keep between things that are pressed.
    pub gap: u32,
    /// Columns a grid of tiles may have.
    pub columns: u32,
}

impl Metrics {
    /// The metrics for a form factor at a display scale in percent: targets are 48 logical px on a phone or a watch (a
    /// finger), 40 on a small touch panel, 32 for a pointer, and 24 on a micro board, where focus and not touch drives it.
    #[must_use]
    pub const fn of(form: FormFactor, scale_percent: u32) -> Self {
        let (target, gap, columns) = match form {
            FormFactor::Micro => (24, 2, 1),
            FormFactor::Panel => (40, 4, 2),
            FormFactor::Watch => (48, 6, 2),
            FormFactor::Phone => (48, 8, 2),
            FormFactor::Tablet => (44, 12, 3),
            FormFactor::Desktop => (32, 12, 4),
        };
        let sc = clamp_scale(scale_percent);
        Self {
            min_target: target * sc / 100,
            gap: gap * sc / 100,
            columns,
        }
    }
}

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
    /// Physical width and height of the glass in millimetres, or 0 where the host does not know.
    pub size_mm: (u32, u32),
    /// The form factor the host declares (a board file, an app), which beats everything measured.
    pub form: Option<FormFactor>,
    /// How the glass makes its picture.
    pub kind: DisplayKind,
    /// Levels the glass shows, as bits per pixel (1 for black and white, 2 for four grays, 4 for sixteen); 0 for "any".
    pub depth_bits: u8,
    /// How long a refresh takes in milliseconds (an e-ink panel 150 to 2000); 0 for "as fast as asked".
    pub refresh_ms: u32,
    /// How it is driven.
    pub inputs: Inputs,
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
            size_mm: (0, 0),
            form: None,
            kind: DisplayKind::Emissive,
            depth_bits: 0,
            refresh_ms: 0,
            inputs: Inputs::NONE,
        }
    }

    /// With the kind of glass, its depth in bits per pixel and its refresh time in milliseconds.
    #[must_use]
    pub const fn with_glass(self, kind: DisplayKind, depth_bits: u8, refresh_ms: u32) -> Self {
        Self {
            kind,
            depth_bits,
            refresh_ms,
            ..self
        }
    }

    /// With how it is driven.
    #[must_use]
    pub const fn with_inputs(self, inputs: Inputs) -> Self {
        Self { inputs, ..self }
    }

    /// Whether the glass reflects light and is slow (e-ink): no animation, a mono or gray theme, one update per refresh.
    #[must_use]
    pub const fn is_reflective(&self) -> bool {
        matches!(self.kind, DisplayKind::Reflective)
    }

    /// The motion the glass can bear: none on a reflective one, full on any other.
    #[must_use]
    pub const fn motion(&self) -> MotionBudget {
        if self.is_reflective() {
            MotionBudget::Off
        } else {
            MotionBudget::Full
        }
    }

    /// The palette this glass shows: `wanted` on an ordinary screen, [`Palette::PAPER`] on a reflective one with few levels
    /// (a colour a panel cannot show is not a colour). A reflective glass of 16 grays or more (`depth_bits` 4 and up)
    /// keeps the palette asked for.
    #[must_use]
    pub const fn palette(&self, wanted: Palette) -> Palette {
        if self.is_reflective() && self.depth_bits < 4 {
            Palette::PAPER
        } else {
            wanted
        }
    }

    /// The size class by window width, which follows a resize.
    #[must_use]
    pub const fn size_class(&self) -> SizeClass {
        SizeClass::of(self.logical_width())
    }

    /// The sizes for a layout, in pixels.
    #[must_use]
    pub const fn metrics(&self) -> Metrics {
        Metrics::of(self.form_factor(), self.scale())
    }

    /// With the physical size of the glass in millimetres (width, height; 0 for unknown). A watch and a phone with the
    /// same logical width are told apart by this.
    #[must_use]
    pub const fn with_size_mm(self, width: u32, height: u32) -> Self {
        Self {
            size_mm: (width, height),
            ..self
        }
    }

    /// With the form factor declared by the host, which is trusted over any measurement.
    #[must_use]
    pub const fn with_form_factor(self, form: FormFactor) -> Self {
        Self {
            form: Some(form),
            ..self
        }
    }

    /// The shorter physical side in millimetres, or 0 if either is unknown (so it does not depend on the orientation).
    #[must_use]
    pub const fn short_side_mm(&self) -> u32 {
        let (w, h) = self.size_mm;
        if w == 0 || h == 0 {
            0
        } else if w < h {
            w
        } else {
            h
        }
    }

    /// Pixels per inch, from the physical width, or `None` where it is not known.
    #[must_use]
    pub const fn dpi(&self) -> Option<u32> {
        let mm = self.size_mm.0;
        if mm == 0 {
            None
        } else {
            Some((self.width as u64 * 254 / (mm as u64 * 10)) as u32)
        }
    }

    /// What kind of device this is: what the host declared, else what the physical size says, else what the logical width
    /// suggests.
    #[must_use]
    pub const fn form_factor(&self) -> FormFactor {
        if let Some(f) = self.form {
            return f;
        }
        if let Some(f) = FormFactor::of_short_side_mm(self.short_side_mm()) {
            return f;
        }
        if self.logical_width() < MICRO_BELOW {
            return FormFactor::Micro;
        }
        match Class::of(self.logical_width()) {
            Class::Watch => FormFactor::Watch,
            Class::Phone => FormFactor::Phone,
            Class::Tablet => FormFactor::Tablet,
            Class::Desktop => FormFactor::Desktop,
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

    /// The class of this screen: the logical width's, except that where the device is known (declared or measured) a watch
    /// is always a watch and nothing else is, so a 360 dp phone is not given the watch's tiles.
    #[must_use]
    pub const fn class(&self) -> Class {
        let by_width = Class::of(self.logical_width());
        let known = match self.form {
            Some(f) => Some(f),
            None => FormFactor::of_short_side_mm(self.short_side_mm()),
        };
        match (known, by_width) {
            (Some(f), _) if f.is_compact() && !matches!(f, FormFactor::Phone) => Class::Watch,
            (Some(_), Class::Watch) => Class::Phone,
            _ => by_width,
        }
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
        let t = base.scaled(self.scale()).with_motion(self.motion());
        // Grays do not show on a reflective glass, and a shadow is only grays.
        if self.is_reflective() {
            Theme {
                elevation: 0,
                decor: t.decor.flat(),
                ..t
            }
        } else {
            t
        }
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
    fn the_physical_size_tells_a_phone_from_a_watch_at_the_same_logical_width() {
        // 1080 px at 3x is 360 logical: by width a watch.
        let phone = Screen::new(1080, 2400).with_scale(300);
        assert_eq!(phone.class(), Class::Watch);
        assert_eq!(phone.form_factor(), FormFactor::Watch);
        // Told it is 68 x 150 mm, it is a phone, and gets the phone class.
        let phone = phone.with_size_mm(68, 150);
        assert_eq!(phone.form_factor(), FormFactor::Phone);
        assert_eq!(phone.class(), Class::Phone);
        // A 1.75 inch watch: 35 x 43 mm.
        let watch = Screen::new(410, 502).with_size_mm(35, 43);
        assert_eq!(watch.form_factor(), FormFactor::Watch);
        assert_eq!(watch.class(), Class::Watch);
        assert_eq!(watch.short_side_mm(), 35);
        // The orientation does not matter.
        assert_eq!(
            Screen::new(2400, 1080)
                .with_size_mm(150, 68)
                .short_side_mm(),
            68
        );
        // A 7 inch panel (about 90 mm short side) is not decided by glass: by width, a tablet.
        let panel = Screen::new(1024, 600).with_size_mm(154, 90);
        assert_eq!(panel.form_factor(), FormFactor::Tablet);
        assert_eq!(panel.class(), Class::Tablet);
        // A big monitor says nothing either way.
        let monitor = Screen::new(2560, 1440).with_size_mm(597, 336);
        assert_eq!(monitor.form_factor(), FormFactor::Desktop);
    }

    #[test]
    fn what_the_host_declares_beats_what_is_measured() {
        let s = Screen::new(2560, 1440)
            .with_size_mm(597, 336)
            .with_form_factor(FormFactor::Watch);
        assert_eq!(s.form_factor(), FormFactor::Watch);
        assert_eq!(s.class(), Class::Watch);
        let p = Screen::new(300, 300).with_form_factor(FormFactor::Phone);
        assert_eq!(p.class(), Class::Phone, "a declared phone is never a watch");
    }

    #[test]
    fn unknown_physical_size_changes_nothing_and_dpi_follows_it() {
        let s = Screen::new(410, 502);
        assert_eq!(s.short_side_mm(), 0);
        assert_eq!(s.dpi(), None);
        assert_eq!(s.form_factor(), FormFactor::Watch);
        assert_eq!(Screen::new(10, 10).with_size_mm(0, 5).short_side_mm(), 0);
        // 410 px over 35 mm is about 297 dpi.
        assert_eq!(Screen::new(410, 502).with_size_mm(35, 43).dpi(), Some(297));
    }

    #[test]
    fn small_boards_are_declared_micro_or_panel_and_are_never_watches() {
        // A 1.28 inch round board has a watch's glass (33 mm) but is not worn: declared, it is a panel.
        let board = Screen::new(240, 240)
            .with_size_mm(33, 33)
            .with_form_factor(FormFactor::Panel);
        assert_eq!(board.form_factor(), FormFactor::Panel);
        assert_eq!(board.class(), Class::Watch, "the small layout");
        // Undeclared, the same glass is a watch by size: that is why boards declare.
        assert_eq!(
            Screen::new(240, 240).with_size_mm(33, 33).form_factor(),
            FormFactor::Watch
        );
        // Nothing known and very few pixels: a micro board.
        for (w, h) in [(128, 64), (240, 135), (200, 200)] {
            assert_eq!(
                Screen::new(w, h).form_factor(),
                FormFactor::Micro,
                "{w}x{h}"
            );
        }
        assert_eq!(Screen::new(410, 502).form_factor(), FormFactor::Watch);
        assert_eq!(FormFactor::from_name("micro"), Some(FormFactor::Micro));
        assert_eq!(FormFactor::from_name("panel"), Some(FormFactor::Panel));
        assert_eq!(FormFactor::from_name("toaster"), None);
        assert!(FormFactor::Micro.is_compact() && !FormFactor::Tablet.is_compact());
    }

    #[test]
    fn a_reflective_glass_has_no_motion_and_the_theme_follows_without_the_app_asking() {
        let eink = Screen::new(296, 128).with_glass(DisplayKind::Reflective, 2, 300);
        assert!(eink.is_reflective());
        assert_eq!(eink.motion(), MotionBudget::Off);
        let t = eink.theme(&Theme::DARK);
        assert_eq!(
            (t.motion.fast_ms, t.motion.normal_ms, t.motion.slow_ms),
            (0, 0, 0)
        );
        assert_eq!((eink.depth_bits, eink.refresh_ms), (2, 300));
        let lcd = Screen::new(800, 600);
        assert_eq!(lcd.motion(), MotionBudget::Full);
        assert_eq!(lcd.theme(&Theme::DARK), Theme::DARK, "nothing changed");
        assert_eq!(
            DisplayKind::from_name("eink"),
            Some(DisplayKind::Reflective)
        );
        assert_eq!(DisplayKind::from_name("lcd"), None);
    }

    #[test]
    fn a_reflective_glass_draws_the_cyberdeck_design_in_plain_lines_and_solid_fills() {
        let eink = Screen::new(296, 128).with_glass(DisplayKind::Reflective, 1, 300);
        let t = eink.theme(&Theme::CYBERDECK);
        assert_eq!((t.decor.hatch, t.decor.dots), (0, 0), "no pattern of greys");
        assert_eq!(t.decor.ticks, Theme::CYBERDECK.decor.ticks, "lines stay");
        assert_eq!(eink.palette(Palette::CYBERDECK), Palette::PAPER);
        let lcd = Screen::new(800, 600);
        assert_eq!(lcd.theme(&Theme::CYBERDECK).decor, Theme::CYBERDECK.decor);
    }

    #[test]
    fn a_black_and_white_glass_gets_the_paper_palette_and_others_keep_theirs() {
        let eink = Screen::new(296, 128).with_glass(DisplayKind::Reflective, 1, 300);
        assert_eq!(eink.palette(Palette::FOREST), Palette::PAPER);
        // A glass with depth unknown (0) is also treated as having few levels.
        let unknown = Screen::new(296, 128).with_glass(DisplayKind::Reflective, 0, 0);
        assert_eq!(unknown.palette(Palette::FOREST), Palette::PAPER);
        let gray16 = Screen::new(800, 480).with_glass(DisplayKind::Reflective, 4, 900);
        assert_eq!(gray16.palette(Palette::FOREST), Palette::FOREST);
        assert_eq!(
            Screen::new(296, 128).palette(Palette::FOREST),
            Palette::FOREST
        );
        assert!(
            Palette::named("paper").is_none(),
            "not a choice in a switcher"
        );
    }

    #[test]
    fn inputs_are_a_set_of_how_the_device_is_driven() {
        let i = Inputs::TOUCH.with(Inputs::KEYS);
        assert!(i.has(Inputs::TOUCH) && i.has(Inputs::KEYS) && !i.has(Inputs::ENCODER));
        assert!(Inputs::NONE.is_none() && !i.is_none());
        assert!(i.has(Inputs::NONE), "everything has nothing");
        assert_eq!(Inputs::from_name("crown"), Some(Inputs::ENCODER));
        assert_eq!(Inputs::from_name("voice"), None);
        let s = Screen::new(240, 240).with_inputs(i);
        assert_eq!(s.inputs, i);
    }

    #[test]
    fn the_size_class_follows_the_window_and_the_metrics_follow_the_device() {
        for (w, c) in [
            (0, SizeClass::Compact),
            (599, SizeClass::Compact),
            (600, SizeClass::Medium),
            (839, SizeClass::Medium),
            (840, SizeClass::Expanded),
            (u32::MAX, SizeClass::Expanded),
        ] {
            assert_eq!(SizeClass::of(w), c, "{w}");
        }
        assert!(SizeClass::Compact < SizeClass::Expanded);
        // A watch stays a watch at any width; its size class is only the window's.
        let watch = Screen::new(410, 502).with_form_factor(FormFactor::Watch);
        assert_eq!(watch.size_class(), SizeClass::Compact);
        let m = watch.metrics();
        assert_eq!((m.min_target, m.columns), (48, 2));
        // At 3x the targets are three times the pixels.
        let phone = Screen::new(1080, 2400)
            .with_scale(300)
            .with_form_factor(FormFactor::Phone);
        assert_eq!(phone.metrics().min_target, 144);
        assert!(
            Metrics::of(FormFactor::Micro, 100).min_target
                < Metrics::of(FormFactor::Phone, 100).min_target
        );
        assert!(
            Metrics::of(FormFactor::Desktop, 100).columns
                > Metrics::of(FormFactor::Watch, 100).columns
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
