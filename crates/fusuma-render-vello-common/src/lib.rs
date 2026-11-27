#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

mod font;

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::convert::Infallible;

use embedded_graphics::{
    Pixel,
    geometry::{OriginDimensions, Size as EgSize},
    mono_font::MonoTextStyle,
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Drawable, Point as EgPoint},
    text::{Baseline, Text},
};
pub use font::{MAX_SCALE, font_for, font_for_char, runs, scaled_text_size, text_scale, text_size};
use fusuma_render::{
    Capabilities, Color, Error, Image, PathCmd, Point, Probe, Rect, Renderer, Size, TextStyle,
    left_edge,
};
pub use vello_common::{
    color, kurbo,
    paint::{ImageSource, PaintType},
    peniko,
    pixmap::Pixmap,
};
use vello_common::{
    kurbo::{Affine, BezPath, Circle, Join, RoundedRect, Shape, Stroke},
    paint::Image as PaintImage,
    peniko::{ImageAlphaType, ImageQuality, ImageSampler},
    pixmap::PixelMetadata,
};

/// Shapes reaching farther than this many pixels from the origin (after the translation) are not
/// drawn, except plain rectangles, which are clipped exactly. Vello rasterizes in `f32`, which loses
/// all precision at huge coordinates; no surface is anywhere near this size.
const BAND: i64 = 1 << 20;

/// Surfaces are at most this many pixels on a side (Vello uses `u16`).
pub const MAX_SIDE: u32 = u16::MAX as u32;

/// How many nested `save` calls the renderer allows.
pub const STATE_DEPTH: usize = 64;

fn in_band(v: i64) -> bool {
    (-BAND..=BAND).contains(&v)
}

/// A recording drawing surface: what a Vello context can do, as seen by the shared backend.
///
/// The CPU and the GPU contexts have the same drawing methods, so implementing this is a thin layer.
/// Every method that changes the picture must make the next [`pixels`](Self::pixels) call redraw.
pub trait Scene {
    /// Width and height in pixels.
    fn size(&self) -> (u16, u16);

    /// Forgets everything drawn so far and resets the transform, the paint and the stroke.
    fn reset(&mut self);

    /// Whether the pixels survive a [`reset`](Self::reset), so the next frame can draw only what changed over them
    /// (the CPU scene). False when every frame is rendered from nothing (the GPU scene).
    fn retains_picture(&self) -> bool {
        true
    }

    /// Moves the origin: everything drawn afterwards, and clips pushed afterwards, are shifted.
    fn set_translation(&mut self, dx: f64, dy: f64);

    /// What the next shapes are painted with.
    fn set_paint(&mut self, paint: PaintType);

    /// A transform of the paint (an image or a gradient) relative to the shape, replacing any
    /// earlier one; `Affine::IDENTITY` resets it.
    fn set_paint_transform(&mut self, transform: Affine);

    /// How an image is handed to the renderer. A CPU scene takes the pixels as they are (the
    /// default); a GPU scene uploads them to a texture and returns a handle to it.
    fn image_source(&mut self, pixmap: Arc<Pixmap>) -> ImageSource {
        ImageSource::Pixmap(pixmap)
    }

    /// Fills a rectangle.
    fn fill_rect(&mut self, rect: &kurbo::Rect);

    /// Fills a path with the even-odd rule.
    fn fill_path(&mut self, path: &BezPath);

    /// Strokes a path, `width` pixels wide, with mitered joins and flat ends.
    fn stroke_path(&mut self, path: &BezPath, width: f64);

    /// Limits drawing to `rect` (in the current translated space) until the matching
    /// [`pop_clip`](Self::pop_clip).
    fn push_clip(&mut self, rect: &kurbo::Rect);

    /// Ends the innermost clip.
    fn pop_clip(&mut self);

    /// Whether this scene can draw glyph runs. Vector text is used (for measuring as well as drawing) only when
    /// it can, so the size a text was measured at is the size it is drawn at. The default cannot.
    #[cfg(feature = "text")]
    fn can_draw_glyphs(&self) -> bool {
        false
    }

    /// Fills glyphs of `font` at `size` pixels per em with the current paint, `glyphs` placed relative to
    /// `origin` (in the current translated space). Returns false when this scene cannot draw glyphs, and the
    /// caller draws the bitmap font instead. The default cannot.
    #[cfg(feature = "text")]
    fn draw_glyphs(
        &mut self,
        font: &fusuma_text::FontData,
        size: f32,
        glyphs: &[fusuma_text::Glyph],
        origin: (f64, f64),
    ) -> bool {
        let _ = (font, size, glyphs, origin);
        false
    }

