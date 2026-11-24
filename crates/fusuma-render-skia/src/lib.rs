#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

extern crate alloc;

#[cfg(feature = "png")]
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use core::convert::Infallible;

use embedded_graphics::{
    Pixel,
    geometry::{OriginDimensions, Size as EgSize},
    mono_font::MonoTextStyle,
    pixelcolor::Rgb888,
    prelude::{DrawTarget, Drawable, Point as EgPoint},
    text::{Baseline, Text},
};
use fusuma_render::{
    Capabilities, Color, Error, Image, PathCmd, Point, Probe, Rect, Renderer, Size, TextStyle,
    left_edge,
};
use fusuma_render_embedded::{font_for, text_size};
use tiny_skia::{
    BlendMode, FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

/// Shapes reaching farther than this many pixels from the origin (after the translation) are not
/// drawn, except plain rectangles, which are clipped exactly. `tiny-skia`'s scan converter
/// asserts on coordinates far beyond any surface; this keeps a valid call from panicking.
const BAND: i64 = 1 << 20;

fn in_band(v: i64) -> bool {
    (-BAND..=BAND).contains(&v)
}

/// How many nested `save` calls the renderer allows.
pub const STATE_DEPTH: usize = 64;

/// Distance of the control points of a quarter circle, as a fraction of the radius.
const KAPPA: f32 = 0.552_284_8;

/// The PNG encoder failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(feature = "png")]
pub struct EncodeError(pub String);

/// What `save` keeps: the clip is always a rectangle (an intersection of rectangles), so it is kept as one and costs
/// no memory; the mask that tiny-skia needs is made from it only when a shape crosses it.
#[derive(Clone, Copy, Debug)]
struct State {
    /// The visible area in device space, always inside the surface.
    clip: Rect,
    tx: i32,
    ty: i32,
}

/// The pixels a draw may touch, in device space: 64-bit, because a shape can lie anywhere in the coordinate range.
#[derive(Clone, Copy, Debug)]
struct Span {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
}

/// The Fusuma renderer on a `tiny-skia` pixmap.
#[derive(Debug)]
pub struct Skia {
    pixmap: Pixmap,
    state: State,
    stack: Vec<State>,
    /// One mask for the whole surface (1 byte a pixel), made the first time a shape crosses the clip and reused:
    /// it selects `mask_clip`, and is rewritten only when a crossing shape needs another clip. Shapes inside the
    /// clip, plain rectangles and text need no mask at all.
    mask: Option<Mask>,
    mask_clip: Rect,
}

fn paint(c: Color, anti_alias: bool) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, c.a);
    p.anti_alias = anti_alias;
    p
}

fn skia_rect(x: f32, y: f32, w: f32, h: f32) -> Option<tiny_skia::Rect> {
    tiny_skia::Rect::from_xywh(x, y, w, h)
}

fn rect_f(r: Rect) -> Option<tiny_skia::Rect> {
    skia_rect(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
}

/// A rectangle with round corners of `r` pixels (`r` is at most half of each side).
fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let k = r * KAPPA;
    let mut b = PathBuilder::new();
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    b.close();
    b.finish()
}

fn to_path(path: &[PathCmd], shift: f32) -> Option<tiny_skia::Path> {
    let f = |p: Point| (p.x as f32 + shift, p.y as f32 + shift);
    let mut b = PathBuilder::new();
    for cmd in path {
        match *cmd {
            PathCmd::MoveTo(p) => {
                let (x, y) = f(p);
                b.move_to(x, y);
            }
            PathCmd::LineTo(p) => {
                let (x, y) = f(p);
                b.line_to(x, y);
            }
            PathCmd::QuadTo(c, p) => {
                let ((cx, cy), (x, y)) = (f(c), f(p));
                b.quad_to(cx, cy, x, y);
            }
            PathCmd::CubicTo(c1, c2, p) => {
                let ((ax, ay), (bx, by), (x, y)) = (f(c1), f(c2), f(p));
                b.cubic_to(ax, ay, bx, by, x, y);
            }
            PathCmd::Close => b.close(),
        }
    }
    b.finish()
}

