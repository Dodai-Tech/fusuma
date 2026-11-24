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

    /// One scene on both backends: shapes without curves, text and an image must match, because
    /// the two backends share the fonts and the rules.
    fn scene<R: fusuma_render::Renderer>(r: &mut R) {
        use fusuma_render::{Align, Image, TextStyle};
        let white = Color::WHITE;
        r.clear(Color::rgb(24, 24, 32)).unwrap();
        r.fill_rect(Rect::new(8, 8, 100, 40), 0, Color::rgb(0, 160, 160))
            .unwrap();
        r.stroke_rect(Rect::new(8, 60, 100, 30), 0, 2, Color::rgb(250, 200, 0))
            .unwrap();
        r.draw_text("Dodai", Point::new(12, 14), &TextStyle::new(20, white))
            .unwrap();
        r.draw_text(
            "Fusuma",
            Point::new(110, 70),
            &TextStyle::new(14, white).aligned(Align::Right),
        )
        .unwrap();
        r.save().unwrap();
        r.translate(120, 10).unwrap();
        r.clip(Rect::new(0, 0, 30, 30)).unwrap();
        r.fill_rect(Rect::new(-10, -10, 60, 60), 0, Color::rgb(200, 40, 40))
            .unwrap();
        r.draw_text("clip", Point::new(2, 8), &TextStyle::new(10, white))
            .unwrap();
        r.restore().unwrap();
        let data = [
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 0,
        ];
        r.draw_image(&Image::new(2, 2, &data).unwrap(), Point::new(150, 100))
            .unwrap();
    }

    #[test]
    fn the_same_scene_looks_the_same_as_on_the_integer_backend() {
        use fusuma_render_embedded::{Embedded, Frame};
        let mut a = new(180, 120).unwrap();
        let mut b = Embedded::new(Frame::new(180, 120, vec![0u16; 180 * 120]).unwrap());
        scene(&mut a);
        scene(&mut b);
        let differing = (0..120)
            .flat_map(|y| (0..180).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let (p, q) = (a.pixel(x, y).unwrap(), b.pixel(x, y).unwrap());
                p.r.abs_diff(q.r) > 10 || p.g.abs_diff(q.g) > 10 || p.b.abs_diff(q.b) > 10
            })
            .count();
        assert_eq!(
            differing, 0,
            "{differing} pixels differ between the backends"
        );
    }

    #[test]
    fn curves_agree_with_the_integer_backend_except_on_their_edges() {
        use fusuma_render::triangle;
        use fusuma_render_embedded::{Embedded, Frame};
        fn curves<R: fusuma_render::Renderer>(r: &mut R) {
            r.clear(Color::BLACK).unwrap();
            r.fill_circle(Point::new(40, 40), 25, Color::WHITE).unwrap();
            r.fill_rect(Rect::new(80, 10, 50, 50), 14, Color::WHITE)
                .unwrap();
            r.fill_path(
                &triangle(Point::new(10, 110), Point::new(70, 110), Point::new(40, 70)),
                Color::WHITE,
            )
            .unwrap();
        }
        let mut a = new(140, 120).unwrap();
        let mut b = Embedded::new(Frame::new(140, 120, vec![0u16; 140 * 120]).unwrap());
        curves(&mut a);
        curves(&mut b);
        let differing = (0..120)
            .flat_map(|y| (0..140).map(move |x| (x, y)))
            .filter(|&(x, y)| (a.pixel(x, y).unwrap().r > 127) != (b.pixel(x, y).unwrap().r > 127))
            .count();
        let area = core::f64::consts::PI * 25.0 * 25.0 + 50.0 * 50.0 + 30.0 * 60.0;
        assert!(
            (differing as f64) < area * 0.03,
            "{differing} of {area:.0} pixels differ"
        );
    }

    #[test]
    fn text_at_twice_the_height_is_the_same_glyphs_twice_as_big() {
        use fusuma_render::{Probe, TextStyle};
        let paint = |size: u32| {
            let mut r = new(120, 60).unwrap();
            r.clear(Color::BLACK).unwrap();
            r.draw_text("I", Point::new(0, 0), &TextStyle::new(size, Color::WHITE))
                .unwrap();
            r
        };
        let (small, big) = (paint(20), paint(40));
        let lit = |r: &VelloCpu, w: i32, h: i32| {
            (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| r.pixel(x, y) != Some(Color::BLACK))
                .count()
        };
        let (a, b) = (lit(&small, 120, 60), lit(&big, 120, 60));
        assert_eq!(
            b,
            a * 4,
            "twice as wide and twice as tall is four times the pixels: {a} {b}"
        );
        // Every lit pixel of the small glyph is a 2x2 block in the big one.
        for y in 0..20 {
            for x in 0..10 {
                let on = small.pixel(x, y) != Some(Color::BLACK);
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(
                        big.pixel(x * 2 + dx, y * 2 + dy) != Some(Color::BLACK),
                        on,
                        "({x}, {y})"
                    );
                }
            }
        }
        assert_eq!(
            small.measure_text("abc", &TextStyle::new(20, Color::WHITE)),
            fusuma_render::Size::new(30, 20)
        );
        assert_eq!(
            big.measure_text("abc", &TextStyle::new(40, Color::WHITE)),
            fusuma_render::Size::new(60, 40)
        );
    }
}