    /// Says that only `region` (x, y, width, height) of the next frame matters: a frame that redraws one button does
    /// not need the rest of the surface rasterised. A scene that can render a part of itself then does only that, and
    /// [`pixels`](Self::pixels) is valid inside the region (what is outside it is left as the last frame had it, or
    /// empty if it never was drawn). The hint lasts for one frame: [`reset`](Self::reset) forgets it. The default
    /// ignores it, which is always correct, only slower.
    fn set_damage(&mut self, region: (i32, i32, u32, u32)) {
        let _ = region;
    }

    /// Adds `region` to the parts of the next frame that matter (see [`set_damage`](Self::set_damage), which only
    /// applies when nothing was added). The default ignores it.
    fn add_damage(&mut self, region: (i32, i32, u32, u32)) {
        let _ = region;
    }

    /// Moves the picture already rendered inside `region` down by `dy` rows before this frame's damage is
    /// rendered, and returns true; false when the scene cannot, and then nothing was changed. It is only valid
    /// together with [`add_damage`](Self::add_damage) for the parts that moved in, so the default cannot.
    fn shift(&mut self, region: (i32, i32, u32, u32), dy: i32) -> bool {
        let _ = (region, dy);
        false
    }

    /// The whole surface as premultiplied RGBA, 4 bytes per pixel, row by row, rendering first if
    /// anything changed since the last call.
    fn pixels(&mut self) -> &[u8];
}

#[derive(Clone, Copy, Debug)]
struct State {
    /// The visible area in device space.
    clip: Rect,
    tx: i32,
    ty: i32,
    /// How many clips this state has pushed on the scene.
    pushed: u32,
}

#[derive(Debug)]
struct Inner<S> {
    scene: S,
    size: Size,
    state: State,
    stack: Vec<State>,
    /// Vector fonts, set up on first use.
    #[cfg(feature = "text")]
    text: Option<fusuma_text::TextEngine>,
    /// Which linked family draws each font role.
    #[cfg(feature = "text")]
    fonts: fusuma_text::FontSet,
}

/// The Fusuma renderer on a Vello scene.
#[derive(Debug)]
pub struct Backend<S: Scene> {
    inner: RefCell<Inner<S>>,
}

fn kurbo_rect(r: Rect) -> kurbo::Rect {
    kurbo::Rect::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.x) + f64::from(r.w),
        f64::from(r.y) + f64::from(r.h),
    )
}

fn solid(c: Color) -> PaintType {
    PaintType::Solid(color::AlphaColor::from_rgba8(c.r, c.g, c.b, c.a))
}

fn rounded(rect: Rect, r: u32) -> BezPath {
    RoundedRect::new(
        f64::from(rect.x),
        f64::from(rect.y),
        f64::from(rect.x) + f64::from(rect.w),
        f64::from(rect.y) + f64::from(rect.h),
        f64::from(r),
    )
    .to_path(0.1)
}

/// Converts a path. A path that draws before its first `MoveTo`, or after a `Close`, starts at the
/// point where the last one ended (the origin at first), as in the integer backend; `kurbo`
/// panics on such a path, so the start is made explicit.
fn to_path(path: &[PathCmd], shift: f64) -> BezPath {
    let f = |p: Point| kurbo::Point::new(f64::from(p.x) + shift, f64::from(p.y) + shift);
    let mut b = BezPath::new();
    let mut cur = f(Point::new(0, 0));
    let mut start = cur;
    let mut open = false;
    for cmd in path {
        if !open && !matches!(cmd, PathCmd::MoveTo(_) | PathCmd::Close) {
            b.move_to(cur);
            open = true;
        }
        match *cmd {
            PathCmd::MoveTo(p) => {
                cur = f(p);
                start = cur;
                b.move_to(cur);
                open = true;
            }
            PathCmd::LineTo(p) => {
                cur = f(p);
                b.line_to(cur);
            }
            PathCmd::QuadTo(c, p) => {
                cur = f(p);
                b.quad_to(f(c), cur);
            }
            PathCmd::CubicTo(c1, c2, p) => {
                cur = f(p);
                b.curve_to(f(c1), f(c2), cur);
            }
            PathCmd::Close => {
                if open {
                    b.close_path();
                }
                cur = start;
                open = false;
            }
        }
    }
    b
}