impl Skia {
    /// A transparent surface of `width` x `height` pixels; `None` when the size is zero or too big.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let pixmap = Pixmap::new(width, height)?;
        let state = State {
            clip: Rect::new(0, 0, width, height),
            tx: 0,
            ty: 0,
        };
        Some(Self {
            pixmap,
            state,
            stack: Vec::new(),
            mask: None,
            mask_clip: Rect::default(),
        })
    }

    /// The pixels (premultiplied RGBA).
    #[must_use]
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    /// The pixels, mutable.
    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }

    /// Gives the pixmap back.
    #[must_use]
    pub fn into_pixmap(self) -> Pixmap {
        self.pixmap
    }

    /// The surface as a PNG file.
    ///
    /// # Errors
    /// [`EncodeError`] with the encoder's message.
    #[cfg(feature = "png")]
    pub fn png(&self) -> core::result::Result<Vec<u8>, EncodeError> {
        self.pixmap
            .encode_png()
            .map_err(|e| EncodeError(e.to_string()))
    }

    /// Whether a local point, once translated, is inside the safe band.
    fn point_ok(&self, x: i64, y: i64) -> bool {
        in_band(x + i64::from(self.state.tx)) && in_band(y + i64::from(self.state.ty))
    }

    /// Whether a local rectangle, once translated, is inside the safe band.
    fn rect_ok(&self, r: Rect) -> bool {
        self.point_ok(i64::from(r.x), i64::from(r.y))
            && self.point_ok(i64::from(r.right()), i64::from(r.bottom()))
    }

    /// Whether every point of `path`, once translated, is inside the safe band.
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

    fn transform(&self) -> Transform {
        Transform::from_translate(self.state.tx as f32, self.state.ty as f32)
    }

    /// The device pixels of the local box `(x0, y0)..(x1, y1)`, grown by `pad` on every side (anti-aliasing and
    /// stroke width).
    fn span(&self, (x0, y0): (i64, i64), (x1, y1): (i64, i64), pad: i64) -> Span {
        let (tx, ty) = (i64::from(self.state.tx), i64::from(self.state.ty));
        Span {
            x0: x0 + tx - pad,
            y0: y0 + ty - pad,
            x1: x1 + tx + pad,
            y1: y1 + ty + pad,
        }
    }

    /// The device pixels a rectangle covers, grown by `pad`.
    fn rect_span(&self, r: Rect, pad: i64) -> Span {
        self.span(
            (i64::from(r.x), i64::from(r.y)),
            (
                i64::from(r.x) + i64::from(r.w),
                i64::from(r.y) + i64::from(r.h),
            ),
            pad,
        )
    }

    /// The device pixels a path can touch: a curve stays inside the hull of its points. An empty path touches none.
    fn path_span(&self, path: &[PathCmd], pad: i64) -> Span {
        let (mut lo, mut hi) = ((i64::MAX, i64::MAX), (i64::MIN, i64::MIN));
        let mut add = |p: &Point| {
            let (x, y) = (i64::from(p.x), i64::from(p.y));
            lo = (lo.0.min(x), lo.1.min(y));
            hi = (hi.0.max(x), hi.1.max(y));
        };
        for cmd in path {
            match cmd {
                PathCmd::MoveTo(a) | PathCmd::LineTo(a) => add(a),
                PathCmd::QuadTo(a, b) => [a, b].into_iter().for_each(&mut add),
                PathCmd::CubicTo(a, b, c) => [a, b, c].into_iter().for_each(&mut add),
                PathCmd::Close => {}
            }
        }
        if lo.0 > hi.0 {
            return Span {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            };
        }
        self.span(lo, (hi.0 + 1, hi.1 + 1), pad)
    }

    /// What a draw that touches `span` needs: `None` when nothing of it can be seen (skip it), `Some(false)` when it
    /// lies inside the clip (no mask), `Some(true)` when it crosses the clip's edge, and then the mask selects the
    /// clip.
    fn prepare(&mut self, s: Span) -> Option<bool> {
        let c = self.state.clip;
        let (cx0, cy0) = (i64::from(c.x), i64::from(c.y));
        let (cx1, cy1) = (cx0 + i64::from(c.w), cy0 + i64::from(c.h));
        if c.is_empty() || s.x1 <= cx0 || s.y1 <= cy0 || s.x0 >= cx1 || s.y0 >= cy1 {
            return None;
        }
        if s.x0 >= cx0 && s.y0 >= cy0 && s.x1 <= cx1 && s.y1 <= cy1 {
            return Some(false);
        }
        self.select(c);
        Some(true)
    }

    /// Makes the mask select `clip` (inside the surface), making the mask the first time. Only the rows of the old
    /// and the new rectangle are written, not the whole surface.
    fn select(&mut self, clip: Rect) {
        let w = self.pixmap.width();
        if self.mask.is_none() {
            self.mask = Mask::new(w, self.pixmap.height());
            self.mask_clip = Rect::default();
        }
        if self.mask_clip == clip {
            return;
        }
        let Some(mask) = self.mask.as_mut() else {
            return;
        };
        let stride = w as usize;
        let data = mask.data_mut();
        let mut paint = |r: Rect, v: u8| {
            if r.is_empty() {
                return;
            }
            let (x, y) = (r.x.max(0) as usize, r.y.max(0) as usize);
            for row in y..y + r.h as usize {
                let at = row * stride + x;
                if let Some(px) = data.get_mut(at..at + r.w as usize) {
                    px.fill(v);
                }
            }
        };
        paint(self.mask_clip, 0);
        paint(clip, 255);
        self.mask_clip = clip;
    }

    /// The number of masks made so far (0 or 1), for the tests.
    #[cfg(test)]
    fn masks(&self) -> usize {
        usize::from(self.mask.is_some())
    }
}

