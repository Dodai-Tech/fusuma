//! Fusuma on iOS and iPadOS: touch phases and hardware keys, translated to Fusuma input over the shared lifecycle (feature `ios`; a scaffold, no activity, view or window yet).

use crate::{HostInfo, Status};
use fusuma_render::Point;
use fusuma_ui::{Edit, Key, Pointer};

/// What this host is, and how far it has got.
pub const HOST: HostInfo = HostInfo {
    name: "ios",
    targets: &[
        "aarch64-apple-ios",
        "aarch64-apple-ios-sim",
        "x86_64-apple-ios",
    ],
    surface: "a UIView with a CAMetalLayer through winit; Metal by wgpu, Vello CPU as the fallback",
    text_input: "UIKeyInput and UITextInput, marked text as the preedit",
    status: Status::Scaffold,
};

/// `UITouch.Phase.began`.
pub const PHASE_BEGAN: u8 = 0;
/// `UITouch.Phase.moved`.
pub const PHASE_MOVED: u8 = 1;
/// `UITouch.Phase.stationary`.
pub const PHASE_STATIONARY: u8 = 2;
/// `UITouch.Phase.ended`.
pub const PHASE_ENDED: u8 = 3;
/// `UITouch.Phase.cancelled`.
pub const PHASE_CANCELLED: u8 = 4;

/// The display for a `UIScreen`: its `bounds` in points and its `scale` (1, 2 or 3), so the pixels are points times the scale.
/// UIKit does not report the glass's size: the host looks it up from the device model (an iPhone is 65 to 78 mm wide, an
/// Apple Watch 30 to 40) and gives 0 when it does not know.
#[must_use]
pub fn display(
    points_width: f32,
    points_height: f32,
    scale: f32,
    width_mm: u32,
    height_mm: u32,
) -> crate::Display {
    let scale = if scale.is_finite() && scale >= 1.0 {
        scale
    } else {
        1.0
    };
    let px = |points: f32| u32::try_from(crate::pixel(points * scale).max(0)).unwrap_or(0);
    crate::Display {
        width: px(points_width),
        height: px(points_height),
        scale_percent: (scale * 100.0 + 0.5).min(100_000.0) as u32,
        width_mm,
        height_mm,
        ..crate::Display::default()
    }
}

/// The pointer event for a `UITouch` of the first finger, in **points** times the screen scale (that is, pixels).
/// A finger that has not moved (`stationary`) gives nothing.
#[must_use]
pub fn pointer(phase: u8, x: f32, y: f32) -> Option<Pointer> {
    let p = Point::new(crate::pixel(x), crate::pixel(y));
    match phase {
        PHASE_BEGAN => Some(Pointer::Down(p)),
        PHASE_MOVED => Some(Pointer::Move(p)),
        PHASE_ENDED => Some(Pointer::Up(p)),
        PHASE_CANCELLED => Some(Pointer::Leave),
        _ => None,
    }
}

/// The key for a `UIKeyCommand` input string (the `UIKeyInput...` constants, and `"\t"` and `"\r"` from a hardware keyboard).
#[must_use]
pub fn key(input: &str) -> Option<Key> {
    Some(match input {
        "UIKeyInputEscape" => crate::back(),
        "\t" => Key::Next,
        "\r" | "\n" => Key::Activate,
        "UIKeyInputUpArrow" => Key::Up,
        "UIKeyInputDownArrow" => Key::Down,
        "UIKeyInputLeftArrow" => Key::Left,
        "UIKeyInputRightArrow" => Key::Right,
        _ => return None,
    })
}

/// The edit for a `UIKeyCommand` input string while a text field has the focus (`shift`: Shift is held).
#[must_use]
pub fn edit(input: &str, shift: bool) -> Option<Edit> {
    Some(match input {
        "\u{8}" | "\u{7f}" => Edit::Backspace,
        "UIKeyInputDelete" => Edit::Delete,
        "UIKeyInputHome" => Edit::Home(shift),
        "UIKeyInputEnd" => Edit::End(shift),
        "UIKeyInputLeftArrow" => Edit::Left(shift),
        "UIKeyInputRightArrow" => Edit::Right(shift),
        _ => return None,
    })
}

