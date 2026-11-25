//! [`Measure`]: how big text is, which is all a layout needs to know about a renderer.

use crate::geometry::Size;
use crate::renderer::Renderer;
use crate::text::TextStyle;

/// Measures text. A layout asks for nothing else, so it needs no renderer: a text UI measures in character cells
/// (one column a character, one row a line) and lays out the same widgets without drawing a pixel.
pub trait Measure {
    /// The size `text` would cover in `style`: pixels for a renderer, cells for a text UI.
    fn measure_text(&self, text: &str, style: &TextStyle) -> Size;
}

/// Every renderer measures with its own text drawing.
impl<R: Renderer + ?Sized> Measure for R {
    fn measure_text(&self, text: &str, style: &TextStyle) -> Size {
        Renderer::measure_text(self, text, style)
    }
}