type Result<T = ()> = core::result::Result<T, Error<Infallible>>;

impl Renderer for Skia {
    type Error = Error<Infallible>;

    fn size(&self) -> Size {
        Size::new(self.pixmap.width(), self.pixmap.height())
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            alpha: true,
            antialiasing: true,
        }
    }

    fn save(&mut self) -> Result {
        if self.stack.len() >= STATE_DEPTH {
            return Err(Error::StackFull);
        }
        self.stack.push(self.state);
        Ok(())
    }

    fn restore(&mut self) -> Result {
        self.state = self.stack.pop().ok_or(Error::StackEmpty)?;
        Ok(())
    }

    fn clip(&mut self, rect: Rect) -> Result {
        // Only the rectangle: a mask is made from it when a shape crosses it (see `prepare`). An empty clip hides
        // everything.
        let device = rect.translate(self.state.tx, self.state.ty);
        self.state.clip = self.state.clip.intersect(&device);
        Ok(())
    }

    fn translate(&mut self, dx: i32, dy: i32) -> Result {
        self.state.tx = self.state.tx.saturating_add(dx);
        self.state.ty = self.state.ty.saturating_add(dy);
        Ok(())
    }

    fn clear(&mut self, color: Color) -> Result {
        let mut p = paint(color.with_alpha(255), false);
        p.blend_mode = BlendMode::Source;
        // The clip is a rectangle in whole pixels: filling exactly it is the clear, with no mask.
        let c = self.state.clip;
        if let Some(all) = rect_f(c).filter(|_| !c.is_empty()) {
            self.pixmap.fill_rect(all, &p, Transform::identity(), None);
        }
        Ok(())
    }

    fn fill_rect(&mut self, rect: Rect, radius: u32, color: Color) -> Result {
        if rect.is_empty() || color.a == 0 {
            return Ok(());
        }
        let r = radius.min(rect.w / 2).min(rect.h / 2);
        if r == 0 {
            // A plain rectangle is clipped exactly, whatever its coordinates.
            let visible = rect
                .translate(self.state.tx, self.state.ty)
                .intersect(&self.state.clip);
            // Already cut to the clip, so no mask.
            if let Some(shape) = rect_f(visible).filter(|_| !visible.is_empty()) {
                self.pixmap
                    .fill_rect(shape, &paint(color, false), Transform::identity(), None);
            }
        } else if self.rect_ok(rect) {
            let Some(masked) = self.prepare(self.rect_span(rect, 1)) else {
                return Ok(());
            };
            let t = self.transform();
            if let Some(path) = rounded_rect(
                rect.x as f32,
                rect.y as f32,
                rect.w as f32,
                rect.h as f32,
                r as f32,
            ) {
                let mask = if masked { self.mask.as_ref() } else { None };
                self.pixmap
                    .fill_path(&path, &paint(color, true), FillRule::Winding, t, mask);
            }
        }
        Ok(())
    }

    fn stroke_rect(&mut self, rect: Rect, radius: u32, width: u32, color: Color) -> Result {
        if rect.is_empty() || width == 0 || color.a == 0 {
            return Ok(());
        }
        if !self.rect_ok(rect) {
            return Ok(());
        }
        if width.saturating_mul(2) >= rect.w.min(rect.h) {
            return self.fill_rect(rect, radius, color);
        }
        // The stroke lies inside the rectangle.
        let Some(masked) = self.prepare(self.rect_span(rect, 1)) else {
            return Ok(());
        };
        let half = width as f32 / 2.0;
        let r = (radius.min(rect.w / 2).min(rect.h / 2) as f32 - half).max(0.0);
        let (x, y, w, h) = (
            rect.x as f32 + half,
            rect.y as f32 + half,
            rect.w as f32 - width as f32,
            rect.h as f32 - width as f32,
        );
        let path = if r > 0.0 {
            rounded_rect(x, y, w, h, r)
        } else {
            skia_rect(x, y, w, h).map(PathBuilder::from_rect)
        };
        if let Some(path) = path {
            let stroke = Stroke {
                width: width as f32,
                ..Stroke::default()
            };
            let t = self.transform();
            let mask = if masked { self.mask.as_ref() } else { None };
            self.pixmap
                .stroke_path(&path, &paint(color, r > 0.0), &stroke, t, mask);
        }
        Ok(())
    }

    fn fill_circle(&mut self, center: Point, radius: u32, color: Color) -> Result {
        if color.a == 0
            || i64::from(radius) > BAND
            || !self.point_ok(i64::from(center.x), i64::from(center.y))
        {
            return Ok(());
        }
        let (cx, cy, r) = (i64::from(center.x), i64::from(center.y), i64::from(radius));
        let Some(masked) = self.prepare(self.span((cx - r, cy - r), (cx + r + 1, cy + r + 1), 1))
        else {
            return Ok(());
        };
        // The pixel at `center` is the middle of the disc.
        if let Some(path) = PathBuilder::from_circle(
            center.x as f32 + 0.5,
            center.y as f32 + 0.5,
            radius as f32 + 0.5,
        ) {
            let t = self.transform();
            let mask = if masked { self.mask.as_ref() } else { None };
            self.pixmap
                .fill_path(&path, &paint(color, true), FillRule::Winding, t, mask);
        }
        Ok(())
    }

    fn fill_path(&mut self, path: &[PathCmd], color: Color) -> Result {
        if color.a == 0 || !self.path_ok(path) {
            return Ok(());
        }
        let Some(masked) = self.prepare(self.path_span(path, 1)) else {
            return Ok(());
        };
        if let Some(path) = to_path(path, 0.0) {
            let t = self.transform();
            let mask = if masked { self.mask.as_ref() } else { None };
            self.pixmap
                .fill_path(&path, &paint(color, true), FillRule::EvenOdd, t, mask);
        }
        Ok(())
    }

    fn stroke_path(&mut self, path: &[PathCmd], width: u32, color: Color) -> Result {
        if width == 0 || color.a == 0 || i64::from(width) > BAND || !self.path_ok(path) {
            return Ok(());
        }
        // Half the width on each side, and a miter join can reach farther: a generous pad only costs a mask.
        let pad = i64::from(width).saturating_mul(2) + 2;
        let Some(masked) = self.prepare(self.path_span(path, pad)) else {
            return Ok(());
        };
        // Through pixel centers, so a line at an integer coordinate is crisp.
        if let Some(path) = to_path(path, 0.5) {
            let stroke = Stroke {
                width: width as f32,
                ..Stroke::default()
            };
            let t = self.transform();
            let mask = if masked { self.mask.as_ref() } else { None };
            self.pixmap
                .stroke_path(&path, &paint(color, true), &stroke, t, mask);
        }
        Ok(())
    }

    fn draw_text(&mut self, text: &str, at: Point, style: &TextStyle) -> Result {
        if text.is_empty() || style.color.a == 0 {
            return Ok(());
        }
        let font = font_for(style.size);
        let left = left_edge(at.x, text_size(font, text).w, style.align);
        // `embedded-graphics` lays glyphs out in `i32` and overflows near its ends.
        let far = |v: i32| i64::from(v).abs() > 1 << 24;
        if far(left) || far(at.y) {
            return Ok(());
        }
        let mut plot = Plot {
            skia: self,
            paint: paint(style.color, false),
        };
        let mono = MonoTextStyle::new(font, Rgb888::new(255, 255, 255));
        let origin = EgPoint::new(left, at.y);
        let _ = Text::with_baseline(text, origin, mono, Baseline::Top).draw(&mut plot);
        Ok(())
    }

    fn measure_text(&self, text: &str, style: &TextStyle) -> Size {
        text_size(font_for(style.size), text)
    }

    fn draw_image(&mut self, image: &Image<'_>, at: Point) -> Result {
        // `at` on its own must be small too: tiny-skia adds the image size to it in an i32.
        let near = in_band(i64::from(at.x)) && in_band(i64::from(at.y));
        if !near || !self.point_ok(i64::from(at.x), i64::from(at.y)) {
            return Ok(());
        }
        let span = self.span(
            (i64::from(at.x), i64::from(at.y)),
            (
                i64::from(at.x) + i64::from(image.width()),
                i64::from(at.y) + i64::from(image.height()),
            ),
            0,
        );
        let Some(masked) = self.prepare(span) else {
            return Ok(());
        };
        let Some(mut layer) = Pixmap::new(image.width(), image.height()) else {
            return Ok(());
        };
        let (dst, _) = layer.data_mut().as_chunks_mut::<4>();
        let (src, _) = image.rgba().as_chunks::<4>();
        for (dst, src) in dst.iter_mut().zip(src) {
            let a = u16::from(src[3]);
            let mul = |c: u8| ((u16::from(c) * a + 127) / 255) as u8;
            *dst = [mul(src[0]), mul(src[1]), mul(src[2]), src[3]];
        }
        let t = self.transform();
        let mask = if masked { self.mask.as_ref() } else { None };
        self.pixmap
            .draw_pixmap(at.x, at.y, layer.as_ref(), &PixmapPaint::default(), t, mask);
        Ok(())
    }
}

