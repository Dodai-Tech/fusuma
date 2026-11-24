//! The [`Renderer`] trait: what a backend must draw.

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::image::Image;
use crate::path::PathCmd;
use crate::text::TextStyle;

/// The smallest depth of the `save` stack every backend supports.
pub const MIN_STATE_DEPTH: usize = 4;

/// What a backend can do beyond the common ground; widgets can adapt (for example skip a shadow).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Colors with alpha below 255 are blended with what is already drawn.
    pub alpha: bool,
    /// Edges of curves and diagonals are anti-aliased.
    pub antialiasing: bool,
}

/// Why a drawing call failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error<E> {
    /// The target (display, buffer) reported an error.
    Target(E),
    /// `save` was called more often than the backend's state stack allows.
    StackFull,
    /// `restore` was called without a matching `save`.
    StackEmpty,
    /// A path had more points than the backend can handle without allocating.
    TooComplex,
}

/// A 2D renderer.
///
/// All coordinates are whole pixels in the *current* space: the origin is moved by
/// [`translate`](Self::translate) and drawing is limited to the current clip. `save` pushes the
/// clip and the translation, `restore` brings them back; every backend supports at least
/// [`MIN_STATE_DEPTH`] nested saves.
///
/// Backends differ in quality, not in what they draw: lines, rectangles, circles, paths, text and
/// images land on the same pixels give or take anti-aliasing. The `conformance` feature has the
/// test-suite that holds them to that.
pub trait Renderer {
    /// The error a drawing call can return.
    type Error: core::fmt::Debug;

    /// Size of the drawing surface in pixels.
    fn size(&self) -> Size;

    /// What this backend can do beyond the basics.
    fn capabilities(&self) -> Capabilities;

    /// Remembers the clip and translation.
    ///
    /// # Errors
    /// [`Error::StackFull`] when the stack is full.
    fn save(&mut self) -> Result<(), Self::Error>;

    /// Restores the clip and translation of the matching [`save`](Self::save).
    ///
    /// # Errors
    /// [`Error::StackEmpty`] without a matching `save`.
    fn restore(&mut self) -> Result<(), Self::Error>;

    /// Limits drawing to `rect` (in the current space) intersected with the current clip.
    ///
    /// # Errors
    /// Returns the backend's error.
    fn clip(&mut self, rect: Rect) -> Result<(), Self::Error>;

    /// Moves the pixels already drawn inside `rect` (in the current space) down by `dy` rows (up when negative), the
    /// way a scroll does, and returns true. What comes into view at the top or bottom edge is left unspecified: the
    /// caller draws it. Returns false, and changes nothing, when this backend cannot (the default), and then the
    /// caller redraws the whole area instead. It ignores the clip, and `rect` must lie inside the surface.
    fn shift_area(&mut self, rect: Rect, dy: i32) -> bool {
        let _ = (rect, dy);
        false
    }

    /// Says that this frame will draw only inside `rect` (in the current space), so a backend that renders lazily
    /// can rasterise just that. Call it once for each area, before drawing in it. It is a hint: ignoring it is always
    /// correct, and the default does.
    fn will_draw(&mut self, rect: Rect) {
        let _ = rect;
    }

    /// Moves the origin by `(dx, dy)`.
    ///
    /// # Errors
    /// Returns the backend's error.
    fn translate(&mut self, dx: i32, dy: i32) -> Result<(), Self::Error>;

    /// Replaces every pixel inside the current clip with `color` (alpha is ignored).
    ///
    /// # Errors
    /// Returns the backend's error.
    fn clear(&mut self, color: Color) -> Result<(), Self::Error>;

    /// Fills a rectangle; `radius` rounds the corners (clamped to half the shorter side).
    ///
    /// # Errors
    /// Returns the backend's error.
    fn fill_rect(&mut self, rect: Rect, radius: u32, color: Color) -> Result<(), Self::Error>;

    /// Outlines a rectangle with a stroke of `width` that lies *inside* the rectangle.
    ///
    /// # Errors
    /// Returns the backend's error.
    fn stroke_rect(
        &mut self,
        rect: Rect,
        radius: u32,
        width: u32,
        color: Color,
    ) -> Result<(), Self::Error>;

    /// Fills the disc around `center` that covers the pixels within `radius` of it.
    ///
    /// # Errors
    /// Returns the backend's error.
    fn fill_circle(&mut self, center: Point, radius: u32, color: Color) -> Result<(), Self::Error>;

    /// Fills a path (even-odd rule; open sub-paths are closed).
    ///
    /// # Errors
    /// [`Error::TooComplex`] when the path is too big for the backend.
    fn fill_path(&mut self, path: &[PathCmd], color: Color) -> Result<(), Self::Error>;

    /// Strokes a path with a line `width` centered on it.
    ///
    /// # Errors
    /// [`Error::TooComplex`] when the path is too big for the backend.
    fn stroke_path(
        &mut self,
        path: &[PathCmd],
        width: u32,
        color: Color,
    ) -> Result<(), Self::Error>;

    /// Draws a single line of text. `at` is the top of the line and, depending on the alignment of
    /// `style`, its left edge, center or right edge.
    ///
    /// # Errors
    /// Returns the backend's error.
    fn draw_text(&mut self, text: &str, at: Point, style: &TextStyle) -> Result<(), Self::Error>;

    /// The size [`draw_text`](Self::draw_text) would cover.
    fn measure_text(&self, text: &str, style: &TextStyle) -> Size;

    /// Draws `image` with its top-left corner at `at`; transparent pixels leave the target alone.
    ///
    /// # Errors
    /// Returns the backend's error.
    fn draw_image(&mut self, image: &Image<'_>, at: Point) -> Result<(), Self::Error>;
}

/// Reads back what was drawn, for tests and screenshots.
pub trait Probe {
    /// The color shown at `(x, y)` in device space (not the translated space), `None` outside.
    fn pixel(&self, x: i32, y: i32) -> Option<Color>;
}
