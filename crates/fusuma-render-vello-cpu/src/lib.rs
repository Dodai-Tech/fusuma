#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

#[cfg(feature = "png")]
extern crate alloc;

use fusuma_render::Renderer;
pub use fusuma_render_vello_common::Scene;
use fusuma_render_vello_common::{
    Backend, MAX_SIDE, PaintType,
    kurbo::{Affine, BezPath, Rect as KRect},
    peniko::Fill,
    stroke,
};
use vello_cpu::{Pixmap, RenderContext, Resources};

/// The Fusuma renderer on `vello_cpu`.
pub type VelloCpu = Backend<CpuScene>;

/// A `vello_cpu` context and the pixmap it renders into.
#[derive(Debug)]
pub struct CpuScene {
    ctx: RenderContext,
    resources: Resources,
    target: Pixmap,
    size: (u16, u16),
    dirty: bool,
}

impl CpuScene {
    /// A transparent scene of `width` x `height` pixels; `None` when a side is zero or above
    /// `u16::MAX`.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        let (w, h) = (u16::try_from(width).ok()?, u16::try_from(height).ok()?);
        Some(Self {
            ctx: RenderContext::new(w, h),
            resources: Resources::new(),
            target: Pixmap::new(w, h),
            size: (w, h),
            dirty: true,
        })
    }

    /// The context, for what Vello offers beyond Fusuma's trait (layers, filters, blurs).
    /// Anything drawn through it shows up in the next frame.
    pub fn context_mut(&mut self) -> &mut RenderContext {
        self.dirty = true;
        &mut self.ctx
    }
}

impl Scene for CpuScene {
    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn reset(&mut self) {
        self.ctx.reset();
        self.dirty = true;
    }

    fn set_translation(&mut self, dx: f64, dy: f64) {
        self.ctx.set_transform(Affine::translate((dx, dy)));
    }

    fn set_paint(&mut self, paint: PaintType) {
        self.ctx.set_paint(paint);
    }

    fn set_paint_transform(&mut self, transform: Affine) {
        self.ctx.set_paint_transform(transform);
    }

    fn fill_rect(&mut self, rect: &KRect) {
        self.ctx.fill_rect(rect);
        self.dirty = true;
    }

    fn fill_path(&mut self, path: &BezPath) {
        self.ctx.set_fill_rule(Fill::EvenOdd);
        self.ctx.fill_path(path);
        self.ctx.set_fill_rule(Fill::NonZero);
        self.dirty = true;
    }

    fn stroke_path(&mut self, path: &BezPath, width: f64) {
        self.ctx.set_stroke(stroke(width));
        self.ctx.stroke_path(path);
        self.dirty = true;
    }

    fn push_clip(&mut self, rect: &KRect) {
        self.ctx.push_clip_rect(rect);
    }

    fn pop_clip(&mut self) {
        self.ctx.pop_clip();
    }

    fn pixels(&mut self) -> &[u8] {
        if self.dirty {
            self.ctx.flush();
            // The scene is drawn from nothing every time, so what was there is cleared first.
            self.target.data_as_u8_slice_mut().fill(0);
            self.ctx.render(&mut self.target, &mut self.resources);
            self.dirty = false;
        }
        self.target.data_as_u8_slice()
    }
}

/// A renderer on a new transparent surface; `None` when a side is zero or above `u16::MAX`.
#[must_use]
pub fn new(width: u32, height: u32) -> Option<VelloCpu> {
    CpuScene::new(width, height).map(Backend::new)
}

/// The PNG encoder failed.
#[cfg(feature = "png")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodeError(pub alloc::string::String);

/// The surface as a PNG file.
///
/// # Errors
/// [`EncodeError`] with the encoder's message.
#[cfg(feature = "png")]
pub fn png(renderer: &VelloCpu) -> Result<alloc::vec::Vec<u8>, EncodeError> {
    renderer.with_scene(|scene| {
        let (w, h) = scene.size();
        let rgba = scene.pixels().to_vec();
        let mut pixmap = Pixmap::new(w, h);
        pixmap.data_as_u8_slice_mut().copy_from_slice(&rgba);
        pixmap
            .into_png()
            .map_err(|e| EncodeError(alloc::string::ToString::to_string(&e)))
    })
}

/// Which renderer trait this is, for a doc test of the bound.
#[doc(hidden)]
pub fn assert_renderer<R: Renderer>(_: &R) {}

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma_render::{Color, Point, Probe, Rect};

    #[test]
    fn passes_the_shared_conformance_suite() {
        fusuma_render::conformance::run_all(|| new(64, 64).unwrap());
        fusuma_render::conformance::run_all(|| new(200, 120).unwrap());
    }

    #[test]
    fn sizes_that_cannot_exist_are_refused() {
        assert!(new(0, 10).is_none());
        assert!(new(10, 0).is_none());
        assert!(new(MAX_SIDE + 1, 10).is_none());
        assert!(new(10, MAX_SIDE + 1).is_none());
    }

    #[test]
    fn drawing_after_reading_pixels_keeps_working() {
        let mut r = new(16, 16).unwrap();
        r.clear(Color::BLACK).unwrap();
        r.fill_rect(Rect::new(0, 0, 4, 4), 0, Color::rgb(255, 0, 0))
            .unwrap();
        assert_eq!(r.pixel(1, 1).unwrap(), Color::rgb(255, 0, 0));
        r.fill_rect(Rect::new(8, 8, 4, 4), 0, Color::rgb(0, 255, 0))
            .unwrap();
        assert_eq!(
            r.pixel(9, 9).unwrap(),
            Color::rgb(0, 255, 0),
            "drawn after a read"
        );
        assert_eq!(
            r.pixel(1, 1).unwrap(),
            Color::rgb(255, 0, 0),
            "earlier drawing is kept"
        );
    }

    #[test]
    fn many_frames_of_clear_and_draw_do_not_grow_the_scene() {
        let mut r = new(64, 64).unwrap();
        for i in 0..200 {
            r.clear(Color::BLACK).unwrap();
            r.fill_circle(Point::new(i % 60, 30), 5, Color::WHITE)
                .unwrap();
            assert_eq!(r.pixel(i % 60, 30).unwrap(), Color::WHITE);
        }
    }

    #[test]
    fn png_has_the_signature_and_decodes_to_the_size() {
        let mut r = new(30, 20).unwrap();
        r.clear(Color::rgb(10, 20, 30)).unwrap();
        let bytes = png(&r).unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let back = Pixmap::from_png(std::io::Cursor::new(&bytes)).unwrap();
        assert_eq!((back.width(), back.height()), (30, 20));
        let c = back.data()[0];
        assert_eq!((c.r, c.g, c.b, c.a), (10, 20, 30, 255));
    }

    #[test]
    fn the_context_is_open_for_what_vello_offers_beyond_the_trait() {
        use fusuma_render_vello_common::{color::AlphaColor, kurbo::Rect as K};
        let mut r = new(16, 16).unwrap();
        r.clear(Color::BLACK).unwrap();
        r.with_scene(|s| {
            let ctx = s.context_mut();
            ctx.set_paint(AlphaColor::from_rgba8(255, 255, 255, 255));
            ctx.fill_rect(&K::new(2.0, 2.0, 6.0, 6.0));
        });
        assert_eq!(r.pixel(3, 3).unwrap(), Color::WHITE);
    }
}