/// Whether a touch that began at `x` (pixels) on a screen `width` pixels wide starts the system's back swipe: within the
/// leftmost `edge` pixels. A host that sees one sends [`crate::back`] and does not send the touch on.
#[must_use]
pub const fn is_edge_swipe(x: f32, edge: u32) -> bool {
    x >= 0.0 && (x as u32) < edge
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_phases_become_pointer_events_and_a_stationary_finger_is_nothing() {
        assert_eq!(
            pointer(PHASE_BEGAN, 3.0, 4.0),
            Some(Pointer::Down(Point::new(3, 4)))
        );
        assert_eq!(
            pointer(PHASE_MOVED, 3.5, 4.5),
            Some(Pointer::Move(Point::new(4, 5)))
        );
        assert_eq!(
            pointer(PHASE_ENDED, 3.0, 4.0),
            Some(Pointer::Up(Point::new(3, 4)))
        );
        assert_eq!(pointer(PHASE_CANCELLED, 0.0, 0.0), Some(Pointer::Leave));
        assert_eq!(pointer(PHASE_STATIONARY, 1.0, 1.0), None);
        assert_eq!(pointer(99, 1.0, 1.0), None);
        assert_eq!(
            pointer(PHASE_BEGAN, f32::NAN, f32::INFINITY),
            Some(Pointer::Down(Point::new(0, 0)))
        );
    }

    #[test]
    fn hardware_keys_map_and_escape_is_back() {
        assert_eq!(key("UIKeyInputEscape"), Some(Key::Cancel));
        assert_eq!(key("\t"), Some(Key::Next));
        assert_eq!(key("\r"), Some(Key::Activate));
        assert_eq!(key("UIKeyInputUpArrow"), Some(Key::Up));
        assert_eq!(key("UIKeyInputRightArrow"), Some(Key::Right));
        assert_eq!(key("a"), None);
        assert_eq!(key(""), None);
    }

    #[test]
    fn editing_keys_follow_shift() {
        assert_eq!(edit("\u{7f}", false), Some(Edit::Backspace));
        assert_eq!(edit("UIKeyInputDelete", false), Some(Edit::Delete));
        assert_eq!(edit("UIKeyInputLeftArrow", true), Some(Edit::Left(true)));
        assert_eq!(edit("UIKeyInputHome", true), Some(Edit::Home(true)));
        assert_eq!(edit("x", false), None);
    }

    #[test]
    fn a_touch_at_the_left_edge_is_the_back_swipe() {
        assert!(is_edge_swipe(0.0, 24));
        assert!(is_edge_swipe(23.9, 24));
        assert!(!is_edge_swipe(24.0, 24));
        assert!(!is_edge_swipe(-5.0, 24));
        assert!(!is_edge_swipe(f32::NAN, 24));
    }

    #[test]
    fn a_screen_in_points_becomes_pixels_with_its_glass() {
        let d = display(393.0, 852.0, 3.0, 71, 154);
        assert_eq!((d.width, d.height, d.scale_percent), (1179, 2556, 300));
        assert_eq!(d.screen().form_factor(), fusuma_ui::FormFactor::Phone);
        assert_eq!(d.screen().class(), fusuma_ui::Class::Phone);
        // Nonsense from the system is a 1x screen of nothing, not a panic.
        let d = display(f32::NAN, -5.0, f32::NAN, 0, 0);
        assert_eq!((d.width, d.height, d.scale_percent), (0, 0, 100));
    }

    #[test]
    fn the_host_says_what_it_is() {
        assert_eq!(HOST.name, "ios");
        assert!(HOST.targets.iter().all(|t| t.contains("ios")));
        assert_eq!(HOST.status, Status::Scaffold);
    }
}