/// Lets `embedded-graphics` draw glyphs into the pixmap: every lit pixel becomes a 1x1 fill in the
/// text color, moved by the translation. The clip is a rectangle of whole pixels, so a dot outside it is skipped
/// here and text never needs the mask.
struct Plot<'a> {
    skia: &'a mut Skia,
    paint: Paint<'static>,
}

impl OriginDimensions for Plot<'_> {
    fn size(&self) -> EgSize {
        EgSize::new(self.skia.pixmap.width(), self.skia.pixmap.height())
    }
}

impl DrawTarget for Plot<'_> {
    type Color = Rgb888;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb888>>>(
        &mut self,
        pixels: I,
    ) -> core::result::Result<(), Infallible> {
        let (tx, ty) = (self.skia.state.tx, self.skia.state.ty);
        let clip = self.skia.state.clip;
        for Pixel(p, _) in pixels {
            let (x, y) = (
                i64::from(p.x) + i64::from(tx),
                i64::from(p.y) + i64::from(ty),
            );
            let inside = x >= i64::from(clip.x)
                && y >= i64::from(clip.y)
                && x < i64::from(clip.x) + i64::from(clip.w)
                && y < i64::from(clip.y) + i64::from(clip.h);
            if let Some(dot) = skia_rect(x as f32, y as f32, 1.0, 1.0).filter(|_| inside) {
                self.skia
                    .pixmap
                    .fill_rect(dot, &self.paint, Transform::identity(), None);
            }
        }
        Ok(())
    }
}