impl<S: Scene> Backend<S> {
    /// Draws on `scene`.
    pub fn new(scene: S) -> Self {
        let (w, h) = scene.size();
        let size = Size::new(u32::from(w), u32::from(h));
        let state = State {
            clip: Rect::new(0, 0, size.w, size.h),
            tx: 0,
            ty: 0,
            pushed: 0,
        };
        Self {
            inner: RefCell::new(Inner {
                scene,
                size,
                state,
                stack: Vec::new(),
                #[cfg(feature = "text")]
                text: None,
                #[cfg(feature = "text")]
                fonts: fusuma_text::FontSet::default_set(),
            }),
        }
    }

    /// Draws the font roles with `set`'s families from now on; a role without one is the bitmap font.
    #[cfg(feature = "text")]
    pub fn set_fonts(&mut self, set: fusuma_text::FontSet) {
        let i = self.inner.get_mut();
        i.fonts = set;
        if let Some(engine) = i.text.as_mut() {
            engine.set_fonts(set);
        }
    }

    /// Runs `f` on the scene, for what is specific to it: a PNG, a GPU texture, a render setting.
    pub fn with_scene<T>(&self, f: impl FnOnce(&mut S) -> T) -> T {
        f(&mut self.inner.borrow_mut().scene)
    }

    /// Gives the scene back.
    pub fn into_scene(self) -> S {
        self.inner.into_inner().scene
    }
}

impl<S: Scene> Inner<S> {
    fn sync_transform(&mut self) {
        self.scene
            .set_translation(f64::from(self.state.tx), f64::from(self.state.ty));
    }

    fn hidden(&self) -> bool {
        self.state.clip.is_empty()
    }

    /// The text engine, made on first use.
    #[cfg(feature = "text")]
    fn engine(&mut self) -> &mut fusuma_text::TextEngine {
        let fonts = self.fonts;
        self.text.get_or_insert_with(|| {
            let mut engine = fusuma_text::TextEngine::new();
            engine.set_fonts(fonts);
            engine
        })
    }

    /// The size of `text` in a vector font, if the style asks for one that is bundled.
    #[cfg(feature = "text")]
    fn measure_vector_text(&mut self, text: &str, style: &TextStyle) -> Option<Size> {
        if !self.scene.can_draw_glyphs() {
            return None;
        }
        self.engine().measure(text, style.font, style.size)
    }

    /// Draws `text` with a vector font when the style asks for one that is bundled and the scene can draw
    /// glyphs; false means the caller draws the bitmap font.
    #[cfg(feature = "text")]
    fn draw_vector_text(&mut self, text: &str, at: Point, style: &TextStyle) -> bool {
        // Glyph positions are `f32`, which keeps whole pixels only up to about 16 million.
        let far = |v: i32| i64::from(v).abs() > 1 << 22;
        if !self.scene.can_draw_glyphs() {
            return false;
        }
        let Some(laid) = self.engine().layout(text, style.font, style.size).cloned() else {
            return false;
        };
        let left = left_edge(at.x, laid.size.w, style.align);
        if far(left) || far(at.y) {
            return true;
        }
        self.scene.set_paint(solid(style.color));
        let origin = (f64::from(left), f64::from(at.y));
        laid.runs
            .iter()
            .all(|r| self.scene.draw_glyphs(&r.font, r.size, &r.glyphs, origin))
    }

    /// Whether a local point, once translated, is inside the safe band.
    fn point_ok(&self, x: i64, y: i64) -> bool {
        in_band(x + i64::from(self.state.tx)) && in_band(y + i64::from(self.state.ty))
    }

    fn rect_ok(&self, r: Rect) -> bool {
        self.point_ok(i64::from(r.x), i64::from(r.y))
            && self.point_ok(i64::from(r.right()), i64::from(r.bottom()))
    }

    fn path_ok(&self, path: &[PathCmd]) -> bool {
        path.iter().all(|cmd| {
            let ok = |p: &Point| self.point_ok(i64::from(p.x), i64::from(p.y));
            match cmd {
                PathCmd::MoveTo(a) | PathCmd::LineTo(a) => ok(a),
                PathCmd::QuadTo(a, b) => ok(a) && ok(b),
                PathCmd::CubicTo(a, b, c) => ok(a) && ok(b) && ok(c),
                PathCmd::Close => true,
            }
        })
    }

