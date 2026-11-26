#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

use fusuma_platform_mobile::{HostInfo, Status};
use fusuma_render::Point;
use fusuma_ui::{Edit, Key, Pointer};

pub use fusuma_platform_mobile as mobile;

/// What this host is, and how far it has got.
pub const HOST: HostInfo = HostInfo {
    name: "android",
    targets: &[
        "aarch64-linux-android",
        "armv7-linux-androideabi",
        "x86_64-linux-android",
    ],
    surface: "ANativeWindow through winit (android-activity); Vulkan by wgpu, Vello CPU as the fallback",
    text_input: "InputMethodManager and the IME's commit and composing text",
    status: Status::Scaffold,
};

/// `MotionEvent.ACTION_DOWN`.
pub const ACTION_DOWN: u8 = 0;
/// `MotionEvent.ACTION_UP`.
pub const ACTION_UP: u8 = 1;
/// `MotionEvent.ACTION_MOVE`.
pub const ACTION_MOVE: u8 = 2;
/// `MotionEvent.ACTION_CANCEL`.
pub const ACTION_CANCEL: u8 = 3;
/// `MotionEvent.ACTION_HOVER_MOVE` (a mouse or a stylus above the screen).
pub const ACTION_HOVER_MOVE: u8 = 7;

fn at(x: f32, y: f32) -> Point {
    // The system reports pixels as floats; the toolkit works in whole pixels.
    Point::new(
        fusuma_platform_mobile::pixel(x),
        fusuma_platform_mobile::pixel(y),
    )
}

/// Millimetres for `pixels` at `dpi` pixels per inch (`DisplayMetrics.xdpi` and `ydpi`), or 0 when the system reports none.
#[must_use]
pub fn millimetres(pixels: u32, dpi: f32) -> u32 {
    if !dpi.is_finite() || dpi < 1.0 {
        return 0;
    }
    let mm = f64::from(pixels) * 25.4 / f64::from(dpi);
    if mm >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        (mm + 0.5) as u32
    }
}

/// The display for `DisplayMetrics` (`widthPixels`, `heightPixels`, `density`, `xdpi`, `ydpi`): the scale is the density
/// (1.0 is 160 dpi), the size in millimetres comes from the real dpi, so a phone and a watch with the same logical width
/// are told apart. Insets and the keyboard are the host's to add.
#[must_use]
pub fn display(
    width: u32,
    height: u32,
    density: f32,
    xdpi: f32,
    ydpi: f32,
) -> fusuma_platform_mobile::Display {
    let percent = if density.is_finite() && density > 0.0 {
        (density * 100.0 + 0.5).min(100_000.0) as u32
    } else {
        100
    };
    fusuma_platform_mobile::Display {
        width,
        height,
        scale_percent: percent,
        width_mm: millimetres(width, xdpi),
        height_mm: millimetres(height, ydpi),
        ..fusuma_platform_mobile::Display::default()
    }
}

/// The pointer event of the **first finger** for a `MotionEvent` action (the masked action, `getActionMasked`) at `(x, y)`.
/// A second finger's down and up (`ACTION_POINTER_DOWN` and `_UP`, 5 and 6) are not this finger's: they give `None`.
#[must_use]
pub fn pointer(action: u8, x: f32, y: f32) -> Option<Pointer> {
    let p = at(x, y);
    match action {
        ACTION_DOWN => Some(Pointer::Down(p)),
        ACTION_MOVE | ACTION_HOVER_MOVE => Some(Pointer::Move(p)),
        ACTION_UP => Some(Pointer::Up(p)),
        // A cancelled touch (the system took it over) ends without a click.
        ACTION_CANCEL => Some(Pointer::Leave),
        _ => None,
    }
}

/// The key for an Android `KeyEvent` code that moves the focus or presses, or `None`. The system **back** button
/// (`KEYCODE_BACK`, 4) is cancel.
#[must_use]
pub fn key(code: i32) -> Option<Key> {
    Some(match code {
        4 | 111 => fusuma_platform_mobile::back(), // KEYCODE_BACK, KEYCODE_ESCAPE
        61 => Key::Next,                           // KEYCODE_TAB
        66 | 160 | 23 => Key::Activate,            // ENTER, NUMPAD_ENTER, DPAD_CENTER
        19 => Key::Up,
        20 => Key::Down,
        21 => Key::Left,
        22 => Key::Right,
        _ => return None,
    })
}

