//! What the terminal sends, as Fusuma input.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use fusuma::render::Point;
use fusuma::{Edit, Input, InputText, Key, Pointer, wheel_pixels};

/// Rows a wheel notch scrolls: a layout in cells is one row a line.
pub const WHEEL_ROWS: i32 = 3;

/// `event` as a Fusuma input, or `None` for what Fusuma does not use (a key release, a focus change, a paste: not
/// handled yet). `wants_text` is the app's [`fusuma::App::wants_text`]: with a text field focused the edit keys
/// edit, otherwise the arrows move the focus. Mouse positions are in cells.
#[must_use]
pub fn map_event(event: &Event, wants_text: bool) -> Option<Input> {
    match event {
        Event::Key(k) => map_key(k, wants_text),
        Event::Mouse(m) => map_mouse(m),
        Event::Resize(w, h) => Some(Input::Resize {
            width: u32::from(*w),
            height: u32::from(*h),
            scale_percent: 100,
        }),
        _ => None,
    }
}

fn map_key(k: &KeyEvent, wants_text: bool) -> Option<Input> {
    if k.kind == KeyEventKind::Release {
        return None;
    }
    let shift = k.modifiers.contains(KeyModifiers::SHIFT);
    let command = k
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::SUPER);
    let edit = |e: Edit| wants_text.then_some(Input::Edit(e));
    match k.code {
        KeyCode::Char(c) if command => {
            (c.eq_ignore_ascii_case(&'a') && wants_text).then_some(Input::Edit(Edit::SelectAll))
        }
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            Some(Input::Text(InputText::new(c.encode_utf8(&mut buf))))
        }
        KeyCode::Enter => Some(Input::Key(Key::Activate)),
        KeyCode::Tab if shift => Some(Input::Key(Key::Prev)),
        KeyCode::Tab => Some(Input::Key(Key::Next)),
        KeyCode::BackTab => Some(Input::Key(Key::Prev)),
        KeyCode::Esc => Some(Input::Key(Key::Cancel)),
        KeyCode::Up => Some(Input::Key(Key::Up)),
        KeyCode::Down => Some(Input::Key(Key::Down)),
        KeyCode::Left if wants_text => Some(Input::Edit(Edit::Left(shift))),
        KeyCode::Right if wants_text => Some(Input::Edit(Edit::Right(shift))),
        KeyCode::Left => Some(Input::Key(Key::Left)),
        KeyCode::Right => Some(Input::Key(Key::Right)),
        KeyCode::Backspace => edit(Edit::Backspace),
        KeyCode::Delete => edit(Edit::Delete),
        KeyCode::Home => edit(Edit::Home(shift)),
        KeyCode::End => edit(Edit::End(shift)),
        _ => None,
    }
}

fn map_mouse(m: &MouseEvent) -> Option<Input> {
    let at = Point::new(i32::from(m.column), i32::from(m.row));
    let p = match m.kind {
        MouseEventKind::Down(MouseButton::Left) => Pointer::Down(at),
        MouseEventKind::Up(MouseButton::Left) => Pointer::Up(at),
        MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Moved => Pointer::Move(at),
        // Towards the user: the content moves up, which Fusuma counts as positive.
        MouseEventKind::ScrollDown => Pointer::Wheel(at, wheel_pixels(WHEEL_ROWS, 1)),
        MouseEventKind::ScrollUp => Pointer::Wheel(at, wheel_pixels(-WHEEL_ROWS, 1)),
        _ => return None,
    };
    Some(Input::Pointer(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent::new(code, modifiers))
    }

    fn plain(code: KeyCode) -> Event {
        key(code, KeyModifiers::NONE)
    }

    #[test]
    fn keys_move_the_focus_and_press() {
        let k = |code| map_event(&plain(code), false);
        assert_eq!(k(KeyCode::Tab), Some(Input::Key(Key::Next)));
        assert_eq!(k(KeyCode::BackTab), Some(Input::Key(Key::Prev)));
        assert_eq!(
            map_event(&key(KeyCode::Tab, KeyModifiers::SHIFT), false),
            Some(Input::Key(Key::Prev))
        );
        assert_eq!(k(KeyCode::Enter), Some(Input::Key(Key::Activate)));
        assert_eq!(k(KeyCode::Esc), Some(Input::Key(Key::Cancel)));
        assert_eq!(k(KeyCode::Left), Some(Input::Key(Key::Left)));
        assert_eq!(k(KeyCode::Down), Some(Input::Key(Key::Down)));
        assert_eq!(k(KeyCode::Backspace), None, "no field: nothing to edit");
        assert_eq!(k(KeyCode::F(5)), None);
    }

    #[test]
    fn with_a_text_field_the_keys_edit_and_characters_are_text() {
        let k = |e| map_event(&e, true);
        assert_eq!(
            k(plain(KeyCode::Char('ș'))),
            Some(Input::Text(InputText::new("ș")))
        );
        assert_eq!(
            k(key(KeyCode::Left, KeyModifiers::SHIFT)),
            Some(Input::Edit(Edit::Left(true)))
        );
        assert_eq!(
            k(plain(KeyCode::Backspace)),
            Some(Input::Edit(Edit::Backspace))
        );
        assert_eq!(
            k(key(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            Some(Input::Edit(Edit::SelectAll))
        );
        assert_eq!(
            k(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            None,
            "a shortcut is not typed"
        );
        let mut release = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        release.kind = KeyEventKind::Release;
        assert_eq!(k(Event::Key(release)), None);
    }

    #[test]
    fn the_mouse_is_a_pointer_in_cells_and_resizes_are_in_cells() {
        let m = |kind| {
            map_event(
                &Event::Mouse(MouseEvent {
                    kind,
                    column: 7,
                    row: 3,
                    modifiers: KeyModifiers::NONE,
                }),
                false,
            )
        };
        let at = Point::new(7, 3);
        assert_eq!(
            m(MouseEventKind::Down(MouseButton::Left)),
            Some(Input::Pointer(Pointer::Down(at)))
        );
        assert_eq!(
            m(MouseEventKind::Drag(MouseButton::Left)),
            Some(Input::Pointer(Pointer::Move(at)))
        );
        assert_eq!(
            m(MouseEventKind::ScrollDown),
            Some(Input::Pointer(Pointer::Wheel(at, 3)))
        );
        assert_eq!(
            m(MouseEventKind::ScrollUp),
            Some(Input::Pointer(Pointer::Wheel(at, -3)))
        );
        assert_eq!(m(MouseEventKind::Down(MouseButton::Right)), None);
        assert_eq!(
            map_event(&Event::Resize(80, 24), false),
            Some(Input::Resize {
                width: 80,
                height: 24,
                scale_percent: 100
            })
        );
        assert_eq!(map_event(&Event::FocusLost, false), None);
    }
}