impl Probe for Skia {
    fn pixel(&self, x: i32, y: i32) -> Option<Color> {
        let (x, y) = (u32::try_from(x).ok()?, u32::try_from(y).ok()?);
        // `Pixmap::pixel` does not check `x` against the width: it would read the next row.
        if x >= self.pixmap.width() || y >= self.pixmap.height() {
            return None;
        }
        let p = self.pixmap.pixel(x, y)?.demultiply();
        Some(Color::rgba(p.red(), p.green(), p.blue(), p.alpha()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma_render::{Align, triangle};

    fn backend(w: u32, h: u32) -> Skia {
        Skia::new(w, h).unwrap()
    }

    #[test]
    fn passes_the_shared_conformance_suite() {
        fusuma_render::conformance::run_all(|| backend(64, 64));
        fusuma_render::conformance::run_all(|| backend(200, 120));
    }

    #[test]
    fn capabilities_are_honest() {
        let c = backend(8, 8).capabilities();
        assert!(c.alpha && c.antialiasing);
    }

    #[test]
    fn sizes_that_cannot_exist_are_refused() {
        assert!(Skia::new(0, 10).is_none());
        assert!(Skia::new(10, 0).is_none());
    }

    #[test]
    fn colors_blend_with_their_alpha() {
        let mut r = backend(16, 16);
        r.clear(Color::rgb(0, 0, 255)).unwrap();
        r.fill_rect(Rect::new(0, 0, 16, 16), 0, Color::rgba(255, 0, 0, 128))
            .unwrap();
        let c = r.pixel(8, 8).unwrap();
        assert!(
            c.r.abs_diff(128) <= 2 && c.b.abs_diff(127) <= 2 && c.g == 0,
            "{c:?}"
        );
        assert_eq!(c.a, 255);
        let mut r = backend(4, 4);
        r.fill_rect(Rect::new(0, 0, 4, 4), 0, Color::rgba(0, 255, 0, 100))
            .unwrap();
        assert_eq!(
            r.pixel(1, 1).unwrap().a,
            100,
            "a translucent fill on a clear surface stays translucent"
        );
    }

    #[test]
    fn edges_are_anti_aliased_and_rects_are_crisp() {
        let mut r = backend(40, 40);
        r.clear(Color::BLACK).unwrap();
        r.fill_circle(Point::new(20, 20), 12, Color::WHITE).unwrap();
        let soft = (0..40)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let c = r.pixel(x, y).unwrap();
                c.r > 20 && c.r < 235
            });
        assert!(soft.count() >= 20);
        let mut r = backend(20, 20);
        r.clear(Color::BLACK).unwrap();
        r.fill_rect(Rect::new(5, 5, 10, 10), 0, Color::WHITE)
            .unwrap();
        for (x, y) in [(5, 5), (14, 14), (4, 5), (15, 14)] {
            let c = r.pixel(x, y).unwrap().r;
            assert!(c == 0 || c == 255, "({x}, {y}) is {c}");
        }
    }

    #[cfg(feature = "png")]
    #[test]
    fn png_has_the_signature_and_decodes_to_the_size() {
        let mut r = backend(30, 20);
        r.clear(Color::rgb(10, 20, 30)).unwrap();
        let bytes = r.png().unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let back = Pixmap::decode_png(&bytes).unwrap();
        assert_eq!((back.width(), back.height()), (30, 20));
        assert_eq!(r.into_pixmap().width(), 30);
    }

    #[test]
    fn the_save_stack_is_bounded() {
        let mut r = backend(8, 8);
        for _ in 0..STATE_DEPTH {
            r.save().unwrap();
        }
        assert_eq!(r.save(), Err(Error::StackFull));
        for _ in 0..STATE_DEPTH {
            r.restore().unwrap();
        }
        assert_eq!(r.restore(), Err(Error::StackEmpty));
    }

    /// The toolkit's pattern: a clip for the redrawn part, one for a scroll window, one for each label, with the
    /// shapes inside them. None of it may cost a surface-sized mask: that is what kept tiny-skia over the memory the
    /// chooser counts for it (a mask of 1 byte a pixel per clip, copied on every `save`).
    #[test]
    fn shapes_and_text_inside_their_clips_need_no_mask() {
        let mut r = backend(120, 80);
        r.save().unwrap();
        r.clip(Rect::new(0, 0, 120, 80)).unwrap();
        r.clear(Color::BLACK).unwrap();
        for depth in 0..8 {
            r.save().unwrap();
            r.clip(Rect::new(depth, depth, 100, 60)).unwrap();
        }
        r.fill_rect(Rect::new(20, 20, 40, 20), 6, Color::WHITE)
            .unwrap();
        r.stroke_rect(Rect::new(20, 20, 40, 20), 6, 2, Color::WHITE)
            .unwrap();
        r.fill_circle(Point::new(50, 40), 8, Color::WHITE).unwrap();
        r.stroke_path(
            &triangle(Point::new(30, 30), Point::new(40, 30), Point::new(35, 36)),
            1,
            Color::WHITE,
        )
        .unwrap();
        // Text crossing the clip is cut dot by dot, without a mask.
        r.draw_text(
            "clipped",
            Point::new(90, 30),
            &TextStyle::new(10, Color::WHITE),
        )
        .unwrap();
        for _ in 0..9 {
            r.restore().unwrap();
        }
        assert_eq!(r.masks(), 0);
        assert!(r.pixel(50, 40).unwrap().r > 200, "the shapes were drawn");
        let past = (107..120).any(|x| (0..80).any(|y| r.pixel(x, y).unwrap().r > 0));
        assert!(!past, "no text dot right of the innermost clip at x = 107");
    }

    #[test]
    fn a_shape_crossing_the_clip_makes_one_mask_which_follows_nested_clips_and_restores() {
        let mut r = backend(60, 60);
        r.clear(Color::BLACK).unwrap();
        r.save().unwrap();
        r.clip(Rect::new(10, 10, 40, 40)).unwrap();
        r.save().unwrap();
        r.clip(Rect::new(20, 20, 10, 10)).unwrap();
        // Crosses the inner clip: the mask selects 20..30.
        r.fill_circle(Point::new(25, 25), 20, Color::rgb(255, 0, 0))
            .unwrap();
        r.restore().unwrap();
        // Crosses the outer clip: the same mask must now select 10..50, not stay at 20..30.
        r.fill_circle(Point::new(30, 30), 28, Color::rgb(0, 0, 255))
            .unwrap();
        r.restore().unwrap();
        // Back to no clip: inside the surface, so no mask is needed.
        r.fill_circle(Point::new(55, 55), 3, Color::rgb(0, 255, 0))
            .unwrap();
        assert_eq!(r.masks(), 1, "one mask, reused");
        let at = |x, y| r.pixel(x, y).unwrap();
        assert_eq!(at(5, 30), Color::BLACK, "left of the outer clip");
        assert_eq!(at(52, 30), Color::BLACK, "right of the outer clip");
        assert!(
            at(12, 30).b > 200,
            "inside the outer clip, outside the inner"
        );
        assert!(at(25, 25).b > 200, "the blue disc covers the red one");
        assert!(
            at(55, 55).g > 200,
            "after the last restore nothing is clipped"
        );
        // Only the inner clip let red through: draw it again on a fresh surface and look at its edge.
        let mut red = backend(60, 60);
        red.clear(Color::BLACK).unwrap();
        red.save().unwrap();
        red.clip(Rect::new(10, 10, 40, 40)).unwrap();
        red.clip(Rect::new(20, 20, 10, 10)).unwrap();
        red.fill_circle(Point::new(25, 25), 20, Color::rgb(255, 0, 0))
            .unwrap();
        for (x, y) in [(19, 25), (30, 25), (25, 19), (25, 30)] {
            assert_eq!(red.pixel(x, y).unwrap(), Color::BLACK, "({x}, {y})");
        }
        assert_eq!(red.pixel(20, 20).unwrap().r, 255);
        assert_eq!(red.pixel(29, 29).unwrap().r, 255);
    }

    /// The reverse order: the mask first selects a large clip, then a small one elsewhere. What the large one let
    /// through must be closed again, not only the new one opened.
    #[test]
    fn a_mask_that_moves_to_a_smaller_clip_closes_the_old_one() {
        let mut r = backend(40, 40);
        r.clear(Color::BLACK).unwrap();
        r.save().unwrap();
        r.clip(Rect::new(2, 2, 36, 36)).unwrap();
        r.fill_circle(Point::new(20, 20), 30, Color::rgb(0, 0, 255))
            .unwrap();
        r.restore().unwrap();
        r.save().unwrap();
        r.clip(Rect::new(30, 30, 5, 5)).unwrap();
        r.fill_circle(Point::new(20, 20), 30, Color::rgb(255, 0, 0))
            .unwrap();
        r.restore().unwrap();
        assert_eq!(r.masks(), 1);
        assert_eq!(
            r.pixel(10, 10).unwrap().r,
            0,
            "the old clip's area got no red"
        );
        assert_eq!(r.pixel(32, 32).unwrap().r, 255, "the new clip's area did");
    }

    #[test]
    fn an_empty_clip_hides_everything_and_a_clear_fills_exactly_the_clip() {
        let mut r = backend(20, 20);
        r.clear(Color::BLACK).unwrap();
        r.save().unwrap();
        r.clip(Rect::new(4, 4, 6, 6)).unwrap();
        r.clear(Color::WHITE).unwrap();
        r.clip(Rect::new(100, 100, 5, 5)).unwrap();
        r.fill_circle(Point::new(6, 6), 30, Color::rgb(255, 0, 0))
            .unwrap();
        r.draw_text(
            "x",
            Point::new(4, 4),
            &TextStyle::new(10, Color::rgb(255, 0, 0)),
        )
        .unwrap();
        r.restore().unwrap();
        for y in 0..20 {
            for x in 0..20 {
                let inside = (4..10).contains(&x) && (4..10).contains(&y);
                let want = if inside { Color::WHITE } else { Color::BLACK };
                assert_eq!(r.pixel(x, y).unwrap(), want, "({x}, {y})");
            }
        }
        assert_eq!(r.masks(), 0);
    }

    #[test]
    fn absurd_coordinates_never_panic() {
        let mut r = backend(16, 16);
        r.translate(i32::MAX, i32::MIN).unwrap();
        r.translate(i32::MAX, i32::MIN).unwrap();
        r.fill_rect(
            Rect::new(i32::MAX, i32::MIN, u32::MAX, u32::MAX),
            u32::MAX,
            Color::WHITE,
        )
        .unwrap();
        r.stroke_rect(Rect::new(0, 0, 9, 9), 4, u32::MAX, Color::WHITE)
            .unwrap();
        r.fill_circle(Point::new(i32::MAX, i32::MIN), u32::MAX, Color::WHITE)
            .unwrap();
        r.clip(Rect::new(i32::MIN, i32::MIN, u32::MAX, u32::MAX))
            .unwrap();
        r.draw_text(
            "x",
            Point::new(i32::MIN, i32::MAX),
            &TextStyle::new(10, Color::WHITE),
        )
        .unwrap();
        r.fill_path(
            &triangle(
                Point::new(i32::MIN, 0),
                Point::new(i32::MAX, 0),
                Point::new(0, i32::MAX),
            ),
            Color::WHITE,
        )
        .unwrap();
        r.stroke_path(
            &triangle(Point::new(0, 0), Point::new(1, 1), Point::new(2, 0)),
            0,
            Color::WHITE,
        )
        .unwrap();
        assert_eq!(r.pixel(-1, 0), None);
        assert_eq!(r.pixel(16, 0), None);
    }

    #[test]
    fn regression_a_huge_image_position_cancelled_by_a_translation_does_not_panic() {
        // Found by the draw fuzz target: tiny-skia adds the image size to the position in an i32.
        let mut r = backend(16, 16);
        r.translate(i32::MIN, i32::MIN).unwrap();
        let px = [255u8; 16];
        let img = Image::new(2, 2, &px).unwrap();
        r.draw_image(&img, Point::new(i32::MAX, i32::MAX)).unwrap();
        r.translate(i32::MAX, i32::MAX).unwrap();
        r.draw_image(&img, Point::new(i32::MAX - 1, i32::MAX - 1))
            .unwrap();
    }

    /// One scene on both backends: shapes without curves, text and an image must match pixel for
    /// pixel (within the RGB565 rounding), because the two backends share the fonts and the rules.
    fn scene<R: Renderer>(r: &mut R) {
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
    fn the_same_scene_looks_the_same_on_both_backends() {
        use fusuma_render_embedded::{Embedded, Frame};
        let mut a = backend(180, 120);
        let mut b = Embedded::new(Frame::new(180, 120, vec![0u16; 180 * 120]).unwrap());
        scene(&mut a);
        scene(&mut b);
        let mut differing = 0;
        for y in 0..120 {
            for x in 0..180 {
                let (p, q) = (a.pixel(x, y).unwrap(), b.pixel(x, y).unwrap());
                if p.r.abs_diff(q.r) > 10 || p.g.abs_diff(q.g) > 10 || p.b.abs_diff(q.b) > 10 {
                    differing += 1;
                }
            }
        }
        assert_eq!(
            differing, 0,
            "{differing} pixels differ between the backends"
        );
    }

    #[test]
    fn curves_agree_between_the_backends_except_on_their_edges() {
        use fusuma_render_embedded::{Embedded, Frame};
        fn curves<R: Renderer>(r: &mut R) {
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
        let mut a = backend(140, 120);
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
}
