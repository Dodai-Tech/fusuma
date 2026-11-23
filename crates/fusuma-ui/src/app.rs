//! What a platform needs from an application, so one app runs on a board, a desktop and a phone.

use fusuma_render::{Rect, Renderer};

use crate::{Key, Pointer};

/// An application, as a platform sees it. The platform owns the screen, the input devices and the clock; it
/// feeds events in, asks for frames, and picks the renderer. The app never names a renderer.
pub trait App {
    /// A pointer event (a finger or a mouse), in screen pixels.
    fn pointer(&mut self, event: Pointer);

    /// A key.
    fn key(&mut self, key: Key);

    /// Time passes by `dt_ms`. Returns whether the app wants a frame.
    fn tick(&mut self, dt_ms: u32) -> bool;

    /// Lays out and draws what changed onto `renderer`, whose surface is the whole screen. Returns the
    /// rectangle that was redrawn, or `None` when nothing changed: the platform only has to show that part.
    ///
    /// # Errors
    /// The renderer's error.
    fn frame<R: Renderer>(&mut self, renderer: &mut R) -> Result<Option<Rect>, R::Error>;
}
