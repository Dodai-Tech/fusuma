#![doc = include_str!("../README.md")]
#![no_std]

mod frame;
pub mod raster;

pub use frame::{Blend, Frame, PixelSource, mix565};

use core::fmt::Debug;

use embedded_graphics::{
    draw_target::DrawTargetExt,
    geometry::Size as EgSize,
    mono_font::{
        MonoFont, MonoTextStyle,
        ascii::{FONT_6X10, FONT_9X15, FONT_10X20},
    },
    pixelcolor::{IntoStorage, Rgb565},
    prelude::{DrawTarget, Drawable, Point as EgPoint, Primitive},
    primitives::{Circle, Line, PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use fusuma_render::{
    Capabilities, Color, Error, Image, PathCmd, Point, Probe, Rect, Renderer, Size, TextStyle,
    left_edge,
};

/// Coordinates farther than this from the origin (after the translation) are not drawn, except
/// for plain rectangles, which are clipped exactly. `embedded-graphics` does its geometry in
/// `i32` and overflows near the ends of the range; no display is anywhere near this size.
const BAND: i64 = 1 << 24;

fn in_band(v: i64) -> bool {
    (-BAND..=BAND).contains(&v)
}

/// The largest circle radius and rounded-rectangle side: `embedded-graphics` squares distances in
/// `i32`, which overflows beyond this. Bigger shapes are not drawn (no screen is that big).
/// Circles with a radius above this are drawn through the path rasteriser, which clips.
const BIG_CIRCLE: u32 = 256;
const MAX_EXTENT: u32 = 16_383;

/// The widest stroke drawn; a wider one is drawn this wide.
const MAX_STROKE: u32 = 1024;

/// How many nested `save` calls the renderer allows.
pub const STATE_DEPTH: usize = 8;

#[derive(Clone, Copy, Debug)]
struct State {
    clip: Rect,
    tx: i32,
    ty: i32,
}

/// The mono font that comes closest to a requested line height, and its glyph metrics.
#[must_use]
pub fn font_for(size: u32) -> &'static MonoFont<'static> {
    match size {
        0..=12 => &FONT_6X10,
        13..=17 => &FONT_9X15,
        _ => &FONT_10X20,
    }
}

/// The size of `text` in `font`, in pixels.
#[must_use]
pub fn text_size(font: &MonoFont<'_>, text: &str) -> Size {
    let n = text.chars().count() as u32;
    if n == 0 {
        return Size::new(0, 0);
    }
    let w = n * font.character_size.width + (n - 1) * font.character_spacing;
    Size::new(w, font.character_size.height)
}

fn rgb565(c: Color) -> Rgb565 {
    Rgb565::new(c.r >> 3, c.g >> 2, c.b >> 3)
}

fn eg_rect(r: Rect) -> Rectangle {
    Rectangle::new(EgPoint::new(r.x, r.y), EgSize::new(r.w, r.h))
}

/// The Fusuma renderer for an `embedded-graphics` RGB565 target.
#[derive(Debug)]
pub struct Embedded<T> {
    target: T,
    size: Size,
    state: State,
    stack: [State; STATE_DEPTH],
    depth: usize,
    /// Which linked family draws each font role (smooth text).
    #[cfg(feature = "fonts")]
    fonts: fusuma_fonts::FontSet,
}

impl<T: DrawTarget<Color = Rgb565>> Embedded<T> {
    /// Draws on `target`; the surface is the target's bounding box, which must start at (0, 0).
    pub fn new(target: T) -> Self {
        let b = target.bounding_box();
        let size = Size::new(b.size.width, b.size.height);
        let state = State {
            clip: Rect::new(0, 0, size.w, size.h),
            tx: 0,
            ty: 0,
        };
        Self {
            target,
            size,
            state,
            stack: [state; STATE_DEPTH],
            depth: 0,
            #[cfg(feature = "fonts")]
            fonts: fusuma_fonts::FontSet::default_set(),
        }
    }

    /// The target.
    pub const fn target(&self) -> &T {
        &self.target
    }

    /// The target, mutable (to flush a frame, for example).
    pub fn target_mut(&mut self) -> &mut T {
        &mut self.target
    }

    /// Draws the font roles with `set`'s families from now on; a role without one is the bitmap font.
    #[cfg(feature = "fonts")]
    pub fn set_fonts(&mut self, set: fusuma_fonts::FontSet) {
        self.fonts = set;
    }

    /// Gives the target back.
    pub fn into_target(self) -> T {
        self.target
    }

    fn shift(&self, p: Point) -> EgPoint {
        EgPoint::new(
            p.x.saturating_add(self.state.tx),
            p.y.saturating_add(self.state.ty),
        )
    }

    /// Whether every edge, moved by the translation, is inside the safe band.
    fn edges_ok(&self, edges: &raster::Edges) -> bool {
        edges.as_slice().iter().all(|e| {
            [e.a, e.b].iter().all(|p| {
                in_band(i64::from(p.x) + i64::from(self.state.tx))
                    && in_band(i64::from(p.y) + i64::from(self.state.ty))
            })
        })
    }

    /// Whether a device-space point is inside the safe band.
    fn point_ok(p: EgPoint) -> bool {
        in_band(i64::from(p.x)) && in_band(i64::from(p.y))
    }

    /// Runs `f` on the target limited to the current clip.
    fn draw<F>(&mut self, f: F) -> Result<(), Error<T::Error>>
    where
        F: FnOnce(&mut embedded_graphics::draw_target::Clipped<'_, T>) -> Result<(), T::Error>,
    {
        if self.state.clip.is_empty() {
            return Ok(());
        }
        let area = eg_rect(self.state.clip);
        f(&mut self.target.clipped(&area)).map_err(Error::Target)
    }
}

impl<T: DrawTarget<Color = Rgb565> + Blend> Embedded<T> {
    /// Draws `text` from a glyph atlas (smooth text), clipped, each glyph's coverage blended into the target.
    #[cfg(feature = "fonts")]
    fn draw_atlas_text(
        &mut self,
        atlas: &fusuma_fonts::atlas::Atlas,
        text: &str,
        at: Point,
        style: &TextStyle,
    ) -> Result<(), Error<T::Error>> {
        let width = atlas.width(text);
        let left = left_edge(at.x, width, style.align);
        // A smaller atlas than asked sits in the middle of the line, as the bitmap font does.
        let lift = (style.size.saturating_sub(u32::from(atlas.line)) / 2) as i32;
        let origin = self.shift(Point::new(left, at.y.saturating_add(lift)));
        if !Self::point_ok(origin) || self.state.clip.is_empty() {
            return Ok(());
        }
        let clip = self.state.clip;
        let (cx0, cy0) = (i64::from(clip.x), i64::from(clip.y));
        let (cx1, cy1) = (cx0 + i64::from(clip.w), cy0 + i64::from(clip.h));
        let color = rgb565(style.color);
        let opacity = u32::from(style.color.a);
        let mut buf = [0u8; 256];
        let mut pen: i64 = 0;
        for ch in text.chars() {
            let Some(g) = atlas.glyph_or_replacement(ch) else {
                continue;
            };
            let gx = i64::from(origin.x) + (pen + 32) / 64 + i64::from(g.left);
            let gy = i64::from(origin.y) + i64::from(g.top);
            pen += i64::from(g.advance);
            let (x0, x1) = (gx.max(cx0), (gx + i64::from(g.width)).min(cx1));
            if x0 >= x1 {
                continue;
            }
            for row in 0..u32::from(g.height) {
                let y = gy + i64::from(row);
                if y < cy0 || y >= cy1 {
                    continue;
                }
                let n = (x1 - x0) as usize;
                for (i, a) in buf[..n].iter_mut().enumerate() {
                    let c = u32::from(atlas.coverage_at(g, (x0 - gx) as u32 + i as u32, row));
                    *a = (c * 17 * opacity / 255) as u8;
                }
                // Inside the band checked above, so these fit `i32`.
                let (bx, by) = (x0 as i32, y as i32);
                if !self.target.blend_row(bx, by, &buf[..n], color) {
                    // The target cannot blend: the pixels at least half covered, solid.
                    let solid =
                        buf[..n]
                            .iter()
                            .enumerate()
                            .filter(|(_, a)| **a >= 128)
                            .map(|(i, _)| {
                                embedded_graphics::Pixel(EgPoint::new(bx + i as i32, by), color)
                            });
                    self.target.draw_iter(solid).map_err(Error::Target)?;
                }
            }
        }
        Ok(())
    }
}

/// The atlas for `style`'s font at its size in `set`, when smooth text is built in and the font is not the bitmap one.
#[cfg(feature = "fonts")]
fn atlas_for(
    set: &fusuma_fonts::FontSet,
    style: &TextStyle,
) -> Option<&'static fusuma_fonts::atlas::Atlas> {
    use fusuma_fonts::Face;
    let face = match style.font {
        fusuma_render::Font::Sans => Face::Sans,
        fusuma_render::Font::SansBold => Face::SansBold,
        fusuma_render::Font::Mono => Face::Mono,
        fusuma_render::Font::Bitmap => return None,
    };
    fusuma_fonts::atlas::Atlas::for_face(set, face, style.size)
}

impl<T: DrawTarget<Color = Rgb565> + Blend> Renderer for Embedded<T>
where
    T::Error: Debug,
{
    type Error = Error<T::Error>;

    fn size(&self) -> Size {
        self.size
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            alpha: false,
            antialiasing: false,
        }
    }

    fn save(&mut self) -> Result<(), Self::Error> {
        let slot = self.stack.get_mut(self.depth).ok_or(Error::StackFull)?;
        *slot = self.state;
        self.depth += 1;
        Ok(())
    }

    fn restore(&mut self) -> Result<(), Self::Error> {
        self.depth = self.depth.checked_sub(1).ok_or(Error::StackEmpty)?;
        self.state = self.stack[self.depth];
        Ok(())
    }

    fn clip(&mut self, rect: Rect) -> Result<(), Self::Error> {
        let device = rect.translate(self.state.tx, self.state.ty);
        self.state.clip = self.state.clip.intersect(&device);
        Ok(())
    }

    fn translate(&mut self, dx: i32, dy: i32) -> Result<(), Self::Error> {
        self.state.tx = self.state.tx.saturating_add(dx);
        self.state.ty = self.state.ty.saturating_add(dy);
        Ok(())
    }

    fn clear(&mut self, color: Color) -> Result<(), Self::Error> {
        self.draw(|t| t.clear(rgb565(color)))
    }

    fn fill_rect(&mut self, rect: Rect, radius: u32, color: Color) -> Result<(), Self::Error> {
        if rect.is_empty() || color.a == 0 {
            return Ok(());
        }
        if radius.min(rect.w / 2).min(rect.h / 2) > 0 {
            return self.fill_path(&raster::rounded_rect(rect, radius), color);
        }
        // A plain rectangle is clipped exactly, whatever its coordinates.
        let visible = rect
            .translate(self.state.tx, self.state.ty)
            .intersect(&self.state.clip);
        if visible.is_empty() {
            return Ok(());
        }
        self.target
            .fill_solid(&eg_rect(visible), rgb565(color))
            .map_err(Error::Target)
    }

    fn stroke_rect(
        &mut self,
        rect: Rect,
        radius: u32,
        width: u32,
        color: Color,
    ) -> Result<(), Self::Error> {
        if rect.is_empty() || width == 0 || color.a == 0 {
            return Ok(());
        }
        if width.saturating_mul(2) >= rect.w.min(rect.h) {
            return self.fill_rect(rect, radius, color);
        }
        if radius.min(rect.w / 2).min(rect.h / 2) == 0 {
            // Four bars: top and bottom full width, left and right between them.
            let side = rect.h - 2 * width;
            let bars = [
                Rect::new(rect.x, rect.y, rect.w, width),
                Rect::new(
                    rect.x,
                    rect.bottom().saturating_sub(width as i32),
                    rect.w,
                    width,
                ),
                Rect::new(rect.x, rect.y.saturating_add(width as i32), width, side),
                Rect::new(
                    rect.right().saturating_sub(width as i32),
                    rect.y.saturating_add(width as i32),
                    width,
                    side,
                ),
            ];
            for bar in bars {
                self.fill_rect(bar, 0, color)?;
            }
            return Ok(());
        }
        // A rounded ring: the outer outline and the inner one, filled with the even-odd rule.
        let mut ring = [PathCmd::Close; 20];
        ring[..10].copy_from_slice(&raster::rounded_rect(rect, radius));
        ring[10..].copy_from_slice(&raster::rounded_rect(
            rect.inset(width),
            radius.saturating_sub(width),
        ));
        self.fill_path(&ring, color)
    }

    fn fill_circle(&mut self, center: Point, radius: u32, color: Color) -> Result<(), Self::Error> {
        if color.a == 0 {
            return Ok(());
        }
        let c = self.shift(center);
        if !Self::point_ok(c) || radius > MAX_EXTENT {
            return Ok(());
        }
        let diameter = radius * 2 + 1;
        // Nothing to do when the circle is off the clip; and a big circle is a rounded square, which the
        // path rasteriser clips to the screen, where walking every point of the circle would not stop.
        let bounds = Rect::new(c.x - radius as i32, c.y - radius as i32, diameter, diameter);
        if bounds.intersect(&self.state.clip).is_empty() {
            return Ok(());
        }
        if radius > BIG_CIRCLE {
            let square = Rect::new(
                center.x.saturating_sub(radius as i32),
                center.y.saturating_sub(radius as i32),
                diameter,
                diameter,
            );
            return self.fill_rect(square, radius, color);
        }
        self.draw(|t| {
            Circle::with_center(c, diameter)
                .into_styled(PrimitiveStyle::with_fill(rgb565(color)))
                .draw(t)
        })
    }

    fn fill_path(&mut self, path: &[PathCmd], color: Color) -> Result<(), Self::Error> {
        if color.a == 0 {
            return Ok(());
        }
        let edges = raster::flatten(path, true).map_err(|_| Error::TooComplex)?;
        let (tx, ty) = (self.state.tx, self.state.ty);
        if !self.edges_ok(&edges) {
            return Ok(());
        }
        let local = self
            .state
            .clip
            .translate(tx.saturating_neg(), ty.saturating_neg());
        let value = rgb565(color);
        let mut result = Ok(());
        let spans = raster::fill_spans(&edges, local, |y, x0, x1| {
            if result.is_ok() {
                let run = Rectangle::new(
                    EgPoint::new(x0.saturating_add(tx), y.saturating_add(ty)),
                    EgSize::new((i64::from(x1) - i64::from(x0)) as u32, 1),
                );
                result = self.target.fill_solid(&run, value);
            }
        });
        spans.map_err(|_| Error::TooComplex)?;
        result.map_err(Error::Target)
    }

    fn stroke_path(
        &mut self,
        path: &[PathCmd],
        width: u32,
        color: Color,
    ) -> Result<(), Self::Error> {
        if width == 0 || color.a == 0 {
            return Ok(());
        }
        let edges = raster::flatten(path, false).map_err(|_| Error::TooComplex)?;
        let (tx, ty) = (self.state.tx, self.state.ty);
        if !self.edges_ok(&edges) {
            return Ok(());
        }
        let width = width.min(MAX_STROKE);
        let value = rgb565(color);
        let style = PrimitiveStyle::with_stroke(value, width);
        // Cut every segment to the visible area plus the stroke's reach: `embedded-graphics`
        // squares the length of a line in `i32`, which a long off-screen line would overflow.
        let m = width + 2;
        let clip = self.state.clip;
        let reach = Rect::new(
            clip.x.saturating_sub(m as i32),
            clip.y.saturating_sub(m as i32),
            clip.w.saturating_add(2 * m),
            clip.h.saturating_add(2 * m),
        );
        self.draw(|t| {
            for e in edges.as_slice() {
                let a = Point::new(e.a.x.saturating_add(tx), e.a.y.saturating_add(ty));
                let b = Point::new(e.b.x.saturating_add(tx), e.b.y.saturating_add(ty));
                let Some((a, b)) = raster::clip_segment(a, b, reach) else {
                    continue;
                };
                let (dx, dy) = (
                    i64::from(b.x) - i64::from(a.x),
                    i64::from(b.y) - i64::from(a.y),
                );
                // `embedded-graphics` computes (2 * width)^2 * length^2 in an i32.
                let thick = (i64::from(width) * 2).pow(2) * (dx * dx + dy * dy);
                if thick <= i64::from(i32::MAX) {
                    Line::new(EgPoint::new(a.x, a.y), EgPoint::new(b.x, b.y))
                        .into_styled(style)
                        .draw(t)?;
                } else if let Some(q) = raster::stroke_quad(a, b, width) {
                    // Too thick and long for that line: fill the band around it instead.
                    let band = [
                        PathCmd::MoveTo(q[0]),
                        PathCmd::LineTo(q[1]),
                        PathCmd::LineTo(q[2]),
                        PathCmd::LineTo(q[3]),
                        PathCmd::Close,
                    ];
                    let Ok(edges) = raster::flatten(&band, true) else {
                        continue;
                    };
                    let mut failed = None;
                    let _ = raster::fill_spans(&edges, clip, |y, x0, x1| {
                        let width = (i64::from(x1) - i64::from(x0)) as u32;
                        let run = Rectangle::new(EgPoint::new(x0, y), EgSize::new(width, 1));
                        if let Err(e) = t.fill_solid(&run, value) {
                            failed.get_or_insert(e);
                        }
                    });
                    if let Some(e) = failed {
                        return Err(e);
                    }
                }
            }
            Ok(())
        })
    }

    fn draw_text(&mut self, text: &str, at: Point, style: &TextStyle) -> Result<(), Self::Error> {
        if text.is_empty() || style.color.a == 0 {
            return Ok(());
        }
        #[cfg(feature = "fonts")]
        if let Some(atlas) = atlas_for(&self.fonts, style) {
            return self.draw_atlas_text(atlas, text, at, style);
        }
        let font = font_for(style.size);
        let width = text_size(font, text).w;
        let left = left_edge(at.x, width, style.align);
        let origin = self.shift(Point::new(left, at.y));
        if !Self::point_ok(origin) {
            return Ok(());
        }
        let mono = MonoTextStyle::new(font, rgb565(style.color));
        self.draw(|t| {
            Text::with_baseline(text, origin, mono, Baseline::Top)
                .draw(t)
                .map(drop)
        })
    }

    fn measure_text(&self, text: &str, style: &TextStyle) -> Size {
        #[cfg(feature = "fonts")]
        if let Some(atlas) = atlas_for(&self.fonts, style) {
            let width = atlas.width(text);
            return Size::new(
                width,
                if width == 0 {
                    0
                } else {
                    style.size.max(u32::from(atlas.line))
                },
            );
        }
        text_size(font_for(style.size), text)
    }

    fn draw_image(&mut self, image: &Image<'_>, at: Point) -> Result<(), Self::Error> {
        let origin = self.shift(at);
        if !Self::point_ok(origin) {
            return Ok(());
        }
        let (w, h) = (image.width(), image.height());
        let pixels = (0..h)
            .flat_map(move |y| (0..w).map(move |x| (x, y)))
            .filter_map(|(x, y)| {
                let [r, g, b, a] = image.pixel(x, y)?;
                let at = EgPoint::new(
                    origin
                        .x
                        .saturating_add(i32::try_from(x).unwrap_or(i32::MAX)),
                    origin
                        .y
                        .saturating_add(i32::try_from(y).unwrap_or(i32::MAX)),
                );
                (a != 0).then(|| embedded_graphics::Pixel(at, rgb565(Color::rgb(r, g, b))))
            });
        self.draw(|t| t.draw_iter(pixels))
    }
}

impl<T: DrawTarget<Color = Rgb565> + PixelSource> Probe for Embedded<T> {
    fn pixel(&self, x: i32, y: i32) -> Option<Color> {
        self.target
            .pixel_at(x, y)
            .map(|c| Color::from_rgb565(c.into_storage()))
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec;
    use std::vec::Vec;

    type Backend = Embedded<Frame<Vec<u16>>>;

    fn backend(w: u16, h: u16) -> Backend {
        Embedded::new(Frame::new(w, h, vec![0u16; usize::from(w) * usize::from(h)]).unwrap())
    }

    /// A regression found by the fuzzer: a circle far bigger than the screen used to be walked point by
    /// point (billions of them). It must finish at once and draw the part that is on the screen.
    #[test]
    fn a_huge_circle_is_clipped_and_not_walked_point_by_point() {
        let c = Color::rgb(255, 255, 0);
        let mut b = backend(48, 40);
        b.fill_circle(Point::new(24, 20), 10_000, c).unwrap();
        assert_eq!(b.pixel(0, 0), Some(c), "the screen is inside the circle");
        assert_eq!(b.pixel(47, 39), Some(c));
        // A circle whose edge crosses the screen: the near side is filled, the far corner is not.
        let mut b = backend(48, 40);
        b.fill_circle(Point::new(-4000, 20), 4010, c).unwrap();
        assert_eq!(b.pixel(2, 20), Some(c));
        assert_ne!(b.pixel(47, 20), Some(c));
        // Entirely off the screen: nothing is drawn, and nothing takes long.
        let mut b = backend(48, 40);
        b.fill_circle(Point::new(9000, 9000), 5000, c).unwrap();
        assert_ne!(b.pixel(10, 10), Some(c));
        // A circle just above the threshold looks like one just below it.
        let (mut small, mut big) = (backend(64, 64), backend(64, 64));
        small.fill_circle(Point::new(32, 32), 30, c).unwrap();
        big.fill_circle(Point::new(32, 32), 300, c).unwrap();
        assert_eq!(small.pixel(32, 32), big.pixel(32, 32));
    }

    #[test]
    fn passes_the_shared_conformance_suite() {
        fusuma_render::conformance::run_all(|| backend(64, 64));
        fusuma_render::conformance::run_all(|| backend(200, 120));
    }

    #[test]
    fn capabilities_are_honest() {
        let c = backend(8, 8).capabilities();
        assert!(!c.alpha && !c.antialiasing);
    }

    #[test]
    fn fonts_follow_the_requested_size_and_measure_by_glyph() {
        assert_eq!(font_for(0).character_size.height, 10);
        assert_eq!(font_for(12).character_size.height, 10);
        assert_eq!(font_for(13).character_size.height, 15);
        assert_eq!(font_for(17).character_size.height, 15);
        assert_eq!(font_for(18).character_size.height, 20);
        assert_eq!(font_for(u32::MAX).character_size.height, 20);
        assert_eq!(text_size(&FONT_10X20, "abc"), Size::new(30, 20));
        assert_eq!(text_size(&FONT_10X20, ""), Size::new(0, 0));
        assert_eq!(
            text_size(&FONT_10X20, "é"),
            Size::new(10, 20),
            "chars, not bytes"
        );
    }

    #[test]
    fn the_save_stack_is_bounded_and_restores_exactly() {
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

    #[test]
    fn a_huge_path_is_too_complex_not_a_crash() {
        let mut r = backend(32, 32);
        let mut path = vec![PathCmd::MoveTo(Point::new(0, 0))];
        path.extend((1..=200).map(|i| PathCmd::LineTo(Point::new(i % 30, i % 7))));
        assert_eq!(r.fill_path(&path, Color::WHITE), Err(Error::TooComplex));
        assert_eq!(
            r.stroke_path(&path, 2, Color::WHITE),
            Err(Error::TooComplex)
        );
    }

    #[test]
    fn drawing_with_an_empty_clip_or_far_away_does_nothing_and_never_panics() {
        let mut r = backend(16, 16);
        r.clip(Rect::new(100, 100, 5, 5)).unwrap();
        r.fill_rect(Rect::new(0, 0, 16, 16), 3, Color::WHITE)
            .unwrap();
        r.fill_circle(Point::new(i32::MAX, i32::MIN), u32::MAX, Color::WHITE)
            .unwrap();
        r.draw_text(
            "x",
            Point::new(i32::MIN, i32::MAX),
            &TextStyle::new(10, Color::WHITE),
        )
        .unwrap();
        assert!(r.target().pixels().iter().all(|&p| p == 0));
        let mut r = backend(16, 16);
        r.translate(i32::MAX, i32::MAX).unwrap();
        r.translate(i32::MAX, i32::MAX).unwrap();
        r.fill_rect(
            Rect::new(i32::MAX, i32::MAX, u32::MAX, u32::MAX),
            u32::MAX,
            Color::WHITE,
        )
        .unwrap();
        r.stroke_rect(Rect::new(0, 0, 9, 9), 4, u32::MAX, Color::WHITE)
            .unwrap();
    }

    #[test]
    fn regression_wide_and_long_strokes_do_not_overflow_embedded_graphics() {
        // Found by the draw fuzz target: (2 * width)^2 * length^2 overflowed an i32 in the thick line.
        let mut r = backend(48, 40);
        let line = [
            PathCmd::MoveTo(Point::new(-40_000, 20)),
            PathCmd::LineTo(Point::new(40_000, 20)),
        ];
        r.stroke_path(&line, 1000, Color::WHITE).unwrap();
        assert_eq!(
            r.pixel(10, 20).unwrap(),
            Color::WHITE,
            "the wide band covers the surface"
        );
        let mut r = backend(48, 40);
        r.stroke_path(&line, 6, Color::WHITE).unwrap();
        assert_eq!(r.pixel(10, 20).unwrap(), Color::WHITE);
        assert_eq!(r.pixel(10, 30).unwrap(), Color::BLACK);
        let mut r = backend(48, 40);
        r.stroke_rect(Rect::new(0, 0, 30_000, 30_000), 9, u32::MAX, Color::WHITE)
            .unwrap();
        r.stroke_rect(Rect::new(2, 2, 40, 30), 12, 3, Color::WHITE)
            .unwrap();
        r.fill_rect(Rect::new(0, 0, u32::MAX, u32::MAX), 1_000_000, Color::WHITE)
            .unwrap();
    }

    #[test]
    fn rounded_strokes_are_rings_and_rounded_fills_keep_their_corners_cut() {
        let mut r = backend(60, 60);
        r.stroke_rect(Rect::new(5, 5, 50, 50), 14, 4, Color::WHITE)
            .unwrap();
        assert_eq!(r.pixel(30, 6).unwrap(), Color::WHITE, "top bar");
        assert_eq!(r.pixel(6, 30).unwrap(), Color::WHITE, "left bar");
        assert_eq!(r.pixel(30, 30).unwrap(), Color::BLACK, "hollow inside");
        assert_eq!(r.pixel(5, 5).unwrap(), Color::BLACK, "round corner");
        assert_eq!(
            r.pixel(30, 12).unwrap(),
            Color::BLACK,
            "inside the 4 px ring"
        );
    }

    #[test]
    fn a_target_can_be_taken_back() {
        let mut r = backend(4, 4);
        r.clear(Color::WHITE).unwrap();
        assert!(r.target_mut().pixels().iter().all(|&p| p == 0xFFFF));
        assert!(r.into_target().pixels().iter().all(|&p| p == 0xFFFF));
    }
}

#[cfg(test)]
#[cfg(feature = "fonts")]
mod atlas_text_tests {
    extern crate std;
    use std::vec;
    use std::vec::Vec;

    use fusuma_render::{Align, Font, Probe, Rect, Renderer, TextStyle};

    use super::*;

    type Screen = Embedded<Frame<Vec<u16>>>;

    fn screen(w: u16, h: u16) -> Screen {
        Embedded::new(Frame::new(w, h, vec![0u16; usize::from(w) * usize::from(h)]).unwrap())
    }

    /// The brightness (0 to 255) of every pixel of a `w x h` area of `r`.
    fn grey(r: &impl Probe, w: u32, h: u32) -> Vec<u8> {
        (0..h as i32)
            .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
            .map(|(x, y)| r.pixel(x, y).map_or(0, |c| c.g))
            .collect()
    }

    /// White `text` on black in `font` at line `size`, drawn by `r`.
    fn paint<R: Renderer>(r: &mut R, text: &str, font: Font, size: u32) {
        r.clear(Color::BLACK).unwrap();
        r.draw_text(
            text,
            Point::new(2, 2),
            &TextStyle::new(size, Color::WHITE).font(font),
        )
        .unwrap();
    }

    #[test]
    fn smooth_text_has_partial_pixels_and_fits_its_measured_box() {
        for (font, size) in [
            (Font::Sans, 10),
            (Font::Sans, 15),
            (Font::SansBold, 20),
            (Font::Mono, 15),
        ] {
            let mut r = screen(160, 40);
            let text = "Wi-Fi ș 70%";
            paint(&mut r, text, font, size);
            let px = grey(&r, 160, 40);
            let partial = px.iter().filter(|&&g| g > 10 && g < 245).count();
            let full = px.iter().filter(|&&g| g >= 245).count();
            // At a 10 pixel line Inter's stems are thinner than a pixel, so none is fully covered there.
            let strong = px.iter().any(|&g| g > if size >= 15 { 240 } else { 180 });
            assert!(
                partial > 20 && strong,
                "{font:?} {size}: {partial} partial, {full} full"
            );
            let m = r.measure_text(text, &TextStyle::new(size, Color::WHITE).font(font));
            assert_eq!(m.h, size, "the line is as high as asked");
            // No ink outside the measured box (from (2, 2)), allowing a pixel for edges.
            for y in 0..40i32 {
                for x in 0..160i32 {
                    let inside =
                        (1..=2 + m.w as i32).contains(&x) && (1..=2 + m.h as i32).contains(&y);
                    assert!(
                        inside || px[(y * 160 + x) as usize] == 0,
                        "{font:?} {size}: ink at ({x}, {y}) outside {m:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_bitmap_font_is_still_the_bitmap_font() {
        let mut a = screen(100, 30);
        paint(&mut a, "Hi", Font::Bitmap, 20);
        assert!(
            grey(&a, 100, 30).iter().all(|&g| g == 0 || g == 255),
            "one bit a pixel"
        );
        assert_eq!(
            a.measure_text("Hi", &TextStyle::new(20, Color::WHITE).font(Font::Bitmap)),
            text_size(font_for(20), "Hi")
        );
    }

    #[test]
    fn text_is_clipped_aligned_and_blends_with_what_is_under_it() {
        let style = TextStyle::new(15, Color::WHITE);
        let mut r = screen(120, 30);
        r.clear(Color::rgb(0, 0, 255)).unwrap();
        r.save().unwrap();
        r.clip(Rect::new(0, 0, 20, 30)).unwrap();
        r.draw_text("Notifications", Point::new(2, 2), &style)
            .unwrap();
        r.restore().unwrap();
        for y in 0..30 {
            for x in 20..120 {
                assert_eq!(
                    r.pixel(x, y),
                    Some(Color::rgb(0, 0, 255)),
                    "({x}, {y}) is outside the clip"
                );
            }
        }
        // Cut across the line: rows below the clip stay untouched.
        let mut r2 = screen(120, 30);
        r2.clear(Color::BLACK).unwrap();
        r2.save().unwrap();
        r2.clip(Rect::new(0, 0, 120, 9)).unwrap();
        r2.draw_text("Wipg", Point::new(2, 2), &style).unwrap();
        r2.restore().unwrap();
        assert!(
            (0..120).any(|x| (0..9).any(|y| r2.pixel(x, y).is_some_and(|c| c.g > 0))),
            "the top shows"
        );
        for y in 9..30 {
            for x in 0..120 {
                assert_eq!(
                    r2.pixel(x, y),
                    Some(Color::BLACK),
                    "({x}, {y}) is below the clip"
                );
            }
        }
        // An edge pixel is a mix of white and the blue under it, not of white and black.
        let mixed = (0..30)
            .flat_map(|y| (0..20).map(move |x| (x, y)))
            .filter_map(|(x, y)| r.pixel(x, y))
            .any(|c| c.r > 40 && c.r < 210 && c.b > 230);
        assert!(mixed, "edges blend into the blue");
        // Right-aligned text ends at the anchor.
        let mut r = screen(120, 30);
        r.clear(Color::BLACK).unwrap();
        r.draw_text("Wi", Point::new(100, 5), &style.aligned(Align::Right))
            .unwrap();
        let ink_x: Vec<i32> = (0..120)
            .filter(|&x| (0..30).any(|y| r.pixel(x, y).is_some_and(|c| c.g > 0)))
            .collect();
        assert!(
            *ink_x.last().unwrap() <= 100 && *ink_x.last().unwrap() >= 95,
            "{ink_x:?}"
        );
        // Transparent text draws nothing; absurd positions do not panic.
        r.draw_text(
            "Wi",
            Point::new(10, 5),
            &TextStyle::new(15, Color::rgba(255, 0, 0, 0)),
        )
        .unwrap();
        r.draw_text("Wi", Point::new(i32::MAX, i32::MIN), &style)
            .unwrap();
        r.translate(i32::MAX, 0).unwrap();
        r.draw_text("Wi", Point::new(i32::MAX, 0), &style).unwrap();
    }

    /// A target that cannot read its pixels: the text is drawn one bit a pixel.
    struct Plain(Frame<Vec<u16>>);
    impl embedded_graphics::geometry::OriginDimensions for Plain {
        fn size(&self) -> embedded_graphics::geometry::Size {
            self.0.size()
        }
    }
    impl DrawTarget for Plain {
        type Color = Rgb565;
        type Error = core::convert::Infallible;
        fn draw_iter<I: IntoIterator<Item = embedded_graphics::Pixel<Rgb565>>>(
            &mut self,
            p: I,
        ) -> Result<(), Self::Error> {
            self.0.draw_iter(p)
        }
    }
    impl Blend for Plain {}

    #[test]
    fn a_target_that_cannot_blend_gets_the_half_covered_pixels_solid() {
        let mut r = Embedded::new(Plain(Frame::new(80, 24, vec![0u16; 80 * 24]).unwrap()));
        r.draw_text("Wi", Point::new(2, 2), &TextStyle::new(15, Color::WHITE))
            .unwrap();
        let px = r.target().0.pixels();
        assert!(px.iter().all(|&p| p == 0 || p == 0xffff), "no blending");
        assert!(
            px.iter().filter(|&&p| p == 0xffff).count() > 20,
            "but the strokes are there"
        );
    }

    /// The atlas draws the same text as Vello with the same font: the same ink in the same place. Not the same
    /// pixels: Vello hints and kerns, the atlas does neither, and positions glyphs on whole pixels.
    #[test]
    fn smooth_text_looks_like_vellos() {
        for (font, size) in [
            (Font::Sans, 10),
            (Font::Sans, 15),
            (Font::Sans, 20),
            (Font::SansBold, 20),
            (Font::Mono, 15),
        ] {
            let text = "Settings 42 ș";
            let (w, h) = (180u32, 32u32);
            let mut a = screen(w as u16, h as u16);
            let mut v = fusuma_render_vello_cpu::new(w, h).unwrap();
            paint(&mut a, text, font, size);
            paint(&mut v, text, font, size);
            let (pa, pv) = (grey(&a, w, h), grey(&v, w, h));
            let ink = |p: &[u8]| p.iter().map(|&g| u64::from(g)).sum::<u64>();
            let (ia, iv) = (ink(&pa), ink(&pv));
            let ratio = ia as f64 / iv as f64;
            // Where the ink is: the bounding box of pixels above half.
            let bbox = |p: &[u8]| {
                let lit: Vec<(u32, u32)> = (0..h)
                    .flat_map(|y| (0..w).map(move |x| (x, y)))
                    .filter(|&(x, y)| p[(y * w + x) as usize] > 127)
                    .collect();
                let xs = lit.iter().map(|p| p.0);
                let ys = lit.iter().map(|p| p.1);
                (
                    xs.clone().min().unwrap(),
                    ys.clone().min().unwrap(),
                    xs.max().unwrap(),
                    ys.max().unwrap(),
                )
            };
            let (ba, bv) = (bbox(&pa), bbox(&pv));
            // Blurred difference: each pixel against the 3x3 mean of the other, so a half-pixel shift does not count.
            let diff: u64 = (1..h - 1)
                .flat_map(|y| (1..w - 1).map(move |x| (x, y)))
                .map(|(x, y)| {
                    let mean = |p: &[u8]| {
                        (0..9)
                            .map(|k| u64::from(p[((y + k / 3 - 1) * w + x + k % 3 - 1) as usize]))
                            .sum::<u64>()
                            / 9
                    };
                    mean(&pa).abs_diff(mean(&pv))
                })
                .sum();
            let rel = diff as f64 / iv as f64;
            if let Ok(dir) = std::env::var("FUSUMA_ATLAS_PGM") {
                let mut out = std::format!("P5 {w} {} 255\n", 2 * h).into_bytes();
                out.extend_from_slice(&pa);
                out.extend_from_slice(&pv);
                std::fs::write(std::format!("{dir}/{font:?}-{size}.pgm"), out).unwrap();
            }
            std::println!(
                "{font:?} {size}: ink ratio {ratio:.3}, boxes {ba:?} vs {bv:?}, blurred difference {:.1}% of the ink",
                rel * 100.0
            );
            assert!(
                (0.85..1.15).contains(&ratio),
                "{font:?} {size}: ink {ia} vs {iv}"
            );
            assert!(
                ba.1.abs_diff(bv.1) <= 1 && ba.3.abs_diff(bv.3) <= 1,
                "{font:?} {size}: rows {ba:?} vs {bv:?}"
            );
            assert!(
                ba.0.abs_diff(bv.0) <= 1 && ba.2.abs_diff(bv.2) <= 3,
                "{font:?} {size}: columns {ba:?} vs {bv:?}"
            );
            assert!(
                rel < 0.25,
                "{font:?} {size}: blurred difference {:.1}%",
                rel * 100.0
            );
        }
    }
}
