#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

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

/// The most memory a damaged region is rasterised through at once, in bytes: the extra RAM a partial render costs,
/// whatever the size of the region (a 410-pixel-wide band of 78 rows).
pub const SCRATCH_BYTES: usize = 128 * 1024;

/// The most picture moves one frame may ask for.
const MAX_SHIFTS: usize = 4;

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
    /// The parts of the next frame that matter, from [`Scene::add_damage`] and [`Scene::set_damage`].
    damage: alloc::vec::Vec<(i32, i32, u32, u32)>,
    /// Moves of the picture to make before the damage is rendered, from [`Scene::shift`].
    shifts: alloc::vec::Vec<((i32, i32, u32, u32), i32)>,
    /// The reused buffer a damaged region is rasterised through, in bands.
    scratch: alloc::vec::Vec<u8>,
    /// Draw commands in the scene not yet in the picture; for the tests, which check the scene does not grow.
    #[cfg(test)]
    queued: u32,
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
            damage: alloc::vec::Vec::new(),
            shifts: alloc::vec::Vec::new(),
            scratch: alloc::vec::Vec::new(),
            #[cfg(test)]
            queued: 0,
        })
    }

    /// Brings the picture up to date: the moves of what was drawn before, then the parts that changed. With no part
    /// named the whole scene is rendered; with some named, only those.
    ///
    /// The scene holds only what was drawn since the last render, drawn **over** the picture (`SrcOver`), and is
    /// emptied afterwards. Before, a frame that did not clear the whole surface (every partial redraw: a hover, a
    /// press, each scroll step) left its commands in the scene, so each frame rendered all of them again: on the S3
    /// watch a scroll step grew from 135 to 290 us of host time for the same 2800 pixels within eight frames, and
    /// the memory grew with it. Painting over the picture gives the same pixels, because it holds exactly what those
    /// old commands drew.
    fn render_frame(&mut self) {
        let damage = core::mem::take(&mut self.damage);
        let shifts = core::mem::take(&mut self.shifts);
        let whole = (0, 0, self.size.0, self.size.1);
        let mut parts: alloc::vec::Vec<(u16, u16, u16, u16)> = alloc::vec::Vec::new();
        let mut full = damage.is_empty() && shifts.is_empty();
        for r in damage {
            match self.clip_damage(r) {
                Some(p) if (p.0, p.1, p.2, p.3) == (0, 0, whole.2, whole.3) => full = true,
                Some(p) => parts.push(p),
                None => {
                    // Empty, or the whole surface: told apart here.
                    if r.2 > 0 && r.3 > 0 && self.covers_surface(r) {
                        full = true;
                    }
                }
            }
        }
        if full {
            let settings = vello_cpu::RasterizerSettings {
                target_init: vello_cpu::TargetInit::SrcOver,
                ..vello_cpu::RasterizerSettings::default()
            };
            self.ctx
                .render_with(&mut self.target, &mut self.resources, settings);
        } else {
            for (region, dy) in shifts {
                self.shift_picture(region, dy);
            }
            for p in parts {
                self.render_region(p);
            }
        }
        self.forget_drawn();
    }

    /// Empties the scene once its commands are in the picture, keeping the drawing state (transform, paint, stroke,
    /// fill rule, blend mode) that the next commands are drawn with.
    fn forget_drawn(&mut self) {
        let transform = *self.ctx.transform();
        let paint_transform = *self.ctx.paint_transform();
        let paint = self.ctx.paint().clone();
        let stroke = self.ctx.stroke().clone();
        let fill_rule = *self.ctx.fill_rule();
        let blend = self.ctx.blend_mode();
        self.ctx.reset();
        #[cfg(test)]
        {
            self.queued = 0;
        }
        self.ctx.set_transform(transform);
        self.ctx.set_paint_transform(paint_transform);
        self.ctx.set_paint(paint);
        self.ctx.set_stroke(stroke);
        self.ctx.set_fill_rule(fill_rule);
        self.ctx.set_blend_mode(blend);
    }

    /// Whether `region` covers the whole surface.
    fn covers_surface(&self, r: (i32, i32, u32, u32)) -> bool {
        let (w, h) = (i64::from(self.size.0), i64::from(self.size.1));
        i64::from(r.0) <= 0
            && i64::from(r.1) <= 0
            && i64::from(r.0) + i64::from(r.2) >= w
            && i64::from(r.1) + i64::from(r.3) >= h
    }

    /// Moves the rows of `region` down by `dy` (up when negative) in the picture. What comes in at an edge is left
    /// as it was; the frame renders it.
    fn shift_picture(&mut self, region: (i32, i32, u32, u32), dy: i32) {
        let Some((x, y, w, h)) = self.clip_rect(region) else {
            return;
        };
        let (x, y, w, h) = (
            usize::from(x),
            usize::from(y),
            usize::from(w),
            usize::from(h),
        );
        let by = usize::try_from(dy.unsigned_abs()).unwrap_or(usize::MAX);
        if by == 0 || by >= h {
            return;
        }
        let (stride, row) = (usize::from(self.size.0) * 4, w * 4);
        let data = self.target.data_as_u8_slice_mut();
        let copy = |data: &mut [u8], from: usize, to: usize| {
            let (a, b) = (from * stride + x * 4, to * stride + x * 4);
            data.copy_within(a..a + row, b);
        };
        if dy > 0 {
            // Down: the bottom rows first, so a row is not overwritten before it has moved.
            for to in (y + by..y + h).rev() {
                copy(data, to - by, to);
            }
        } else {
            for to in y..y + h - by {
                copy(data, to + by, to);
            }
        }
    }

    /// `region` cut to the surface, as (x, y, width, height) in pixels; `None` when it is empty or is the whole
    /// surface (a full render is then the same work, without the copy).
    fn clip_damage(&self, region: (i32, i32, u32, u32)) -> Option<(u16, u16, u16, u16)> {
        let p = self.clip_rect(region)?;
        let whole = (0, 0, self.size.0, self.size.1);
        ((p.0, p.1, p.2, p.3) != whole).then_some(p)
    }

    /// `region` cut to the surface; `None` when nothing is left.
    fn clip_rect(&self, region: (i32, i32, u32, u32)) -> Option<(u16, u16, u16, u16)> {
        let (w, h) = (i64::from(self.size.0), i64::from(self.size.1));
        let (x0, y0) = (
            i64::from(region.0).clamp(0, w),
            i64::from(region.1).clamp(0, h),
        );
        let x1 = (i64::from(region.0) + i64::from(region.2)).clamp(0, w);
        let y1 = (i64::from(region.1) + i64::from(region.3)).clamp(0, h);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some((x0 as u16, y0 as u16, (x1 - x0) as u16, (y1 - y0) as u16))
    }

    /// Rasterises only `region` of the scene and copies it into the picture: the cost follows the area that
    /// changed, not the surface. It works in bands of rows through one reused buffer of at most [`SCRATCH_BYTES`],
    /// so a large region does not allocate a second picture-sized pixmap.
    fn render_region(&mut self, (x, y, w, h): (u16, u16, u16, u16)) {
        let row = usize::from(w) * 4;
        let band = (SCRATCH_BYTES / row.max(1)).clamp(1, usize::from(h));
        let stride = usize::from(self.size.0) * 4;
        let mut top = 0usize;
        while top < usize::from(h) {
            let rows = band.min(usize::from(h) - top);
            let bytes = rows * row;
            if self.scratch.len() < bytes {
                self.scratch.resize(bytes, 0);
            }
            // `rows` is at most `h`, so it fits a u16.
            // The band starts as the picture has it, and the scene is drawn over it.
            let src = self.target.data_as_u8_slice();
            for r in 0..rows {
                let at = (usize::from(y) + top + r) * stride + usize::from(x) * 4;
                self.scratch[r * row..(r + 1) * row].copy_from_slice(&src[at..at + row]);
            }
            let Some(part) = vello_cpu::PixmapMut::new(w, rows as u16, &mut self.scratch[..bytes])
            else {
                return;
            };
            let settings = vello_cpu::RasterizerSettings {
                offset: (-i32::from(x), -(i32::from(y) + top as i32)),
                target_init: vello_cpu::TargetInit::SrcOver,
                ..vello_cpu::RasterizerSettings::default()
            };
            self.ctx.render_with(part, &mut self.resources, settings);
            let dst = self.target.data_as_u8_slice_mut();
            for r in 0..rows {
                let at = (usize::from(y) + top + r) * stride + usize::from(x) * 4;
                dst[at..at + row].copy_from_slice(&self.scratch[r * row..(r + 1) * row]);
            }
            top += rows;
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
        #[cfg(test)]
        {
            self.queued = 0;
        }
        self.dirty = true;
        self.damage.clear();
        self.shifts.clear();
    }

    fn set_damage(&mut self, region: (i32, i32, u32, u32)) {
        // A frame that said which parts it draws (`add_damage`) knows better than a platform's bounding box.
        if self.damage.is_empty() {
            self.damage.push(region);
        }
    }

    fn add_damage(&mut self, region: (i32, i32, u32, u32)) {
        self.damage.push(region);
    }

    fn shift(&mut self, region: (i32, i32, u32, u32), dy: i32) -> bool {
        // More than a few moves in one frame is not a scroll: let the caller redraw.
        if self.shifts.len() >= MAX_SHIFTS {
            return false;
        }
        self.shifts.push((region, dy));
        true
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
        #[cfg(test)]
        {
            self.queued += 1;
        }
    }

    fn fill_path(&mut self, path: &BezPath) {
        self.ctx.set_fill_rule(Fill::EvenOdd);
        self.ctx.fill_path(path);
        self.ctx.set_fill_rule(Fill::NonZero);
        self.dirty = true;
        #[cfg(test)]
        {
            self.queued += 1;
        }
    }

    fn stroke_path(&mut self, path: &BezPath, width: f64) {
        self.ctx.set_stroke(stroke(width));
        self.ctx.stroke_path(path);
        self.dirty = true;
        #[cfg(test)]
        {
            self.queued += 1;
        }
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
        #[cfg(test)]
        {
            self.queued += 1;
        }
        drawn
    }

    fn pixels(&mut self) -> &[u8] {
        if self.dirty {
            self.ctx.flush();
            self.render_frame();
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

    #[test]
    fn a_region_taller_than_the_scratch_buffer_is_rendered_in_bands_and_is_still_exact() {
        use fusuma_render::Probe;
        // 400 px wide: 128 KB holds 81 rows, so a 300-row region takes four bands.
        let draw = |r: &mut VelloCpu| {
            r.clear(Color::rgb(10, 10, 40)).unwrap();
            r.fill_rect(Rect::new(5, 5, 390, 290), 12, Color::rgb(220, 80, 40))
                .unwrap();
            r.fill_circle(Point::new(200, 150), 90, Color::rgba(40, 200, 220, 180))
                .unwrap();
            r.draw_text(
                "banded",
                Point::new(30, 140),
                &fusuma_render::TextStyle::new(24, Color::WHITE),
            )
            .unwrap();
        };
        let mut full = new(400, 320).unwrap();
        draw(&mut full);
        let mut part = new(400, 320).unwrap();
        draw(&mut part);
        part.with_scene(|s| s.set_damage((0, 10, 400, 300)));
        for y in 10..310 {
            for x in 0..400 {
                assert_eq!(part.pixel(x, y), full.pixel(x, y), "({x}, {y})");
            }
        }
        part.with_scene(|s| {
            assert!(
                s.scratch.len() <= SCRATCH_BYTES,
                "{} bytes",
                s.scratch.len()
            )
        });
    }

    #[test]
    fn the_scratch_buffer_is_reused_and_never_grows_past_its_cap() {
        let mut r = new(600, 600).unwrap();
        for round in 0..5 {
            r.with_scene(Scene::reset);
            r.fill_rect(Rect::new(0, 0, 600, 600), 0, Color::rgb(round * 10, 0, 0))
                .unwrap();
            r.with_scene(|s| s.set_damage((0, 0, 599, 599)));
            r.with_scene(|s| {
                let _ = s.pixels();
                assert!(s.scratch.len() <= SCRATCH_BYTES);
            });
        }
        // A one-pixel-wide, very tall region is one row per band at worst, and still correct.
        r.with_scene(Scene::reset);
        r.fill_rect(Rect::new(0, 0, 600, 600), 0, Color::rgb(1, 2, 3))
            .unwrap();
        r.with_scene(|s| s.set_damage((7, 0, 1, 600)));
        use fusuma_render::Probe;
        assert_eq!(r.pixel(7, 599), Some(Color::rgb(1, 2, 3)));
    }

    /// A picture with a vertical gradient of distinct rows and some text, so a wrong move shows.
    fn rows(r: &mut VelloCpu) {
        r.clear(Color::rgb(0, 0, 0)).unwrap();
        for y in 0..80 {
            r.fill_rect(
                Rect::new(0, y, 100, 1),
                0,
                Color::rgb(y as u8 * 3, 255 - y as u8 * 3, 40),
            )
            .unwrap();
        }
    }

    #[test]
    fn shifting_moves_the_picture_and_the_frame_renders_what_came_in() {
        use fusuma_render::Probe;
        for dy in [-7, -1, 1, 7, -40, 33] {
            let mut r = new(100, 80).unwrap();
            rows(&mut r);
            assert_eq!(
                r.pixel(0, 0),
                Some(Color::rgb(0, 255, 40)),
                "the first frame is drawn"
            );
            // The next frame: move rows 10..70 by dy, and say what is drawn anew (the strip that came in).
            r.with_scene(Scene::reset);
            assert!(r.shift_area(Rect::new(0, 10, 100, 60), dy));
            let strip = if dy > 0 {
                Rect::new(0, 10, 100, dy as u32)
            } else {
                Rect::new(0, 70 + dy, 100, dy.unsigned_abs())
            };
            r.will_draw(strip);
            r.fill_rect(strip, 0, Color::rgb(250, 0, 250)).unwrap();
            for y in 0..80 {
                let want = if !(10..70).contains(&y) {
                    Color::rgb(y as u8 * 3, 255 - y as u8 * 3, 40)
                } else if (strip.y..strip.bottom()).contains(&y) {
                    Color::rgb(250, 0, 250)
                } else {
                    let from = y - dy;
                    Color::rgb(from as u8 * 3, 255 - from as u8 * 3, 40)
                };
                assert_eq!(r.pixel(50, y), Some(want), "dy {dy}, row {y}");
            }
        }
    }

    /// Regression: a frame that does not clear the whole surface (every partial redraw) left its commands in the
    /// scene, so each later frame rendered all of them again and the S3 watch's home screen got slower with every
    /// hover, press and scroll step.
    #[test]
    fn partial_frames_do_not_pile_up_in_the_scene_and_keep_what_was_drawn() {
        use fusuma_render::Probe;
        let mut r = new(60, 40).unwrap();
        r.clear(Color::BLACK).unwrap();
        r.fill_rect(Rect::new(0, 0, 30, 40), 0, Color::rgb(0, 0, 255))
            .unwrap();
        assert_eq!(r.pixel(5, 5), Some(Color::rgb(0, 0, 255)));
        for i in 0..50u8 {
            // A redraw of one small part, as the toolkit does it: say what is drawn, clip to it, paint it.
            let part = Rect::new(40, 10, 10, 10);
            r.will_draw(part);
            r.save().unwrap();
            r.clip(part).unwrap();
            r.clear(Color::BLACK).unwrap();
            r.fill_rect(part, 2, Color::rgb(i, 200, 0)).unwrap();
            r.restore().unwrap();
            let queued = r.with_scene(|s| s.queued);
            assert!(
                queued <= 2,
                "frame {i}: {queued} commands waiting, the scene grows"
            );
            assert_eq!(r.pixel(45, 15), Some(Color::rgb(i, 200, 0)), "frame {i}");
        }
        assert_eq!(
            r.pixel(5, 5),
            Some(Color::rgb(0, 0, 255)),
            "what an old frame drew stays"
        );
        assert_eq!(r.pixel(55, 35), Some(Color::BLACK));
    }

    #[test]
    fn a_translation_outlives_the_render_that_empties_the_scene() {
        use fusuma_render::Probe;
        let mut r = new(40, 40).unwrap();
        r.clear(Color::BLACK).unwrap();
        r.translate(10, 20).unwrap();
        r.fill_rect(Rect::new(0, 0, 4, 4), 0, Color::WHITE).unwrap();
        assert_eq!(
            r.pixel(11, 21),
            Some(Color::WHITE),
            "rendered, scene emptied"
        );
        r.fill_rect(Rect::new(5, 0, 4, 4), 0, Color::rgb(255, 0, 0))
            .unwrap();
        assert_eq!(
            r.pixel(16, 21),
            Some(Color::rgb(255, 0, 0)),
            "still translated"
        );
        assert_eq!(r.pixel(6, 1), Some(Color::BLACK));
        assert_eq!(
            r.pixel(11, 21),
            Some(Color::WHITE),
            "the first square stays"
        );
    }

    #[test]
    fn a_shift_of_nothing_or_of_the_whole_height_changes_no_pixel() {
        use fusuma_render::Probe;
        let mut r = new(100, 80).unwrap();
        rows(&mut r);
        let before: Vec<_> = (0..80).map(|y| r.pixel(50, y)).collect();
        for dy in [0, 60, -60, 500] {
            r.with_scene(Scene::reset);
            assert!(r.shift_area(Rect::new(0, 10, 100, 60), dy));
            r.will_draw(Rect::new(0, 0, 1, 1));
            r.fill_rect(Rect::new(0, 0, 1, 1), 0, Color::rgb(0, 0, 0))
                .unwrap();
            let after: Vec<_> = (0..80).map(|y| r.pixel(50, y)).collect();
            assert_eq!(after, before, "dy {dy}");
        }
    }

    #[test]
    fn shift_areas_that_do_not_fit_the_surface_are_refused_and_too_many_moves_are_too() {
        let mut r = new(100, 80).unwrap();
        assert!(
            !r.shift_area(Rect::new(0, 0, 200, 10), 1),
            "wider than the surface"
        );
        assert!(
            !r.shift_area(Rect::new(0, 70, 100, 20), 1),
            "past the bottom"
        );
        assert!(!r.shift_area(Rect::new(0, 0, 0, 0), 1), "empty");
        r.with_scene(Scene::reset);
        assert!((0..4).all(|_| r.shift_area(Rect::new(0, 0, 10, 10), 1)));
        assert!(
            !r.shift_area(Rect::new(0, 0, 10, 10), 1),
            "the fifth in one frame"
        );
    }

    #[test]
    fn several_damage_areas_are_each_rendered_and_nothing_between_them_is() {
        use fusuma_render::Probe;
        let mut r = new(100, 80).unwrap();
        r.clear(Color::rgb(9, 9, 9)).unwrap();
        assert_eq!(r.pixel(50, 40), Some(Color::rgb(9, 9, 9)));
        r.with_scene(Scene::reset);
        for rect in [Rect::new(0, 0, 10, 10), Rect::new(90, 70, 10, 10)] {
            r.will_draw(rect);
            r.fill_rect(rect, 0, Color::rgb(1, 2, 3)).unwrap();
        }
        assert_eq!(r.pixel(5, 5), Some(Color::rgb(1, 2, 3)));
        assert_eq!(r.pixel(95, 75), Some(Color::rgb(1, 2, 3)));
        assert_eq!(
            r.pixel(50, 40),
            Some(Color::rgb(9, 9, 9)),
            "the middle was not rendered, so it is as it was"
        );
    }
}
