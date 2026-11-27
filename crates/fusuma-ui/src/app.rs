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

    /// Milliseconds until the app next needs time to have passed even though nothing is moving (a message that hides, a
    /// press that becomes a long press), or `None` for no such thing. A platform sleeps until then instead of polling;
    /// the default says there is nothing.
    fn deadline(&self) -> Option<u32> {
        None
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
    /// to the room [`Shape::safe_insets`](crate::Shape::safe_insets) leaves. A board sends it once at start; a rectangle never does, so the default
    /// does nothing.
    fn shape(&mut self, shape: crate::Shape) {
        let _ = shape;
    }

    /// The device is `screen`: its form factor, the glass's size, whether it reflects light and how it is driven. A
    /// platform that knows sends it once at start, before the first frame; an app that lays out by the class alone can
    /// ignore it, which the default does.
    fn screen(&mut self, screen: crate::Screen) {
        let _ = screen;
    }

    /// The settings ask for `palette` (`[theme] palette` and `accent` in `fusuma.toml`, the accent already put on
    /// it). A platform sends it once at start, before the first frame; an app that has its own colours can ignore
    /// it, which the default does.
    fn palette(&mut self, palette: crate::Palette) {
        let _ = palette;
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

    /// The separate rectangles the last [`frame`](Self::frame) drew, inside the one it returned. A platform that
    /// pays for every pixel it sends (a panel on a bus) shows these one by one instead of the whole rectangle,
    /// which spares what lies between them. Empty (the default) means: show the rectangle `frame` returned.
    fn frame_rects(&self) -> &[Rect] {
        &[]
    }

    /// What really changed in the last [`frame`](Self::frame), when that is less than the rectangle it returned: a renderer
    /// that draws everything each frame (the GPU) returns the whole screen, and a host reads back and presents only this.
    /// `None` (the default) means the rectangle `frame` returned.
    fn changed(&self) -> Option<Rect> {
        None
    }

    /// Says that the host fills the surfaces of the layer plan and calls [`paint_over`](Self::paint_over) and
    /// [`paint_overlays`](Self::paint_overlays) after each frame (see [`Tree::set_host_surfaces`](crate::Tree::set_host_surfaces)). The default does nothing.
    fn host_surfaces(&mut self, on: bool) {
        let _ = on;
    }

    /// Draws what is inside surface `id` over what the host put there, inside `region`; call it after the surface was
    /// filled. The default draws nothing.
    ///
    /// # Errors
    /// The renderer's error.
    fn paint_over<R: Renderer>(
        &mut self,
        renderer: &mut R,
        id: u32,
        region: Rect,
    ) -> Result<(), R::Error> {
        let _ = (renderer, id, region);
        Ok(())
    }

    /// Draws again what is above everything (dialogs, menus, toasts) inside `region`, a place the host filled; call it
    /// last, once for each such place. The default draws nothing.
    ///
    /// # Errors
    /// The renderer's error.
    fn paint_overlays<R: Renderer>(
        &mut self,
        renderer: &mut R,
        region: Rect,
    ) -> Result<(), R::Error> {
        let _ = (renderer, region);
        Ok(())
    }

    /// The layers of the picture (see [`LayerPlan`](crate::LayerPlan)): the interface and the places the host fills itself
    /// (a video, a camera, a native view). `None` (the default) means the interface alone.
    fn layers(&self) -> Option<crate::LayerPlan> {
        None
    }

    /// Calls `visit` for every widget on the screen, in reading order (see [`Tree::semantics`](crate::Tree::semantics)): what a
    /// bridge to assistive technology describes. The default has none.
    fn semantics(&self, visit: &mut dyn FnMut(&crate::Semantic<'_>)) {
        let _ = visit;
    }

    /// Does what an assistive technology asks of a widget (see [`Tree::assist`](crate::Tree::assist)). The default does nothing.
    fn assist(&mut self, request: crate::Assist<'_>) -> crate::Assisted {
        let _ = request;
        crate::Assisted::Ignored
    }
}
