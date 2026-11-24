//! The clipboard: a trait so the shortcuts are tested without a display, and the system's clipboard behind it.

use fusuma::{App, Edit, Input, Shell, text_inputs};

use crate::input::Shortcut;

/// Somewhere text can be copied to and pasted from.
pub trait Clipboard {
    /// The text on the clipboard, if there is some.
    fn get(&mut self) -> Option<String>;
    /// Puts `text` on the clipboard.
    fn set(&mut self, text: &str);
}

/// The system clipboard. If it cannot be opened (no display server, a locked-down session) it does nothing,
/// so the shortcuts are harmless instead of an error.
pub struct SystemClipboard(Option<arboard::Clipboard>);

impl core::fmt::Debug for SystemClipboard {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SystemClipboard")
            .field("open", &self.0.is_some())
            .finish()
    }
}

impl SystemClipboard {
    /// Opens the system clipboard.
    pub fn new() -> Self {
        match arboard::Clipboard::new() {
            Ok(c) => Self(Some(c)),
            Err(e) => {
                log::warn!("no clipboard: {e}");
                Self(None)
            }
        }
    }
}

impl Default for SystemClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Clipboard for SystemClipboard {
    fn get(&mut self) -> Option<String> {
        self.0.as_mut()?.get_text().ok().filter(|t| !t.is_empty())
    }

    fn set(&mut self, text: &str) {
        if let Some(c) = self.0.as_mut()
            && let Err(e) = c.set_text(text)
        {
            log::warn!("cannot copy: {e}");
        }
    }
}

/// Does what `shortcut` asks with the app's text field and `clipboard`. Does nothing when no text field has
/// the focus, or nothing is selected (for a copy or a cut).
pub fn run_shortcut<A: App>(
    shortcut: Shortcut,
    shell: &mut Shell<A>,
    clipboard: &mut dyn Clipboard,
) {
    if !shell.app.wants_text() {
        return;
    }
    match shortcut {
        Shortcut::Copy | Shortcut::Cut => {
            if let Some(text) = shell.app.selected_text() {
                clipboard.set(text);
                if shortcut == Shortcut::Cut {
                    shell.input(Input::Edit(Edit::Backspace));
                }
            }
        }
        Shortcut::Paste => {
            if let Some(text) = clipboard.get() {
                // A single line: a pasted newline is a space, not a submit.
                let line = text.replace(['\r', '\n'], " ");
                shell.inputs(text_inputs(&line));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma::Pointer;
    use fusuma::render::Point;
    use fusuma_showcase::{Page, Showcase};

    #[derive(Default)]
    struct Fake(Option<String>);

    impl Clipboard for Fake {
        fn get(&mut self) -> Option<String> {
            self.0.clone()
        }
        fn set(&mut self, text: &str) {
            self.0 = Some(text.to_owned());
        }
    }

    /// The showcase on its Text page with the field focused.
    fn typing() -> Shell<Showcase> {
        let mut shell = Shell::new(Showcase::new(1024, 600));
        let mut r = fusuma::renderers::vello_cpu::new(1024, 600).unwrap();
        shell.frame(0, &mut r).unwrap();
        let tap = |shell: &mut Shell<Showcase>, id| {
            let rc = shell.app.rect_of(id).unwrap();
            let p = Point::new(rc.x + 6, rc.y + 6);
            shell.input(Input::Pointer(Pointer::Down(p)));
            shell.input(Input::Pointer(Pointer::Up(p)));
        };
        let page = shell.app.sidebar_item(Page::Text).unwrap();
        tap(&mut shell, page);
        shell.frame(16, &mut r).unwrap();
        let field = shell.app.widgets().field.unwrap();
        tap(&mut shell, field);
        shell.frame(32, &mut r).unwrap();
        assert!(shell.app.wants_text());
        shell
    }

    fn text(shell: &Shell<Showcase>) -> String {
        let f = shell.app.widgets().field.unwrap();
        shell.app.tree().text(f).unwrap().to_owned()
    }

    #[test]
    fn copy_puts_the_selection_on_the_clipboard_and_leaves_the_text() {
        let mut shell = typing();
        let mut clip = Fake::default();
        run_shortcut(Shortcut::Copy, &mut shell, &mut clip);
        assert_eq!(clip.0, None, "nothing selected, nothing copied");
        shell.input(Input::Edit(Edit::SelectAll));
        run_shortcut(Shortcut::Copy, &mut shell, &mut clip);
        assert_eq!(clip.0.as_deref(), Some("Hello"));
        assert_eq!(text(&shell), "Hello");
    }

    #[test]
    fn cut_copies_then_deletes_and_paste_inserts_at_the_cursor() {
        let mut shell = typing();
        let mut clip = Fake::default();
        shell.input(Input::Edit(Edit::SelectAll));
        run_shortcut(Shortcut::Cut, &mut shell, &mut clip);
        assert_eq!(
            (clip.0.as_deref(), text(&shell).as_str()),
            (Some("Hello"), "")
        );
        run_shortcut(Shortcut::Paste, &mut shell, &mut clip);
        run_shortcut(Shortcut::Paste, &mut shell, &mut clip);
        assert_eq!(text(&shell), "HelloHello");
    }

    #[test]
    fn a_pasted_newline_is_a_space_and_a_long_paste_is_cut_to_the_capacity() {
        let mut shell = typing();
        shell.input(Input::Edit(Edit::SelectAll));
        let mut clip = Fake(Some("one\r\ntwo".into()));
        run_shortcut(Shortcut::Paste, &mut shell, &mut clip);
        assert_eq!(text(&shell), "one  two");
        shell.input(Input::Edit(Edit::SelectAll));
        clip.0 = Some("x".repeat(500));
        run_shortcut(Shortcut::Paste, &mut shell, &mut clip);
        assert_eq!(text(&shell).len(), fusuma::INPUT_CAP);
    }

    #[test]
    fn an_empty_clipboard_and_a_field_without_the_focus_do_nothing() {
        let mut shell = typing();
        let mut clip = Fake::default();
        run_shortcut(Shortcut::Paste, &mut shell, &mut clip);
        assert_eq!(text(&shell), "Hello");
        shell.input(Input::Key(fusuma::Key::Prev));
        assert!(!shell.app.wants_text());
        clip.0 = Some("zzz".into());
        run_shortcut(Shortcut::Paste, &mut shell, &mut clip);
        run_shortcut(Shortcut::Cut, &mut shell, &mut clip);
        assert_eq!(
            clip.0.as_deref(),
            Some("zzz"),
            "a cut with no field copies nothing"
        );
    }
}