    /// The part of `rect` (local space) that is visible, in local space.
    fn visible(&self, rect: Rect) -> Rect {
        let local = self.state.clip.translate(
            self.state.tx.saturating_neg(),
            self.state.ty.saturating_neg(),
        );
        rect.intersect(&local)
    }

    fn fill_solid_rect(&mut self, rect: Rect, color: Color) {
        let visible = self.visible(rect);
        if visible.is_empty() {
            return;
        }
        self.scene.set_paint(solid(color));
        self.scene.fill_rect(&kurbo_rect(visible));
    }
}

type Res = Result<(), Error<Infallible>>;

impl<S: Scene> Renderer for Backend<S> {
    type Error = Error<Infallible>;

    fn size(&self) -> Size {
        self.inner.borrow().size
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            alpha: true,
            antialiasing: true,
            effects: true,
            retains_picture: self.inner.borrow().scene.retains_picture(),
        }
    }

    fn save(&mut self) -> Res {
        let i = self.inner.get_mut();
        if i.stack.len() >= STATE_DEPTH {
            return Err(Error::StackFull);
        }
        i.stack.push(i.state);
        Ok(())
    }

    fn restore(&mut self) -> Res {
        let i = self.inner.get_mut();
        let saved = i.stack.pop().ok_or(Error::StackEmpty)?;
        for _ in saved.pushed..i.state.pushed {
            i.scene.pop_clip();
        }
        i.state = saved;
        i.sync_transform();
        Ok(())
    }

    fn clip(&mut self, rect: Rect) -> Res {
        let i = self.inner.get_mut();
        let device = rect.translate(i.state.tx, i.state.ty);
        i.state.clip = i.state.clip.intersect(&device);
        // An empty rectangle hides everything and needs no clip on the scene.
        if !rect.is_empty() && i.rect_ok(rect) {
            i.scene.push_clip(&kurbo_rect(rect));
            i.state.pushed += 1;
        } else {
            i.state.clip = Rect::new(i.state.clip.x, i.state.clip.y, 0, 0);
        }
        Ok(())
    }

    fn shift_area(&mut self, rect: Rect, dy: i32) -> bool {
        let i = self.inner.get_mut();
        let device = rect.translate(i.state.tx, i.state.ty);
        // Only a rectangle wholly inside the surface can be shifted as asked.
        let surface = Rect::new(0, 0, i.size.w, i.size.h);
        if device.is_empty() || device.intersect(&surface) != device {
            return false;
        }
        i.scene.shift((device.x, device.y, device.w, device.h), dy)
    }

    fn will_draw(&mut self, rect: Rect) {
        let i = self.inner.get_mut();
        let device = rect
            .translate(i.state.tx, i.state.ty)
            .intersect(&Rect::new(0, 0, i.size.w, i.size.h));
        i.scene.add_damage((device.x, device.y, device.w, device.h));
    }

    fn translate(&mut self, dx: i32, dy: i32) -> Res {
        let i = self.inner.get_mut();
        i.state.tx = i.state.tx.saturating_add(dx);
        i.state.ty = i.state.ty.saturating_add(dy);
        i.sync_transform();
        Ok(())
    }

    fn clear(&mut self, color: Color) -> Res {
        let i = self.inner.get_mut();
        if i.hidden() {
            return Ok(());
        }
        let full = Rect::new(0, 0, i.size.w, i.size.h);
        // Nothing before this can show any more, so the scene can forget it and stay small.
        if i.stack.is_empty() && i.state.pushed == 0 && i.state.clip == full {
            i.scene.reset();
            i.sync_transform();
        }
        let local = full.translate(i.state.tx.saturating_neg(), i.state.ty.saturating_neg());
        i.fill_solid_rect(local, color.with_alpha(255));
        Ok(())
    }

    fn fill_rect(&mut self, rect: Rect, radius: u32, color: Color) -> Res {
        let i = self.inner.get_mut();
        if rect.is_empty() || color.a == 0 || i.hidden() {
            return Ok(());
        }
        let r = radius.min(rect.w / 2).min(rect.h / 2);
        if r == 0 {
            i.fill_solid_rect(rect, color);
        } else if i.rect_ok(rect) {
            i.scene.set_paint(solid(color));
            i.scene.fill_path(&rounded(rect, r));
        }
        Ok(())
    }

    fn fill_gradient(&mut self, rect: Rect, radius: u32, g: fusuma_render::Gradient) -> Res {
        let i = self.inner.get_mut();
        if rect.is_empty() || i.hidden() || !i.rect_ok(rect) {
            return Ok(());
        }
        let (x0, y0) = (f64::from(rect.x), f64::from(rect.y));
        let (x1, y1) = (x0 + f64::from(rect.w), y0 + f64::from(rect.h));
        let (start, end) = match g.direction {
            fusuma_render::Direction::Vertical => ((x0, y0), (x0, y1)),
            fusuma_render::Direction::Horizontal => ((x0, y0), (x1, y0)),
        };
        let stop = |c: Color| color::AlphaColor::from_rgba8(c.r, c.g, c.b, c.a);
        let gradient =
            peniko::Gradient::new_linear(start, end).with_stops([stop(g.from), stop(g.to)]);
        i.scene.set_paint(PaintType::Gradient(gradient));
        let r = radius.min(rect.w / 2).min(rect.h / 2);
        if r == 0 {
            i.scene.fill_rect(&kurbo::Rect::new(x0, y0, x1, y1));
        } else {
            i.scene.fill_path(&rounded(rect, r));
        }
        Ok(())
    }

    fn stroke_rect(&mut self, rect: Rect, radius: u32, width: u32, color: Color) -> Res {
        let i = self.inner.get_mut();
        if rect.is_empty() || width == 0 || color.a == 0 || i.hidden() || !i.rect_ok(rect) {
            return Ok(());
        }
        if width.saturating_mul(2) >= rect.w.min(rect.h) {
            return self.fill_rect(rect, radius, color);
        }
        let i = self.inner.get_mut();
        let half = f64::from(width) / 2.0;
        let r = (f64::from(radius.min(rect.w / 2).min(rect.h / 2)) - half).max(0.0);
        let (x, y) = (f64::from(rect.x) + half, f64::from(rect.y) + half);
        let (w, h) = (
            f64::from(rect.w) - f64::from(width),
            f64::from(rect.h) - f64::from(width),
        );
        let path = if r > 0.0 {
            RoundedRect::new(x, y, x + w, y + h, r).to_path(0.1)
        } else {
            kurbo::Rect::new(x, y, x + w, y + h).to_path(0.1)
        };
        i.scene.set_paint(solid(color));
        i.scene.stroke_path(&path, f64::from(width));
        Ok(())
    }

    fn fill_circle(&mut self, center: Point, radius: u32, color: Color) -> Res {
        let i = self.inner.get_mut();
        let near = i.point_ok(i64::from(center.x), i64::from(center.y));
        if color.a == 0 || i.hidden() || !near || i64::from(radius) > BAND {
            return Ok(());
        }
        // The pixel at `center` is the middle of the disc.
        let c = kurbo::Point::new(f64::from(center.x) + 0.5, f64::from(center.y) + 0.5);
        i.scene.set_paint(solid(color));
        i.scene
            .fill_path(&Circle::new(c, f64::from(radius) + 0.5).to_path(0.1));
        Ok(())
    }

    fn fill_path(&mut self, path: &[PathCmd], color: Color) -> Res {
        let i = self.inner.get_mut();
        if color.a == 0 || i.hidden() || !i.path_ok(path) {
            return Ok(());
        }
        i.scene.set_paint(solid(color));
        i.scene.fill_path(&to_path(path, 0.0));
        Ok(())
    }

    fn stroke_path(&mut self, path: &[PathCmd], width: u32, color: Color) -> Res {
        let i = self.inner.get_mut();
        if width == 0 || color.a == 0 || i.hidden() || i64::from(width) > BAND || !i.path_ok(path) {
            return Ok(());
        }
        // Through pixel centers, so a line at an integer coordinate is crisp.
        i.scene.set_paint(solid(color));
        i.scene.stroke_path(&to_path(path, 0.5), f64::from(width));
        Ok(())
    }

    fn draw_text(&mut self, text: &str, at: Point, style: &TextStyle) -> Res {
        let i = self.inner.get_mut();
        if text.is_empty() || style.color.a == 0 || i.hidden() {
            return Ok(());
        }
        #[cfg(feature = "text")]
        if i.draw_vector_text(text, at, style) {
            return Ok(());
        }
        let font = font_for(style.size);
        let k = text_scale(style.size);
        let left = left_edge(at.x, scaled_text_size(style.size, text).w, style.align);
        // `embedded-graphics` lays glyphs out in `i32` and overflows near its ends.
        let far = |v: i32| i64::from(v).abs() > 1 << 24;
        if far(left) || far(at.y) {
            return Ok(());
        }
        // The built-in bitmap fonts are drawn as horizontal runs of lit pixels, laid out at the origin and
        // then magnified by a whole number and moved to place.
        let mut lit = Runs::default();
        let advance =
            i32::try_from(font.character_size.width + font.character_spacing).unwrap_or(i32::MAX);
        for run in font::runs(text, style.size) {
            let x = advance.saturating_mul(i32::try_from(run.before).unwrap_or(i32::MAX));
            let mono = MonoTextStyle::new(run.font, Rgb888::new(255, 255, 255));
            let _ = Text::with_baseline(run.text, EgPoint::new(x, 0), mono, Baseline::Top)
                .draw(&mut lit);
        }
        lit.finish();
        i.scene.set_paint(solid(style.color));
        let (kf, ox, oy) = (f64::from(k), f64::from(left), f64::from(at.y));
        for run in lit.done {
            i.scene.fill_rect(&kurbo::Rect::new(
                ox + run.x0 * kf,
                oy + run.y0 * kf,
                ox + run.x1 * kf,
                oy + run.y1 * kf,
            ));
        }
        Ok(())
    }

    fn measure_text(&self, text: &str, style: &TextStyle) -> Size {
        #[cfg(feature = "text")]
        if let Some(s) = self.inner.borrow_mut().measure_vector_text(text, style) {
            return s;
        }
        scaled_text_size(style.size, text)
    }

    fn draw_image(&mut self, image: &Image<'_>, at: Point) -> Res {
        let i = self.inner.get_mut();
        let (w, h) = (image.width(), image.height());
        // `at` on its own must be small too: the rasterizer adds the image size to it.
        let near = in_band(i64::from(at.x)) && in_band(i64::from(at.y));
        let (Ok(pw), Ok(ph)) = (u16::try_from(w), u16::try_from(h)) else {
            return Ok(());
        };
        if i.hidden() || w == 0 || h == 0 || !near || !i.point_ok(i64::from(at.x), i64::from(at.y))
        {
            return Ok(());
        }
        let pixmap = Pixmap::from_parts(
            image.rgba().to_vec(),
            pw,
            ph,
            PixelMetadata {
                may_have_transparency: true,
                alpha_type: ImageAlphaType::Alpha,
            },
        );
        let paint = PaintImage {
            image: i.scene.image_source(Arc::new(pixmap)),
            // Nearest neighbor: an image is drawn pixel for pixel.
            sampler: ImageSampler {
                quality: ImageQuality::Low,
                ..ImageSampler::default()
            },
        };
        let (x, y) = (f64::from(at.x), f64::from(at.y));
        i.scene.set_paint(PaintType::Image(paint));
        i.scene.set_paint_transform(Affine::translate((x, y)));
        i.scene
            .fill_rect(&kurbo::Rect::new(x, y, x + f64::from(w), y + f64::from(h)));
        i.scene.set_paint_transform(Affine::IDENTITY);
        Ok(())
    }
}