/// The edit for an Android `KeyEvent` code while a text field has the focus (`shift`: the Shift key is held), or `None`.
#[must_use]
pub fn edit(code: i32, shift: bool) -> Option<Edit> {
    Some(match code {
        67 => Edit::Backspace,    // KEYCODE_DEL
        112 => Edit::Delete,      // KEYCODE_FORWARD_DEL
        122 => Edit::Home(shift), // KEYCODE_MOVE_HOME
        123 => Edit::End(shift),  // KEYCODE_MOVE_END
        21 => Edit::Left(shift),
        22 => Edit::Right(shift),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_finger_becomes_the_pointer_and_a_cancel_ends_without_a_click() {
        assert_eq!(
            pointer(ACTION_DOWN, 10.4, 20.6),
            Some(Pointer::Down(Point::new(10, 21)))
        );
        assert_eq!(
            pointer(ACTION_MOVE, 11.0, 20.0),
            Some(Pointer::Move(Point::new(11, 20)))
        );
        assert_eq!(
            pointer(ACTION_UP, 11.0, 20.0),
            Some(Pointer::Up(Point::new(11, 20)))
        );
        assert_eq!(pointer(ACTION_CANCEL, 0.0, 0.0), Some(Pointer::Leave));
        assert_eq!(
            pointer(ACTION_HOVER_MOVE, 5.0, 5.0),
            Some(Pointer::Move(Point::new(5, 5)))
        );
        // A second finger (POINTER_DOWN 5, POINTER_UP 6) and unknown actions are not the pointer's.
        for a in [5u8, 6, 4, 8, 255] {
            assert_eq!(pointer(a, 1.0, 1.0), None, "action {a}");
        }
    }

    #[test]
    fn odd_coordinates_never_panic_or_overflow() {
        for v in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            -0.0,
        ] {
            let Some(Pointer::Down(p)) = pointer(ACTION_DOWN, v, v) else {
                panic!()
            };
            assert!(
                p.x.abs() <= 1_000_000_000 && p.y.abs() <= 1_000_000_000,
                "{v}: {p:?}"
            );
        }
        assert_eq!(
            pointer(ACTION_DOWN, f32::NAN, 3.0),
            Some(Pointer::Down(Point::new(0, 3)))
        );
    }

    #[test]
    fn back_is_cancel_and_the_keyboard_keys_move_and_press() {
        assert_eq!(key(4), Some(Key::Cancel), "the system back button");
        assert_eq!(key(111), Some(Key::Cancel));
        assert_eq!(key(61), Some(Key::Next));
        assert_eq!(key(66), Some(Key::Activate));
        assert_eq!(key(23), Some(Key::Activate));
        assert_eq!(
            [key(19), key(20), key(21), key(22)],
            [
                Some(Key::Up),
                Some(Key::Down),
                Some(Key::Left),
                Some(Key::Right)
            ]
        );
        assert_eq!(key(29), None, "KEYCODE_A is text, not a key");
        assert_eq!(key(-1), None);
    }

    #[test]
    fn editing_keys_follow_shift() {
        assert_eq!(edit(67, false), Some(Edit::Backspace));
        assert_eq!(edit(112, true), Some(Edit::Delete));
        assert_eq!(edit(21, true), Some(Edit::Left(true)));
        assert_eq!(edit(122, false), Some(Edit::Home(false)));
        assert_eq!(edit(123, true), Some(Edit::End(true)));
        assert_eq!(edit(66, false), None);
    }

    #[test]
    fn metrics_become_a_display_that_knows_its_glass() {
        // A 1080x2400 phone at 420 dpi (density 2.625): 65 x 145 mm, a phone, not a watch.
        let d = display(1080, 2400, 2.625, 420.0, 420.0);
        assert_eq!((d.scale_percent, d.width_mm, d.height_mm), (263, 65, 145));
        assert_eq!(d.screen().form_factor(), fusuma_ui::FormFactor::Phone);
        assert_eq!(d.screen().class(), fusuma_ui::Class::Phone);
        // A 1.4 inch round watch, 454 px at 326 dpi: 35 mm, a watch.
        let w = display(454, 454, 2.0, 326.0, 326.0);
        assert_eq!(w.screen().form_factor(), fusuma_ui::FormFactor::Watch);
        // A system that reports nothing useful leaves the size unknown and the scale at 1x.
        for bad in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let d = display(100, 100, bad, bad, bad);
            assert_eq!(
                (d.scale_percent, d.width_mm, d.height_mm),
                (100, 0, 0),
                "{bad}"
            );
        }
        assert_eq!(millimetres(u32::MAX, 1.0), u32::MAX);
    }

    #[test]
    fn the_host_says_what_it_is() {
        assert_eq!(HOST.name, "android");
        assert!(HOST.targets.iter().all(|t| t.contains("android")));
        assert_eq!(HOST.status, Status::Scaffold, "no activity runs an app yet");
    }
}
