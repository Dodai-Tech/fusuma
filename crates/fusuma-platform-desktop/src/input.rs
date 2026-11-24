//! Window-system input as Fusuma input: what winit says, in the terms of `fusuma::Pointer` and `fusuma::Key`.
//! Pure functions, so they are tested without a window.

use fusuma::render::Point;
use fusuma::{Key, Pointer};
use winit::dpi::PhysicalPosition;
use winit::event::{MouseScrollDelta, TouchPhase};
use winit::keyboard::{Key as WKey, NamedKey};

/// Pixels a wheel line scrolls. A guess at a comfortable step (about two lines of body text); a platform with
/// a better number can change it.
pub const WHEEL_LINE_PX: i32 = 40;

/// A window position in physical pixels as a point: rounded, and held within the coordinate range (a
/// pointer dragged far outside the window reports huge numbers).
#[must_use]
pub fn point(p: PhysicalPosition<f64>) -> Point {
    let to = |v: f64| {
        if v.is_nan() {
            0
        } else {
            v.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
        }
    };
    Point::new(to(p.x), to(p.y))
}

/// The key a window key event means to the toolkit, if any. Tab moves the focus (Shift+Tab back), the arrows
/// move it or a slider, Enter and Space press.
#[must_use]
pub fn map_key(key: &WKey, shift: bool) -> Option<Key> {
    match key {
        WKey::Named(NamedKey::Tab) => Some(if shift { Key::Prev } else { Key::Next }),
        WKey::Named(NamedKey::ArrowDown) => Some(Key::Down),
        WKey::Named(NamedKey::ArrowUp) => Some(Key::Up),
        WKey::Named(NamedKey::ArrowLeft) => Some(Key::Left),
        WKey::Named(NamedKey::ArrowRight) => Some(Key::Right),
        WKey::Named(NamedKey::Enter | NamedKey::Space) => Some(Key::Activate),
        _ => None,
    }
}

/// A wheel turn as the pixels [`Pointer::Wheel`] takes. winit counts a turn away from the user (content moves
/// down) as positive; Fusuma counts content moving up as positive, so the sign flips.
#[must_use]
pub fn wheel(delta: MouseScrollDelta) -> i32 {
    match delta {
        MouseScrollDelta::LineDelta(_, y) => fusuma::wheel_pixels(round(-y), WHEEL_LINE_PX),
        MouseScrollDelta::PixelDelta(p) => round(-p.y as f32),
    }
}

fn round(v: f32) -> i32 {
    if v.is_nan() {
        0
    } else {
        v.round().clamp(i32::MIN as f32, i32::MAX as f32) as i32
    }
}

/// The pointer events a touch phase makes: a lifted finger is an Up followed by Leave, because it hovers over
/// nothing afterwards.
#[must_use]
pub fn touch(phase: TouchPhase, at: Point) -> [Option<Pointer>; 2] {
    match phase {
        TouchPhase::Started => [Some(Pointer::Down(at)), None],
        TouchPhase::Moved => [Some(Pointer::Move(at)), None],
        TouchPhase::Ended | TouchPhase::Cancelled => [Some(Pointer::Up(at)), Some(Pointer::Leave)],
    }
}

/// A scale factor (1.0, 1.5, 2.0) as the percent `Input::Resize` carries; 100 for nonsense.
#[must_use]
pub fn scale_percent(factor: f64) -> u32 {
    if factor.is_finite() && factor > 0.0 {
        (factor * 100.0).round().clamp(1.0, 100_000.0) as u32
    } else {
        100
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_clamp_and_survive_nan() {
        assert_eq!(point(PhysicalPosition::new(10.4, 20.6)), Point::new(10, 21));
        assert_eq!(point(PhysicalPosition::new(-3.0, 0.0)), Point::new(-3, 0));
        assert_eq!(
            point(PhysicalPosition::new(1e30, -1e30)),
            Point::new(i32::MAX, i32::MIN)
        );
        assert_eq!(
            point(PhysicalPosition::new(f64::NAN, f64::INFINITY)),
            Point::new(0, i32::MAX)
        );
    }

    #[test]
    fn the_toolkit_keys_are_mapped_and_the_rest_are_not() {
        let named = |k| WKey::Named(k);
        assert_eq!(map_key(&named(NamedKey::Tab), false), Some(Key::Next));
        assert_eq!(map_key(&named(NamedKey::Tab), true), Some(Key::Prev));
        assert_eq!(map_key(&named(NamedKey::ArrowDown), false), Some(Key::Down));
        assert_eq!(map_key(&named(NamedKey::ArrowUp), false), Some(Key::Up));
        assert_eq!(map_key(&named(NamedKey::ArrowLeft), false), Some(Key::Left));
        assert_eq!(
            map_key(&named(NamedKey::ArrowRight), false),
            Some(Key::Right)
        );
        assert_eq!(map_key(&named(NamedKey::Enter), false), Some(Key::Activate));
        assert_eq!(map_key(&named(NamedKey::Space), false), Some(Key::Activate));
        assert_eq!(map_key(&named(NamedKey::Escape), false), None);
        assert_eq!(map_key(&WKey::Character("a".into()), false), None);
    }

    #[test]
    fn the_wheel_flips_its_sign_and_converts_lines_to_pixels() {
        assert_eq!(
            wheel(MouseScrollDelta::LineDelta(0.0, -1.0)),
            WHEEL_LINE_PX,
            "away from the user scrolls on"
        );
        assert_eq!(
            wheel(MouseScrollDelta::LineDelta(0.0, 2.0)),
            -2 * WHEEL_LINE_PX
        );
        assert_eq!(
            wheel(MouseScrollDelta::PixelDelta(PhysicalPosition::new(
                0.0, -12.4
            ))),
            12
        );
        assert_eq!(
            wheel(MouseScrollDelta::LineDelta(5.0, 0.0)),
            0,
            "sideways is not handled yet"
        );
        assert_eq!(wheel(MouseScrollDelta::LineDelta(0.0, f32::NAN)), 0);
        assert_eq!(wheel(MouseScrollDelta::LineDelta(0.0, f32::MAX)), i32::MIN);
    }

    #[test]
    fn a_finger_is_down_move_then_up_and_leave_and_a_cancel_is_a_lift() {
        let p = Point::new(3, 4);
        assert_eq!(
            touch(TouchPhase::Started, p),
            [Some(Pointer::Down(p)), None]
        );
        assert_eq!(touch(TouchPhase::Moved, p), [Some(Pointer::Move(p)), None]);
        assert_eq!(
            touch(TouchPhase::Ended, p),
            [Some(Pointer::Up(p)), Some(Pointer::Leave)]
        );
        assert_eq!(touch(TouchPhase::Cancelled, p), touch(TouchPhase::Ended, p));
    }

    #[test]
    fn scale_factors_become_percent() {
        assert_eq!(scale_percent(1.0), 100);
        assert_eq!(scale_percent(1.5), 150);
        assert_eq!(scale_percent(2.0), 200);
        assert_eq!(scale_percent(0.0), 100);
        assert_eq!(scale_percent(f64::NAN), 100);
        assert_eq!(scale_percent(-2.0), 100);
    }
}