impl<S: Scene> Probe for Backend<S> {
    fn pixel(&self, x: i32, y: i32) -> Option<Color> {
        let mut i = self.inner.borrow_mut();
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        if x >= i.size.w || y >= i.size.h {
            return None;
        }
        let at = (y as usize * i.size.w as usize + x as usize) * 4;
        let px: [u8; 4] = i.scene.pixels().get(at..at + 4)?.try_into().ok()?;
        let [r, g, b, a] = px;
        if a == 0 {
            return Some(Color::rgba(0, 0, 0, 0));
        }
        // Premultiplied to straight, rounding to nearest.
        let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
        Some(Color::rgba(un(r), un(g), un(b), a))
    }
}

/// Collects the lit pixels of a bitmap glyph run as horizontal rectangles.
#[derive(Default)]
struct Runs {
    done: Vec<kurbo::Rect>,
    open: Option<(i32, i32, i32)>,
}

impl Runs {
    fn finish(&mut self) {
        if let Some((y, x0, x1)) = self.open.take() {
            self.done.push(kurbo::Rect::new(
                f64::from(x0),
                f64::from(y),
                f64::from(x1),
                f64::from(y + 1),
            ));
        }
    }
}

impl OriginDimensions for Runs {
    fn size(&self) -> EgSize {
        EgSize::new(u32::MAX >> 2, u32::MAX >> 2)
    }
}

