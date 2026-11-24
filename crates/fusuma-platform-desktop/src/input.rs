//! Window-system input as Fusuma input: what winit says, in the terms of `fusuma::Pointer` and `fusuma::Key`.
//! Pure functions, so they are tested without a window.

use fusuma::render::Point;
use fusuma::{Edit, Key, Pointer};
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
        // Space is text: a text field types it, anything else is pressed by it (the toolkit decides).
        WKey::Named(NamedKey::Enter) => Some(Key::Activate),
        _ => None,
    }
}

/// The edit command a key means in a text field, if any. Shift extends the selection; Ctrl (or Cmd on a Mac)
/// with `a` selects all.
#[must_use]
pub fn map_edit(key: &WKey, shift: bool, command: bool) -> Option<Edit> {
    match key {
        WKey::Named(NamedKey::Backspace) => Some(Edit::Backspace),
        WKey::Named(NamedKey::Delete) => Some(Edit::Delete),
        WKey::Named(NamedKey::ArrowLeft) => Some(Edit::Left(shift)),
        WKey::Named(NamedKey::ArrowRight) => Some(Edit::Right(shift)),
        WKey::Named(NamedKey::Home) => Some(Edit::Home(shift)),
        WKey::Named(NamedKey::End) => Some(Edit::End(shift)),
        WKey::Character(c) if command && c.eq_ignore_ascii_case("a") => Some(Edit::SelectAll),
        _ => None,
    }
}

/// A clipboard command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    /// Copy the selection.
    Copy,
    /// Copy the selection, then delete it.
    Cut,
    /// Insert the clipboard's text.
    Paste,
}

/// The clipboard command of Ctrl+C, Ctrl+X and Ctrl+V (Cmd on a Mac), if `key` is one.
#[must_use]
pub fn shortcut(key: &WKey, command: bool) -> Option<Shortcut> {
    let WKey::Character(c) = key else { return None };
    if !command {
        return None;
    }
    match c.to_ascii_lowercase().as_str() {
        "c" => Some(Shortcut::Copy),
        "x" => Some(Shortcut::Cut),
        "v" => Some(Shortcut::Paste),
        _ => None,
    }
}

/// The text a key event types, if it types any: printable text, not a command (Ctrl or Cmd held) and not a
/// control character (Enter, Tab and Backspace come with text like `"\r"`, which the keys already cover).
#[must_use]
pub fn typed_text(text: Option<&str>, command: bool) -> Option<&str> {
    let t = text?;
    (!command && !t.is_empty() && t.chars().all(|c| !c.is_control())).then_some(t)
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
        assert_eq!(
            map_key(&named(NamedKey::Space), false),
            None,
            "space is text"
        );
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

    #[test]
    fn edit_keys_follow_shift_and_select_all_follows_the_command_key() {
        let named = WKey::Named;
        assert_eq!(
            map_edit(&named(NamedKey::Backspace), false, false),
            Some(Edit::Backspace)
        );
        assert_eq!(
            map_edit(&named(NamedKey::Delete), false, false),
            Some(Edit::Delete)
        );
        assert_eq!(
            map_edit(&named(NamedKey::ArrowLeft), true, false),
            Some(Edit::Left(true))
        );
        assert_eq!(
            map_edit(&named(NamedKey::ArrowRight), false, false),
            Some(Edit::Right(false))
        );
        assert_eq!(
            map_edit(&named(NamedKey::Home), true, false),
            Some(Edit::Home(true))
        );
        assert_eq!(
            map_edit(&named(NamedKey::End), false, false),
            Some(Edit::End(false))
        );
        assert_eq!(
            map_edit(&WKey::Character("a".into()), false, true),
            Some(Edit::SelectAll)
        );
        assert_eq!(
            map_edit(&WKey::Character("A".into()), false, true),
            Some(Edit::SelectAll)
        );
        assert_eq!(
            map_edit(&WKey::Character("a".into()), false, false),
            None,
            "a plain a is text"
        );
        assert_eq!(map_edit(&named(NamedKey::Tab), false, false), None);
    }

    #[test]
    fn the_clipboard_shortcuts_need_the_command_key() {
        let ch = |s: &str| WKey::Character(s.into());
        assert_eq!(shortcut(&ch("c"), true), Some(Shortcut::Copy));
        assert_eq!(shortcut(&ch("X"), true), Some(Shortcut::Cut));
        assert_eq!(shortcut(&ch("v"), true), Some(Shortcut::Paste));
        assert_eq!(shortcut(&ch("c"), false), None);
        assert_eq!(shortcut(&ch("z"), true), None);
        assert_eq!(shortcut(&WKey::Named(NamedKey::Enter), true), None);
    }

    #[test]
    fn only_printable_text_without_a_command_is_typed() {
        assert_eq!(typed_text(Some("a"), false), Some("a"));
        assert_eq!(typed_text(Some(" "), false), Some(" "));
        assert_eq!(typed_text(Some("é日"), false), Some("é日"));
        assert_eq!(typed_text(Some("a"), true), None, "Ctrl+A is a command");
        assert_eq!(typed_text(Some("\r"), false), None);
        assert_eq!(typed_text(Some("\u{8}"), false), None);
        assert_eq!(typed_text(Some(""), false), None);
        assert_eq!(typed_text(None, false), None);
    }
}
