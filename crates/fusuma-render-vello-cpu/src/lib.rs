#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

#[cfg(any(feature = "png", feature = "text"))]
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
    /// The part of the next frame that matters, from [`Scene::set_damage`].
    damage: Option<(i32, i32, u32, u32)>,
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
            damage: None,
        })
    }

    /// `region` cut to the surface, as (x, y, width, height) in pixels; `None` when it is empty or is the whole
    /// surface (a full render is then the same work, without the copy).
    fn clip_damage(&self, region: (i32, i32, u32, u32)) -> Option<(u16, u16, u16, u16)> {
        let (w, h) = (i64::from(self.size.0), i64::from(self.size.1));
        let (x0, y0) = (
            i64::from(region.0).clamp(0, w),
            i64::from(region.1).clamp(0, h),
        );
        let x1 = (i64::from(region.0) + i64::from(region.2)).clamp(0, w);
        let y1 = (i64::from(region.1) + i64::from(region.3)).clamp(0, h);
        if x1 <= x0 || y1 <= y0 || (x0, y0, x1, y1) == (0, 0, w, h) {
            return None;
        }
        Some((x0 as u16, y0 as u16, (x1 - x0) as u16, (y1 - y0) as u16))
    }

    /// Rasterises only `region` of the scene, into a pixmap of that size, and copies it into the picture: the cost
    /// follows the area that changed, not the surface.
    fn render_region(&mut self, (x, y, w, h): (u16, u16, u16, u16)) {
        let mut part = Pixmap::new(w, h);
        let settings = vello_cpu::RasterizerSettings {
            offset: (-i32::from(x), -i32::from(y)),
            ..vello_cpu::RasterizerSettings::default()
        };
        self.ctx
            .render_with(&mut part, &mut self.resources, settings);
        let stride = usize::from(self.size.0) * 4;
        let row = usize::from(w) * 4;
        let src = part.data_as_u8_slice();
        let dst = self.target.data_as_u8_slice_mut();
        for r in 0..usize::from(h) {
            let at = (usize::from(y) + r) * stride + usize::from(x) * 4;
            dst[at..at + row].copy_from_slice(&src[r * row..(r + 1) * row]);
        }
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
        self.damage = None;
    }

    fn set_damage(&mut self, region: (i32, i32, u32, u32)) {
        self.damage = Some(region);
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

    #[cfg(feature = "text")]
    fn can_draw_glyphs(&self) -> bool {
        true
    }

    #[cfg(feature = "text")]
    fn draw_glyphs(
        &mut self,
        font: &fusuma_render_vello_common::peniko::FontData,
        size: f32,
        glyphs: &[fusuma_text::Glyph],
        origin: (f64, f64),
    ) -> bool {
        let (ox, oy) = (origin.0 as f32, origin.1 as f32);
        let placed: alloc::vec::Vec<vello_cpu::Glyph> = glyphs
            .iter()
            .map(|g| vello_cpu::Glyph {
                id: g.id,
                x: g.x + ox,
                y: g.y + oy,
            })
            .collect();
        let drawn = self
            .ctx
            .glyph_run(&mut self.resources, font)
            .font_size(size)
            .hint(true)
            .fill_glyphs(placed.into_iter())
            .is_ok();
        self.dirty = true;
        drawn
    }

    fn pixels(&mut self) -> &[u8] {
        if self.dirty {
            self.ctx.flush();
            match self.damage.take().and_then(|r| self.clip_damage(r)) {
                Some(region) => self.render_region(region),
                None => {
                    // The scene is drawn from nothing every time, so what was there is cleared first.
                    self.target.data_as_u8_slice_mut().fill(0);
                    self.ctx.render(&mut self.target, &mut self.resources);
                }
            }
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
        use fusuma_render::{Align, Font, Image, TextStyle};
        let white = Color::WHITE;
        r.clear(Color::rgb(24, 24, 32)).unwrap();
        r.fill_rect(Rect::new(8, 8, 100, 40), 0, Color::rgb(0, 160, 160))
            .unwrap();
        r.stroke_rect(Rect::new(8, 60, 100, 30), 0, 2, Color::rgb(250, 200, 0))
            .unwrap();
        r.draw_text(
            "Dodai",
            Point::new(12, 14),
            &TextStyle::new(20, white).font(Font::Bitmap),
        )
        .unwrap();
        r.draw_text(
            "Fusuma",
            Point::new(110, 70),
            &TextStyle::new(14, white)
                .font(Font::Bitmap)
                .aligned(Align::Right),
        )
        .unwrap();
        r.save().unwrap();
        r.translate(120, 10).unwrap();
        r.clip(Rect::new(0, 0, 30, 30)).unwrap();
        r.fill_rect(Rect::new(-10, -10, 60, 60), 0, Color::rgb(200, 40, 40))
            .unwrap();
        r.draw_text(
            "clip",
            Point::new(2, 8),
            &TextStyle::new(10, white).font(Font::Bitmap),
        )
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
            r.draw_text(
                "I",
                Point::new(0, 0),
                &TextStyle::new(size, Color::WHITE).font(fusuma_render::Font::Bitmap),
            )
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
            small.measure_text(
                "abc",
                &TextStyle::new(20, Color::WHITE).font(fusuma_render::Font::Bitmap)
            ),
            fusuma_render::Size::new(30, 20)
        );
        assert_eq!(
            big.measure_text(
                "abc",
                &TextStyle::new(40, Color::WHITE).font(fusuma_render::Font::Bitmap)
            ),
            fusuma_render::Size::new(60, 40)
        );
    }

    /// Vector text: a proportional font through Parley and `glyph_run`, not the bitmap one.
    #[cfg(feature = "inter")]
    mod vector {
        use super::*;
        use fusuma_render::{Font, Probe, TextStyle};

        fn lit(r: &VelloCpu, w: i32, h: i32) -> usize {
            (0..h)
                .flat_map(|y| (0..w).map(move |x| (x, y)))
                .filter(|&(x, y)| r.pixel(x, y) != Some(Color::BLACK))
                .count()
        }

        fn paint(text: &str, style: TextStyle) -> VelloCpu {
            let mut r = new(260, 60).unwrap();
            r.clear(Color::BLACK).unwrap();
            r.draw_text(text, Point::new(4, 4), &style).unwrap();
            r
        }

        #[test]
        fn the_default_font_is_proportional_and_the_bitmap_one_is_not() {
            let sans = TextStyle::new(24, Color::WHITE);
            let bitmap = sans.font(Font::Bitmap);
            let r = new(8, 8).unwrap();
            let (i, w) = (
                r.measure_text("iiii", &sans).w,
                r.measure_text("WWWW", &sans).w,
            );
            assert!(i < w, "Inter: an i is narrower than a W ({i} against {w})");
            let (bi, bw) = (
                r.measure_text("iiii", &bitmap).w,
                r.measure_text("WWWW", &bitmap).w,
            );
            assert_eq!(bi, bw, "the bitmap font is monospace");
        }

        #[test]
        fn vector_text_draws_anti_aliased_pixels_in_the_text_colour() {
            let r = paint("Hello", TextStyle::new(24, Color::WHITE));
            assert!(lit(&r, 260, 60) > 80, "something was drawn");
            let mut partial = 0;
            for y in 0..60 {
                for x in 0..260 {
                    let p = r.pixel(x, y).unwrap();
                    if p != Color::BLACK && p != Color::WHITE {
                        partial += 1;
                    }
                }
            }
            assert!(
                partial > 20,
                "edges are anti-aliased: {partial} grey pixels"
            );
            // Nothing is drawn above or left of where the text was put.
            assert_eq!(lit(&r, 3, 60), 0);
        }

        #[test]
        fn the_text_stays_inside_the_box_it_was_measured_for() {
            let style = TextStyle::new(24, Color::WHITE);
            let r = paint("Weather Ășț", style);
            let size = r.measure_text("Weather Ășț", &style);
            let (w, h) = (size.w as i32, size.h as i32);
            for y in 0..60 {
                for x in 0..260 {
                    let inside = x >= 4 && x < 4 + w + 1 && y >= 4 && y < 4 + h + 1;
                    if !inside {
                        assert_eq!(
                            r.pixel(x, y),
                            Some(Color::BLACK),
                            "({x}, {y}) is outside {w}x{h}"
                        );
                    }
                }
            }
        }

        #[test]
        fn alignment_and_a_translation_move_the_text_as_they_do_for_the_bitmap_font() {
            use fusuma_render::Align;
            let style = TextStyle::new(24, Color::WHITE);
            let centred = {
                let mut r = new(260, 60).unwrap();
                r.clear(Color::BLACK).unwrap();
                r.draw_text("abc", Point::new(130, 4), &style.aligned(Align::Center))
                    .unwrap();
                r
            };
            let w = centred.measure_text("abc", &style).w as i32;
            let (first, last) = (0..260)
                .filter(|&x| (0..60).any(|y| centred.pixel(x, y) != Some(Color::BLACK)))
                .fold((i32::MAX, i32::MIN), |(a, b), x| (a.min(x), b.max(x)));
            assert!(
                (first - (130 - w / 2)).abs() <= 3 && (last - (130 + w / 2)).abs() <= 3,
                "{first}..{last} for width {w}"
            );
            let mut r = new(260, 60).unwrap();
            r.clear(Color::BLACK).unwrap();
            r.save().unwrap();
            r.translate(100, 20).unwrap();
            r.draw_text("abc", Point::new(0, 0), &style).unwrap();
            r.restore().unwrap();
            assert_eq!(lit(&r, 99, 60), 0, "left of the translation");
            assert!((20..50).any(|y| (100..160).any(|x| r.pixel(x, y) != Some(Color::BLACK))));
        }

        #[test]
        fn a_clip_cuts_the_glyphs_and_a_hidden_text_draws_nothing() {
            let mut r = new(260, 60).unwrap();
            r.clear(Color::BLACK).unwrap();
            r.save().unwrap();
            r.clip(Rect::new(0, 0, 30, 60)).unwrap();
            r.draw_text(
                "Hello world",
                Point::new(4, 4),
                &TextStyle::new(24, Color::WHITE),
            )
            .unwrap();
            r.restore().unwrap();
            assert_eq!(
                lit(&r, 260, 60) - lit(&r, 30, 60),
                0,
                "nothing right of the clip"
            );
            assert!(lit(&r, 30, 60) > 10);
            let mut g = new(60, 60).unwrap();
            g.clear(Color::BLACK).unwrap();
            g.draw_text("", Point::new(1, 1), &TextStyle::new(24, Color::WHITE))
                .unwrap();
            g.draw_text(
                "x",
                Point::new(1, 1),
                &TextStyle::new(24, Color::rgba(255, 255, 255, 0)),
            )
            .unwrap();
            assert_eq!(lit(&g, 60, 60), 0);
        }

        #[test]
        fn extreme_positions_and_sizes_do_not_panic() {
            let mut r = new(40, 40).unwrap();
            for at in [
                Point::new(i32::MAX, i32::MAX),
                Point::new(i32::MIN, 0),
                Point::new(0, i32::MIN),
                Point::new(-5000, 5),
            ] {
                r.draw_text("far", at, &TextStyle::new(20, Color::WHITE))
                    .unwrap();
            }
            for size in [0, 1, 7, 400, 100_000] {
                let _ = r.measure_text("x", &TextStyle::new(size, Color::WHITE));
                r.draw_text(
                    "x",
                    Point::new(0, 0),
                    &TextStyle::new(size.min(2000), Color::WHITE),
                )
                .unwrap();
            }
        }
    }

    /// A scene with shapes, text and a clip, drawn the same way on two surfaces.
    fn busy(r: &mut VelloCpu) {
        use fusuma_render::{Align, TextStyle};
        r.clear(Color::rgb(20, 24, 30)).unwrap();
        r.fill_rect(Rect::new(10, 12, 90, 40), 8, Color::rgb(200, 60, 60))
            .unwrap();
        r.fill_circle(Point::new(120, 70), 30, Color::rgba(60, 160, 220, 200))
            .unwrap();
        r.draw_text(
            "Hello",
            Point::new(14, 20),
            &TextStyle::new(20, Color::WHITE),
        )
        .unwrap();
        r.draw_text(
            "right",
            Point::new(190, 90),
            &TextStyle::new(14, Color::WHITE).aligned(Align::Right),
        )
        .unwrap();
        r.save().unwrap();
        r.clip(Rect::new(40, 80, 60, 30)).unwrap();
        r.fill_rect(Rect::new(0, 0, 200, 200), 0, Color::rgb(30, 200, 90))
            .unwrap();
        r.restore().unwrap();
    }

    #[test]
    fn a_damage_region_renders_exactly_what_a_full_render_has_inside_it() {
        use fusuma_render::Probe;
        let mut full = new(200, 120).unwrap();
        busy(&mut full);
        for region in [
            (0, 0, 50, 30),
            (10, 12, 90, 40),
            (95, 60, 105, 60),
            (150, 100, 50, 20),
            (60, 85, 3, 3),
            (199, 119, 1, 1),
        ] {
            let mut part = new(200, 120).unwrap();
            busy(&mut part);
            part.with_scene(|s| s.set_damage((region.0, region.1, region.2, region.3)));
            for y in region.1..region.1 + region.3 as i32 {
                for x in region.0..region.0 + region.2 as i32 {
                    assert_eq!(
                        part.pixel(x, y),
                        full.pixel(x, y),
                        "region {region:?} at ({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn what_is_outside_the_damage_region_is_left_as_the_last_frame_had_it() {
        use fusuma_render::Probe;
        let mut r = new(40, 40).unwrap();
        r.clear(Color::rgb(255, 0, 0)).unwrap();
        assert_eq!(r.pixel(30, 30), Some(Color::rgb(255, 0, 0)));
        r.with_scene(Scene::reset);
        r.fill_rect(Rect::new(0, 0, 10, 10), 0, Color::rgb(0, 0, 255))
            .unwrap();
        r.with_scene(|s| s.set_damage((0, 0, 10, 10)));
        assert_eq!(r.pixel(5, 5), Some(Color::rgb(0, 0, 255)));
        assert_eq!(
            r.pixel(30, 30),
            Some(Color::rgb(255, 0, 0)),
            "not rendered again, not cleared"
        );
    }

    #[test]
    fn odd_damage_regions_are_harmless_and_the_hint_lasts_one_frame() {
        use fusuma_render::Probe;
        let mut r = new(30, 30).unwrap();
        for region in [
            (0, 0, 0, 0),
            (-50, -50, 10, 10),
            (100, 100, 5, 5),
            (0, 0, 30, 30),
            (-5, -5, 100, 100),
            (i32::MAX, 0, 9, 9),
            (i32::MIN, 0, u32::MAX, 5),
            (5, 5, u32::MAX, u32::MAX),
        ] {
            r.with_scene(Scene::reset);
            r.fill_rect(Rect::new(0, 0, 30, 30), 0, Color::rgb(9, 9, 9))
                .unwrap();
            r.with_scene(|s| s.set_damage(region));
            let _ = r.pixel(1, 1);
        }
        // A frame after a hinted one is a full render again.
        r.with_scene(Scene::reset);
        r.fill_rect(Rect::new(0, 0, 30, 30), 0, Color::rgb(7, 7, 7))
            .unwrap();
        assert_eq!(r.pixel(29, 29), Some(Color::rgb(7, 7, 7)));
    }
}