impl DrawTarget for Runs {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb888>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(p, _) in pixels {
            match self.open {
                Some((y, x0, x1)) if y == p.y && x1 == p.x => self.open = Some((y, x0, x1 + 1)),
                _ => {
                    self.finish();
                    self.open = Some((p.y, p.x, p.x + 1));
                }
            }
        }
        Ok(())
    }
}

/// A plain stroke style for scenes that need one: flat ends and mitered joins.
#[must_use]
pub fn stroke(width: f64) -> Stroke {
    Stroke::new(width).with_join(Join::Miter)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scene that only records what it was told, to test the mapping without a rasterizer.
    #[derive(Default, Debug)]
    struct Log {
        ops: Vec<String>,
        px: Vec<u8>,
    }
    impl Scene for Log {
        fn size(&self) -> (u16, u16) {
            (32, 32)
        }
        fn reset(&mut self) {
            self.ops.push("reset".into());
        }
        fn set_translation(&mut self, dx: f64, dy: f64) {
            self.ops.push(format!("move {dx} {dy}"));
        }
        fn set_paint(&mut self, _: PaintType) {
            self.ops.push("paint".into());
        }
        fn set_paint_transform(&mut self, _: Affine) {
            self.ops.push("paint-transform".into());
        }
        fn fill_rect(&mut self, r: &kurbo::Rect) {
            self.ops
                .push(format!("rect {} {} {} {}", r.x0, r.y0, r.x1, r.y1));
        }
        fn fill_path(&mut self, _: &BezPath) {
            self.ops.push("fill-path".into());
        }
        fn stroke_path(&mut self, _: &BezPath, w: f64) {
            self.ops.push(format!("stroke {w}"));
        }
        fn push_clip(&mut self, _: &kurbo::Rect) {
            self.ops.push("push-clip".into());
        }
        fn pop_clip(&mut self) {
            self.ops.push("pop-clip".into());
        }
        fn pixels(&mut self) -> &[u8] {
            self.px = vec![0; 32 * 32 * 4];
            &self.px
        }
    }

    fn backend() -> Backend<Log> {
        Backend::new(Log::default())
    }

    fn ops(b: &Backend<Log>) -> Vec<String> {
        b.with_scene(|s| s.ops.clone())
    }

    #[test]
    fn every_clip_is_popped_by_its_restore_and_only_those() {
        let mut b = backend();
        b.clip(Rect::new(0, 0, 20, 20)).unwrap();
        b.save().unwrap();
        b.clip(Rect::new(2, 2, 5, 5)).unwrap();
        b.clip(Rect::new(3, 3, 1, 1)).unwrap();
        b.restore().unwrap();
        let pops = ops(&b).iter().filter(|o| *o == "pop-clip").count();
        let pushes = ops(&b).iter().filter(|o| *o == "push-clip").count();
        assert_eq!((pushes, pops), (3, 2));
    }

    #[test]
    fn an_empty_clip_hides_everything_without_touching_the_scene() {
        let mut b = backend();
        b.clip(Rect::new(5, 5, 0, 9)).unwrap();
        b.fill_rect(Rect::new(0, 0, 30, 30), 0, Color::WHITE)
            .unwrap();
        b.fill_circle(Point::new(10, 10), 5, Color::WHITE).unwrap();
        b.draw_text("x", Point::new(0, 0), &TextStyle::new(10, Color::WHITE))
            .unwrap();
        assert!(
            !ops(&b)
                .iter()
                .any(|o| o.starts_with("rect") || o == "fill-path")
        );
    }

    #[test]
    fn a_plain_rect_is_cut_to_the_clip_before_it_reaches_the_scene() {
        let mut b = backend();
        b.save().unwrap();
        b.translate(10, 10).unwrap();
        b.clip(Rect::new(0, 0, 5, 5)).unwrap();
        b.fill_rect(Rect::new(-100, -100, 500, 500), 0, Color::WHITE)
            .unwrap();
        assert_eq!(ops(&b).last().unwrap(), "rect 0 0 5 5");
    }

    #[test]
    fn clearing_the_whole_surface_forgets_the_scene_but_not_inside_a_clip() {
        let mut b = backend();
        b.clear(Color::BLACK).unwrap();
        assert!(ops(&b).contains(&"reset".to_owned()));
        let mut b = backend();
        b.save().unwrap();
        b.clear(Color::BLACK).unwrap();
        assert!(
            !ops(&b).contains(&"reset".to_owned()),
            "inside a save the scene may be needed"
        );
        let mut b = backend();
        b.clip(Rect::new(0, 0, 10, 10)).unwrap();
        b.clear(Color::BLACK).unwrap();
        assert!(!ops(&b).contains(&"reset".to_owned()));
    }

    #[test]
    fn the_translation_follows_translate_and_restore() {
        let mut b = backend();
        b.save().unwrap();
        b.translate(4, 5).unwrap();
        b.translate(1, 1).unwrap();
        b.restore().unwrap();
        assert_eq!(ops(&b), ["move 4 5", "move 5 6", "move 0 0"]);
    }

    #[test]
    fn the_save_stack_is_bounded() {
        let mut b = backend();
        for _ in 0..STATE_DEPTH {
            b.save().unwrap();
        }
        assert_eq!(b.save(), Err(Error::StackFull));
        for _ in 0..STATE_DEPTH {
            b.restore().unwrap();
        }
        assert_eq!(b.restore(), Err(Error::StackEmpty));
    }

    #[test]
    fn text_becomes_runs_of_lit_pixels_not_one_rect_per_pixel() {
        let mut b = backend();
        b.draw_text("Hi", Point::new(0, 0), &TextStyle::new(10, Color::WHITE))
            .unwrap();
        let rects = ops(&b).iter().filter(|o| o.starts_with("rect")).count();
        assert!(rects > 0 && rects < 6 * 10 * 2, "{rects}");
    }

    #[test]
    fn premultiplied_pixels_are_returned_as_straight_colors() {
        struct One;
        impl Scene for One {
            fn size(&self) -> (u16, u16) {
                (1, 1)
            }
            fn reset(&mut self) {}
            fn set_translation(&mut self, _: f64, _: f64) {}
            fn set_paint(&mut self, _: PaintType) {}
            fn set_paint_transform(&mut self, _: Affine) {}
            fn fill_rect(&mut self, _: &kurbo::Rect) {}
            fn fill_path(&mut self, _: &BezPath) {}
            fn stroke_path(&mut self, _: &BezPath, _: f64) {}
            fn push_clip(&mut self, _: &kurbo::Rect) {}
            fn pop_clip(&mut self) {}
            fn pixels(&mut self) -> &[u8] {
                &[64, 32, 0, 128]
            }
        }
        let b = Backend::new(One);
        assert_eq!(b.pixel(0, 0), Some(Color::rgba(128, 64, 0, 128)));
        assert_eq!(
            (b.pixel(1, 0), b.pixel(0, 1), b.pixel(-1, 0)),
            (None, None, None)
        );
    }

    #[test]
    fn regression_a_path_without_a_move_to_does_not_panic_kurbo() {
        // Found by the draw fuzz target: kurbo panics on "uninitialized subpath".
        for path in [
            vec![PathCmd::LineTo(Point::new(5, 5))],
            vec![PathCmd::Close],
            vec![
                PathCmd::QuadTo(Point::new(1, 1), Point::new(2, 2)),
                PathCmd::Close,
                PathCmd::LineTo(Point::new(3, 3)),
            ],
            vec![
                PathCmd::MoveTo(Point::new(1, 1)),
                PathCmd::Close,
                PathCmd::CubicTo(Point::new(0, 0), Point::new(1, 1), Point::new(2, 2)),
            ],
            vec![],
        ] {
            let _ = to_path(&path, 0.5);
        }
        // Drawing before a MoveTo starts at the origin, like the integer backend.
        let p = to_path(&[PathCmd::LineTo(Point::new(4, 0))], 0.0);
        assert_eq!(p.elements().len(), 2);
    }

    #[test]
    fn the_stroke_helper_is_mitered() {
        let s = stroke(3.0);
        assert_eq!(s.width, 3.0);
        assert_eq!(s.join, Join::Miter);
    }

    #[test]
    fn absurd_coordinates_do_not_reach_the_scene() {
        let mut b = backend();
        b.translate(i32::MAX, i32::MIN).unwrap();
        b.fill_circle(Point::new(i32::MAX, 0), u32::MAX, Color::WHITE)
            .unwrap();
        b.fill_rect(Rect::new(0, 0, 9, 9), 3, Color::WHITE).unwrap();
        b.stroke_rect(Rect::new(0, 0, 9, 9), 3, 2, Color::WHITE)
            .unwrap();
        assert!(
            !ops(&b)
                .iter()
                .any(|o| o == "fill-path" || o.starts_with("stroke"))
        );
    }
}
