//! What a platform needs from an application, so one app runs on a board, a desktop and a phone.

use fusuma_render::{Rect, Renderer};

use crate::{Edit, Key, Pointer};

/// An application, as a platform sees it. The platform owns the screen, the input devices and the clock; it
/// feeds events in, asks for frames, and picks the renderer. The app never names a renderer.
pub trait App {
    /// A pointer event (a finger or a mouse), in screen pixels.
    fn pointer(&mut self, event: Pointer);

    /// A key.
    fn key(&mut self, key: Key);

    /// Text arrives: typed, pasted, or committed by an input method. A platform sends it for printable keys
    /// (and Space), instead of mapping them to [`Key`]s. The default ignores it.
    fn text(&mut self, text: &str) {
        let _ = text;
    }

    /// An edit command (Backspace, a cursor move, select all) for a focused text field.
    fn edit(&mut self, edit: Edit) {
        let _ = edit;
    }

    /// An input method is composing `text`, not yet committed; empty ends the composition.
    fn preedit(&mut self, text: &str) {
        let _ = text;
    }

    /// Whether typing means something now (a text field has the focus). A platform turns an input method on
    /// and off with it.
    fn wants_text(&self) -> bool {
        false
    }

    /// Where the next character will appear, so the platform can place an input method's candidate window.
    fn text_cursor(&self) -> Option<Rect> {
        None
    }

    /// The selected text, for a clipboard copy or cut (a cut is a copy then [`Edit::Backspace`]).
    fn selected_text(&self) -> Option<&str> {
        None
    }

    /// The shape of the screen is `shape` (rounded corners, a circle): content in a corner is cut off, so a layout keeps
    /// to the room [`Shape::safe_insets`] leaves. A board sends it once at start; a rectangle never does, so the default
    /// does nothing.
    fn shape(&mut self, shape: crate::Shape) {
        let _ = shape;
    }

    /// The drawing area changed to `width` x `height` pixels, or the display's scale did (`scale_percent`: 100
    /// is one pixel per logical pixel). A window being resized or a phone being rotated calls this; a fixed
    /// screen never does, so the default does nothing.
    fn resized(&mut self, width: u32, height: u32, scale_percent: u32) {
        let _ = (width, height, scale_percent);
    }

    /// Time passes by `dt_ms`. Returns whether the app wants a frame.
    fn tick(&mut self, dt_ms: u32) -> bool;

    /// Lays out and draws what changed onto `renderer`, whose surface is the whole screen. Returns the
    /// rectangle that was redrawn, or `None` when nothing changed: the platform only has to show that part.
    ///
    /// # Errors
    /// The renderer's error.
    fn frame<R: Renderer>(&mut self, renderer: &mut R) -> Result<Option<Rect>, R::Error>;
}
