#![doc = include_str!("../README.md")]
#![no_std]

mod cover;
mod fill;
mod frame;
mod mono;
pub mod raster;

pub use frame::{Blend, Frame, PixelSource, mix565};
pub use mono::{Dither, MonoFrame, luma565};

use core::fmt::Debug;

use embedded_graphics::{
    draw_target::DrawTargetExt,
    geometry::Size as EgSize,
    mono_font::MonoTextStyle,
    pixelcolor::{IntoStorage, Rgb565},
    prelude::{DrawTarget, Drawable, Point as EgPoint},
    primitives::Rectangle,
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

pub use fusuma_render::bitmap::{font_for, font_for_char, runs, text_size};

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
    /// Fills row `y` from `x0` to `x1` (device pixels, inside the clip) in `color` at `alpha` (0 to 255).
    fn span(
        &mut self,
        y: i64,
        x0: i64,
        x1: i64,
        color: Rgb565,
        alpha: u8,
    ) -> Result<(), Error<T::Error>> {
        if x1 <= x0 || alpha == 0 {
            return Ok(());
        }
        // Inside the clip, which is inside the target: these fit `i32`.
        let (x, y) = (x0 as i32, y as i32);
        if alpha == 255 {
            let run = Rectangle::new(EgPoint::new(x, y), EgSize::new((x1 - x0) as u32, 1));
            return self.target.fill_solid(&run, color).map_err(Error::Target);
        }
        let buf = [alpha; 256];
        let mut at = x0;
        while at < x1 {
            let n = (x1 - at).min(256) as usize;
            self.blend(at as i32, y, &buf[..n], color)?;
            at += n as i64;
        }
        Ok(())
    }

    /// Mixes `alpha` (one entry a pixel from `(x, y)`) of `color` into the target; a target that cannot blend gets
    /// the pixels at least half covered, solid.
    fn blend(
        &mut self,
        x: i32,
        y: i32,
        alpha: &[u8],
        color: Rgb565,
    ) -> Result<(), Error<T::Error>> {
        if self.target.blend_row(x, y, alpha, color) {
            return Ok(());
        }
        let solid = alpha
            .iter()
            .enumerate()
            .filter(|(_, a)| **a >= 128)
            .map(|(i, _)| embedded_graphics::Pixel(EgPoint::new(x + i as i32, y), color));
        self.target.draw_iter(solid).map_err(Error::Target)
    }

    /// Paints `shape` (local pixels) in `color`, anti-aliased and clipped: rows without a curved edge as spans, the
    /// others pixel by pixel.
    fn cover(&mut self, shape: cover::Shape, color: Color) -> Result<(), Error<T::Error>> {
        let (tx, ty) = (i64::from(self.state.tx), i64::from(self.state.ty));
        let moved = |r: cover::RoundRect| cover::RoundRect {
            x0: r.x0 + tx,
            y0: r.y0 + ty,
            x1: r.x1 + tx,
            y1: r.y1 + ty,
            rad: r.rad,
        };
        let shape = cover::Shape {
            outer: moved(shape.outer),
            inner: shape.inner.map(moved),
        };
        let clip = self.state.clip;
        let (cx0, cy0) = (i64::from(clip.x), i64::from(clip.y));
        let (cx1, cy1) = (cx0 + i64::from(clip.w), cy0 + i64::from(clip.h));
        let o = shape.outer;
        let (y0, y1) = (o.y0.max(cy0), o.y1.min(cy1));
        let (vx0, vx1) = (o.x0.max(cx0), o.x1.min(cx1));
        if vx0 >= vx1 {
            return Ok(());
        }
        let value = rgb565(color);
        let opacity = u32::from(color.a);
        let mut buf = [0u8; 256];
        for y in y0..y1 {
            match shape.row(y) {
                cover::Row::Empty => {}
                cover::Row::Span(a, b) => self.span(y, a.max(vx0), b.min(vx1), value, color.a)?,
                cover::Row::Bars(a, b, c, d) => {
                    self.span(y, a.max(vx0), b.min(vx1), value, color.a)?;
                    self.span(y, c.max(vx0), d.min(vx1), value, color.a)?;
                }
                cover::Row::Curved(a, d) => {
                    // The corners pixel by pixel, the straight middle as one span.
                    let (b, c) = shape.corner_columns();
                    let alpha = |cov: u8| (u32::from(cov) * opacity / 255) as u8;
                    for (from, to) in [(a, b), (c, d)] {
                        let (mut x, end) = (from.max(vx0), to.min(vx1));
                        while x < end {
                            let n = (end - x).min(256) as usize;
                            for (i, v) in buf[..n].iter_mut().enumerate() {
                                *v = alpha(shape.coverage(x + i as i64, y));
                            }
                            self.blend(x as i32, y as i32, &buf[..n], value)?;
                            x += n as i64;
                        }
                    }
                    let middle = alpha(shape.coverage(b, y));
                    self.span(y, b.max(vx0), c.min(vx1), value, middle)?;
                }
            }
        }
        Ok(())
    }

    /// The translation, in pixels.
    fn offset(&self) -> (i64, i64) {
        (i64::from(self.state.tx), i64::from(self.state.ty))
    }

    /// Whether every point of `path`, moved by the translation, is inside the safe band.
    fn path_ok(&self, path: &[PathCmd]) -> bool {
        let (tx, ty) = self.offset();
        path.iter().all(|c| {
            let pts: &[Point] = match c {
                PathCmd::MoveTo(a) | PathCmd::LineTo(a) => core::slice::from_ref(a),
                PathCmd::QuadTo(a, b) => &[*a, *b],
                PathCmd::CubicTo(a, b, c) => &[*a, *b, *c],
                PathCmd::Close => &[],
            };
            pts.iter()
                .all(|p| in_band(i64::from(p.x) + tx) && in_band(i64::from(p.y) + ty))
        })
    }

    /// Paints an outline's coverage in `color`, clipped.
    fn coverage(
        &mut self,
        outline: &fill::Outline,
        rule: fill::Rule,
        color: Color,
    ) -> Result<(), Error<T::Error>> {
        let clip = self.state.clip;
        if clip.is_empty() || outline.is_empty() {
            return Ok(());
        }
        let (x0, y0) = (i64::from(clip.x), i64::from(clip.y));
        let bounds = (x0, y0, x0 + i64::from(clip.w), y0 + i64::from(clip.h));
        let value = rgb565(color);
        let opacity = u32::from(color.a);
        let mut result = Ok(());
        let mut buf = [0u8; 256];
        fill::rasterize(outline, rule, bounds, |y, x, alpha| {
            if result.is_err() {
                return;
            }
            let n = alpha.len().min(256);
            for (b, &a) in buf.iter_mut().zip(&alpha[..n]) {
                *b = (u32::from(a) * opacity / 255) as u8;
            }
            result = self.blend(x as i32, y as i32, &buf[..n], value);
        })
        .map_err(|_| Error::TooComplex)?;
        result
    }

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
        let mut prev: Option<char> = None;
        for ch in text.chars() {
            let Some(g) = atlas.glyph_or_replacement(ch) else {
                continue;
            };
            // The pair's kerning, as a shaper would apply it.
            if let Some(p) = prev {
                pen += i64::from(atlas.kern(p, ch));
            }
            prev = Some(ch);
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

/// `rect` with corners of `rad` (1/256 px), cut to half its shorter side.
fn round_rect(rect: Rect, rad: i64) -> cover::RoundRect {
    let (x, y) = (i64::from(rect.x), i64::from(rect.y));
    cover::RoundRect::new(x, y, x + i64::from(rect.w), y + i64::from(rect.h), rad)
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
        // The icons are `fusuma-icons`': Lucide, from the atlases of the icons the app uses.
        #[cfg(feature = "icons")]
        fusuma_render::Font::Icons => return fusuma_icons::atlas::for_size(style.size),
        #[cfg(not(feature = "icons"))]
        fusuma_render::Font::Icons => return None,
        fusuma_render::Font::Bitmap => return None,
    };
    fusuma_fonts::atlas::for_face(set, face, style.size)
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
        // Fills, strokes, text and images mix into a target that can blend.
        let blend = self.target.can_blend();
        Capabilities {
            alpha: blend,
            antialiasing: blend,
            // Gradients are bands and shadows are layers: right, not smooth.
            effects: false,
            retains_picture: true,
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

    fn shift_area(&mut self, rect: Rect, dy: i32) -> bool {
        let device = rect.translate(self.state.tx, self.state.ty);
        let surface = Rect::new(0, 0, self.size.w, self.size.h);
        if device.is_empty() || device.intersect(&surface) != device {
            return false;
        }
        self.target
            .shift_rows(device.x, device.y, device.w, device.h, dy)
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
        if radius.min(rect.w / 2).min(rect.h / 2) > 0 || color.a < 255 {
            return self.cover(
                cover::Shape {
                    outer: round_rect(rect, i64::from(radius) * 256),
                    inner: None,
                },
                color,
            );
        }
        // A plain opaque rectangle is clipped exactly, whatever its coordinates.
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
        // A rounded ring: the outer outline less the inner one, which keeps the radius less the width.
        let outer = i64::from(radius) * 256;
        self.cover(
            cover::Shape {
                outer: round_rect(rect, outer),
                inner: Some(round_rect(
                    rect.inset(width),
                    outer - i64::from(width) * 256,
                )),
            },
            color,
        )
    }

    fn fill_circle(&mut self, center: Point, radius: u32, color: Color) -> Result<(), Self::Error> {
        if color.a == 0 || !Self::point_ok(self.shift(center)) || radius > MAX_EXTENT {
            return Ok(());
        }
        // The disc of pixels within `radius` of the centre pixel: a square of side `2r + 1` with corners of half that,
        // so the edge runs half a pixel past the outermost pixel centres.
        let (cx, cy, r) = (i64::from(center.x), i64::from(center.y), i64::from(radius));
        let outer =
            cover::RoundRect::new(cx - r, cy - r, cx + r + 1, cy + r + 1, (2 * r + 1) * 128);
        self.cover(cover::Shape { outer, inner: None }, color)
    }

    fn fill_path(&mut self, path: &[PathCmd], color: Color) -> Result<(), Self::Error> {
        if color.a == 0 || !self.path_ok(path) {
            return Ok(());
        }
        let outline = fill::Outline::fill(path, self.offset()).map_err(|_| Error::TooComplex)?;
        self.coverage(&outline, fill::Rule::EvenOdd, color)
    }

    fn stroke_path(
        &mut self,
        path: &[PathCmd],
        width: u32,
        color: Color,
    ) -> Result<(), Self::Error> {
        if width == 0 || color.a == 0 || !self.path_ok(path) {
            return Ok(());
        }
        let width = width.min(MAX_STROKE);
        let outline =
            fill::Outline::stroke(path, width, self.offset()).map_err(|_| Error::TooComplex)?;
        self.coverage(&outline, fill::Rule::NonZero, color)
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
        let advance =
            i32::try_from(font.character_size.width + font.character_spacing).unwrap_or(i32::MAX);
        let colour = rgb565(style.color);
        self.draw(|t| {
            // Nearly every text is one run of one font; a text with Romanian or Cyrillic letters is a few.
            for run in runs(text, style.size) {
                let x = origin.x.saturating_add(
                    advance.saturating_mul(i32::try_from(run.before).unwrap_or(i32::MAX)),
                );
                let mono = MonoTextStyle::new(run.font, colour);
                Text::with_baseline(run.text, EgPoint::new(x, origin.y), mono, Baseline::Top)
                    .draw(t)
                    .map(drop)?;
            }
            Ok(())
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
        if !Self::point_ok(origin) || self.state.clip.is_empty() {
            return Ok(());
        }
        let clip = self.state.clip;
        let (cx0, cy0) = (i64::from(clip.x), i64::from(clip.y));
        let (cx1, cy1) = (cx0 + i64::from(clip.w), cy0 + i64::from(clip.h));
        let (ox, oy) = (i64::from(origin.x), i64::from(origin.y));
        let (w, h) = (i64::from(image.width()), i64::from(image.height()));
        let (x0, x1) = (ox.max(cx0), (ox + w).min(cx1));
        if x0 >= x1 {
            return Ok(());
        }
        let blend = self.target.can_blend();
        let rgba = image.rgba();
        let mut alpha = [0u8; 64];
        let mut colors = [0u16; 64];
        for y in oy.max(cy0)..(oy + h).min(cy1) {
            let row = ((y - oy) * w) as usize * 4;
            let mut x = x0;
            while x < x1 {
                let n = (x1 - x).min(64) as usize;
                let first = row + (x - ox) as usize * 4;
                for i in 0..n {
                    let px = &rgba[first + i * 4..first + i * 4 + 4];
                    alpha[i] = px[3];
                    colors[i] = embedded_graphics::pixelcolor::IntoStorage::into_storage(rgb565(
                        Color::rgb(px[0], px[1], px[2]),
                    ));
                }
                // Inside the clip, so these fit `i32`.
                let (bx, by) = (x as i32, y as i32);
                if blend {
                    self.target.blend_colors(bx, by, &alpha[..n], &colors[..n]);
                } else {
                    // The target cannot blend: the pixels at least half opaque, solid.
                    let solid = (0..n).filter(|&i| alpha[i] >= 128).map(|i| {
                        embedded_graphics::Pixel(
                            EgPoint::new(bx + i as i32, by),
                            Rgb565::from(embedded_graphics::pixelcolor::raw::RawU16::new(
                                colors[i],
                            )),
                        )
                    });
                    self.target.draw_iter(solid).map_err(Error::Target)?;
                }
                x += n as i64;
            }
        }
        Ok(())
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

    /// A target that cannot read its pixels: drawn one bit a pixel, and the renderer says so.
    struct Opaque(Frame<Vec<u16>>);
    impl embedded_graphics::geometry::OriginDimensions for Opaque {
        fn size(&self) -> embedded_graphics::geometry::Size {
            self.0.size()
        }
    }
    impl DrawTarget for Opaque {
        type Color = Rgb565;
        type Error = core::convert::Infallible;
        fn draw_iter<I: IntoIterator<Item = embedded_graphics::Pixel<Rgb565>>>(
            &mut self,
            p: I,
        ) -> Result<(), Self::Error> {
            self.0.draw_iter(p)
        }
    }
    impl Blend for Opaque {}

    #[test]
    fn capabilities_are_honest() {
        let c = backend(8, 8).capabilities();
        assert!(c.alpha && c.antialiasing, "a frame blends");
        let mut opaque = Embedded::new(Opaque(Frame::new(40, 40, vec![0u16; 1600]).unwrap()));
        let c = opaque.capabilities();
        assert!(!c.alpha && !c.antialiasing, "a target that cannot blend");
        // It still draws the shapes, the pixels at least half covered.
        opaque
            .fill_circle(Point::new(20, 20), 12, Color::WHITE)
            .unwrap();
        opaque
            .fill_rect(Rect::new(0, 0, 10, 10), 0, Color::rgba(255, 255, 255, 100))
            .unwrap();
        let px = opaque.target().0.pixels();
        assert!(px.iter().all(|&p| p == 0 || p == 0xffff), "one bit a pixel");
        let lit = px.iter().filter(|&&p| p == 0xffff).count();
        let disc = core::f64::consts::PI * 12.5 * 12.5;
        assert!(
            (lit as f64 - disc).abs() < 0.08 * disc,
            "{lit} pixels for a disc of {disc:.0}"
        );
        assert_eq!(px[0], 0, "a fill under half opacity is left out");
    }

    /// Anti-aliased shapes against Vello drawing the same: the same ink, in the same place.
    #[test]
    fn smooth_shapes_look_like_vellos() {
        let paint = |r: &mut dyn DynShapes| {
            r.wipe(Color::BLACK);
            r.circle(Point::new(30, 30), 20);
            r.circle(Point::new(75, 22), 5);
            r.rect(Rect::new(100, 8, 70, 40), 12);
            r.ring(Rect::new(8, 60, 90, 36), 14, 2);
            r.ring(Rect::new(110, 60, 60, 36), 18, 4);
        };
        let (w, h) = (180u32, 100u32);
        let mut a = backend(w as u16, h as u16);
        let mut v = fusuma_render_vello_cpu::new(w, h).unwrap();
        paint(&mut a);
        paint(&mut v);
        let grey = |r: &dyn Probe| -> Vec<i64> {
            (0..h as i32)
                .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
                .map(|(x, y)| r.pixel(x, y).map_or(0, |c| i64::from(c.g)))
                .collect()
        };
        let (pa, pv) = (grey(&a), grey(&v));
        let (ia, iv): (i64, i64) = (pa.iter().sum(), pv.iter().sum());
        let diff: i64 = pa.iter().zip(&pv).map(|(x, y)| (x - y).abs()).sum();
        let soft = pa.iter().filter(|&&g| g > 20 && g < 235).count();
        std::println!(
            "ink {ia} vs {iv}, difference {:.2}% of the ink, {soft} soft pixels",
            diff as f64 * 100.0 / iv as f64
        );
        assert!((ia - iv).abs() * 100 < iv, "ink within 1%: {ia} vs {iv}");
        assert!(diff * 100 < iv * 3, "pixels within 3% of the ink: {diff}");
        assert!(soft > 200, "{soft} soft edge pixels");
    }

    /// Draws `paint` on the integer renderer and on Vello and returns (ink ours, ink Vello's, pixel difference, soft
    /// pixels), the ink being the green channel summed.
    fn compare(w: u32, h: u32, paint: impl Fn(&mut dyn DynPaths)) -> (i64, i64, i64, usize) {
        let mut a = backend(w as u16, h as u16);
        let mut v = fusuma_render_vello_cpu::new(w, h).unwrap();
        paint(&mut a);
        paint(&mut v);
        let grey = |r: &dyn Probe| -> Vec<i64> {
            (0..h as i32)
                .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
                .map(|(x, y)| r.pixel(x, y).map_or(0, |c| i64::from(c.g)))
                .collect()
        };
        let (pa, pv) = (grey(&a), grey(&v));
        let diff = pa.iter().zip(&pv).map(|(x, y)| (x - y).abs()).sum();
        let soft = pa.iter().filter(|&&g| g > 20 && g < 235).count();
        (pa.iter().sum(), pv.iter().sum(), diff, soft)
    }

    /// Paths on either renderer.
    trait DynPaths {
        fn wipe(&mut self);
        fn fill(&mut self, path: &[PathCmd]);
        fn stroke(&mut self, path: &[PathCmd], width: u32);
    }
    impl<R: Renderer> DynPaths for R {
        fn wipe(&mut self) {
            Renderer::clear(self, Color::BLACK).unwrap();
        }
        fn fill(&mut self, path: &[PathCmd]) {
            self.fill_path(path, Color::WHITE).unwrap();
        }
        fn stroke(&mut self, path: &[PathCmd], width: u32) {
            self.stroke_path(path, width, Color::WHITE).unwrap();
        }
    }

    fn poly(pts: &[(i32, i32)], close: bool) -> Vec<PathCmd> {
        let mut v: Vec<PathCmd> = pts
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                let p = Point::new(x, y);
                if i == 0 {
                    PathCmd::MoveTo(p)
                } else {
                    PathCmd::LineTo(p)
                }
            })
            .collect();
        if close {
            v.push(PathCmd::Close);
        }
        v
    }

    fn assert_like_vello(name: &str, got: (i64, i64, i64, usize), ink_pct: i64, diff_pct: i64) {
        let (ia, iv, diff, soft) = got;
        std::println!(
            "{name}: ink {ia} vs {iv}, difference {:.2}% of the ink, {soft} soft pixels",
            diff as f64 * 100.0 / iv as f64
        );
        assert!(
            (ia - iv).abs() * 100 <= iv * ink_pct,
            "{name}: ink {ia} vs {iv}"
        );
        assert!(
            diff * 100 <= iv * diff_pct,
            "{name}: pixels differ by {diff} of {iv}"
        );
        assert!(soft > 0, "{name}: no soft edge pixels");
    }

    #[test]
    fn strokes_along_paths_look_like_vellos() {
        // The checkbox tick, a chevron, a slanted line and a closed triangle, flat and curved.
        let tick = poly(&[(6, 18), (12, 25), (26, 8)], false);
        let chevron = poly(&[(6, 6), (18, 20), (6, 34)], false);
        let slant = poly(&[(4, 4), (60, 30)], false);
        let tri = poly(&[(10, 10), (50, 14), (30, 44)], true);
        let curve = [
            PathCmd::MoveTo(Point::new(5, 40)),
            PathCmd::CubicTo(Point::new(20, -10), Point::new(50, 60), Point::new(70, 10)),
        ];
        for (name, path, width) in [
            ("tick", &tick[..], 2),
            ("fat tick", &tick[..], 4),
            ("chevron", &chevron[..], 3),
            ("wide chevron", &chevron[..], 9),
            ("slant", &slant[..], 5),
            ("triangle", &tri[..], 2),
            ("curve", &curve[..], 3),
        ] {
            let got = compare(80, 60, |r| {
                r.wipe();
                r.stroke(path, width);
            });
            assert_like_vello(name, got, 4, 8);
        }
    }

    /// A translucent stroke is one layer: the pieces it is built from (segments, joins, caps) must not add up
    /// where they overlap.
    #[test]
    fn a_translucent_stroke_is_even_where_its_pieces_overlap() {
        let mut b = backend(40, 40);
        let tick = poly(&[(6, 10), (14, 20), (30, 6)], false);
        b.stroke_path(&tick, 6, Color::rgba(255, 255, 255, 128))
            .unwrap();
        let levels: std::collections::BTreeSet<u8> = (0..40)
            .flat_map(|y| (0..40).map(move |x| (x, y)))
            .filter_map(|(x, y)| b.pixel(x, y))
            .map(|c| c.g)
            .filter(|&g| g > 100)
            .collect();
        // Interior pixels are all the same grey: one layer of 50 %, not 75 % in the overlaps.
        let top = *levels.iter().max().unwrap();
        assert!(
            (120..=136).contains(&top),
            "brightest {top}, levels {levels:?}"
        );
    }

    /// A sharp corner is mitred (a square outer corner), a very sharp one is bevelled, as Vello does.
    #[test]
    fn corners_are_mitred_up_to_the_limit() {
        let mut b = backend(60, 60);
        b.stroke_path(
            &poly(&[(10, 20), (30, 20), (30, 40)], false),
            8,
            Color::WHITE,
        )
        .unwrap();
        // The outer corner pixel, which a round or bevelled join leaves out.
        let corner = b.pixel(34, 16).unwrap().g;
        assert!(
            (55..=75).contains(&corner),
            "a quarter of the corner pixel is inside: {corner}"
        );
        // A hairpin turn is cut off flat, not a spike: its tip would run about 20 px past the end.
        let mut b = backend(60, 60);
        b.stroke_path(
            &poly(&[(10, 30), (50, 30), (10, 42)], false),
            6,
            Color::WHITE,
        )
        .unwrap();
        for y in 28..31 {
            assert_eq!(b.pixel(57, y), Some(Color::BLACK), "no spike past the turn");
        }
    }

    #[test]
    fn paths_outside_the_clip_or_far_away_draw_nothing_and_finish() {
        let tick = poly(&[(0, 0), (30, 30), (60, 0)], false);
        let mut b = backend(40, 40);
        b.clip(Rect::new(0, 0, 20, 20)).unwrap();
        b.stroke_path(&tick, 5, Color::WHITE).unwrap();
        assert_eq!(b.pixel(30, 25), Some(Color::BLACK), "outside the clip");
        assert_ne!(b.pixel(10, 10), Some(Color::BLACK), "inside the clip");
        let far = poly(&[(-2_000_000, 5), (2_000_000, 30)], false);
        let mut b = backend(40, 40);
        b.stroke_path(&far, 3, Color::WHITE).unwrap();
        b.fill_path(
            &poly(
                &[(-9_000_000, -9_000_000), (9_000_000, 0), (0, 9_000_000)],
                true,
            ),
            Color::WHITE,
        )
        .unwrap();
        // Too many edges is an error, not a crash.
        let zigzag: Vec<(i32, i32)> = (0..400)
            .map(|i| (i % 40, if i % 2 == 0 { 2 } else { 30 }))
            .collect();
        let mut b = backend(40, 40);
        assert!(
            b.stroke_path(&poly(&zigzag, false), 1, Color::WHITE)
                .is_err()
        );
        assert!(b.fill_path(&poly(&zigzag, true), Color::WHITE).is_err());
    }

    /// An image with alpha mixes into what is under it, as Vello's does, and is cut by the clip.
    #[test]
    fn images_with_alpha_blend_like_vellos() {
        // 24 x 16: red to blue across, opacity 0 to 255 down.
        let (iw, ih) = (24u32, 16u32);
        let mut rgba = Vec::new();
        for y in 0..ih {
            for x in 0..iw {
                rgba.extend_from_slice(&[(255 - x * 10) as u8, 40, (x * 10) as u8, (y * 17) as u8]);
            }
        }
        let img = Image::new(iw, ih, &rgba).unwrap();
        let (w, h) = (48u32, 32u32);
        let mut a = backend(w as u16, h as u16);
        let mut v = fusuma_render_vello_cpu::new(w, h).unwrap();
        fn go<R: Renderer>(r: &mut R, img: &Image<'_>) {
            r.clear(Color::rgb(200, 200, 40)).unwrap();
            r.draw_image(img, Point::new(4, 6)).unwrap();
            r.save().unwrap();
            r.clip(Rect::new(30, 0, 10, 32)).unwrap();
            r.draw_image(img, Point::new(26, 10)).unwrap();
            r.restore().unwrap();
        }
        go(&mut a, &img);
        go(&mut v, &img);
        let mut worst = 0i32;
        for y in 0..h as i32 {
            for x in 0..w as i32 {
                let (p, q) = (a.pixel(x, y).unwrap(), v.pixel(x, y).unwrap());
                for (m, n) in [(p.r, q.r), (p.g, q.g), (p.b, q.b)] {
                    worst = worst.max((i32::from(m) - i32::from(n)).abs());
                }
            }
        }
        // RGB565 keeps 5 or 6 bits, so a channel can be off by about 8, and rounding adds a little.
        assert!(worst <= 16, "worst channel difference {worst}");
        // Half-opaque pixels are a mix, not on or off.
        let mid = a.pixel(10, 6 + 8).unwrap();
        assert!(mid.r != 200 && mid.r != 255 - 60, "{mid:?}");
        // Cut by the clip: nothing right of it.
        assert_eq!(
            a.pixel(41, 14),
            Some(Color::from_rgb565(
                rgb565(Color::rgb(200, 200, 40)).into_storage()
            ))
        );
    }

    #[test]
    fn filled_paths_look_like_vellos() {
        let tri = poly(&[(10, 50), (70, 40), (30, 6)], true);
        let arrow = poly(
            &[
                (5, 20),
                (30, 20),
                (30, 8),
                (55, 30),
                (30, 52),
                (30, 40),
                (5, 40),
            ],
            true,
        );
        let blob = [
            PathCmd::MoveTo(Point::new(10, 30)),
            PathCmd::QuadTo(Point::new(30, -10), Point::new(60, 25)),
            PathCmd::CubicTo(Point::new(75, 60), Point::new(20, 70), Point::new(10, 30)),
            PathCmd::Close,
        ];
        let thin = poly(&[(2, 2), (70, 6), (2, 8)], true);
        for (name, path) in [
            ("triangle", &tri[..]),
            ("arrow", &arrow[..]),
            ("blob", &blob[..]),
            ("sliver", &thin[..]),
        ] {
            let got = compare(80, 60, |r| {
                r.wipe();
                r.fill(path);
            });
            assert_like_vello(name, got, 3, 9);
        }
    }

    /// The few shapes the comparison draws, on either renderer.
    trait DynShapes {
        fn wipe(&mut self, c: Color);
        fn circle(&mut self, at: Point, r: u32);
        fn rect(&mut self, r: Rect, radius: u32);
        fn ring(&mut self, r: Rect, radius: u32, width: u32);
    }
    impl<R: Renderer> DynShapes for R {
        fn wipe(&mut self, c: Color) {
            Renderer::clear(self, c).unwrap();
        }
        fn circle(&mut self, at: Point, r: u32) {
            self.fill_circle(at, r, Color::WHITE).unwrap();
        }
        fn rect(&mut self, r: Rect, radius: u32) {
            self.fill_rect(r, radius, Color::WHITE).unwrap();
        }
        fn ring(&mut self, r: Rect, radius: u32, width: u32) {
            self.stroke_rect(r, radius, width, Color::WHITE).unwrap();
        }
    }

    #[test]
    fn fonts_follow_the_requested_size_and_measure_by_glyph() {
        assert_eq!(font_for(0).character_size.height, 10);
        assert_eq!(font_for(12).character_size.height, 10);
        assert_eq!(font_for(13).character_size.height, 15);
        assert_eq!(font_for(17).character_size.height, 15);
        assert_eq!(font_for(18).character_size.height, 20);
        assert_eq!(font_for(u32::MAX).character_size.height, 20);
        assert_eq!(text_size(font_for(20), "abc"), Size::new(30, 20));
        assert_eq!(text_size(font_for(20), ""), Size::new(0, 0));
        assert_eq!(
            text_size(font_for(20), "é"),
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

    /// Kerning: pairs such as "AV" and "To" sit closer, as a shaper sets them, so a line is as wide as Vello's.
    #[test]
    fn kerned_text_is_as_wide_as_vellos() {
        let set = fusuma_fonts::FontSet::default_set();
        let mut closer = 0;
        for (text, size) in [
            ("AVATAR", 20u32),
            ("To We Vo", 20),
            ("Typography AV", 15),
            ("WAVE Yo", 20),
            ("Tel. 02-AV", 10),
        ] {
            let mut a = screen(200, 40);
            a.set_fonts(set);
            let mut v = fusuma_render_vello_cpu::new(200, 40).unwrap();
            v.set_fonts(set);
            let style = TextStyle::new(size, Color::WHITE).font(Font::Sans);
            let (wa, wv) = (
                a.measure_text(text, &style).w,
                v.measure_text(text, &style).w,
            );
            // What it would be with no kerning: the advances alone.
            let atlas =
                fusuma_fonts::atlas::for_face(&set, fusuma_fonts::Face::Sans, size).unwrap();
            let plain: u32 = text
                .chars()
                .filter_map(|c| atlas.glyph_or_replacement(c))
                .map(|g| u32::from(g.advance))
                .sum::<u32>()
                .div_ceil(64);
            std::println!("{text:?} {size}: atlas {wa}, without kerning {plain}, Vello {wv}");
            assert!(
                wa < plain,
                "{text:?}: kerning closes a pair ({wa} against {plain})"
            );
            assert!(
                wa.abs_diff(wv) <= 2,
                "{text:?} at {size}: {wa} px against Vello's {wv}"
            );
            if wa.abs_diff(wv) < plain.abs_diff(wv) {
                closer += 1;
            }
        }
        assert_eq!(closer, 5, "every line is nearer Vello's with kerning");
    }

    /// An icon, drawn by the integer renderer from its atlas and by Vello from the font's outlines: the same ink in the
    /// same square (the line's), at every size.
    #[cfg(feature = "icons")]
    #[test]
    fn icons_fill_their_square_like_vellos() {
        use fusuma_icons::Icon;
        for (icon, size) in [
            (Icon::Settings, 20u32),
            (Icon::House, 15),
            (Icon::Wifi, 10),
            (Icon::Zap, 20),
            (Icon::Trash2, 15),
            (Icon::Search, 10),
            (Icon::ChevronDown, 20),
        ] {
            let mut text = [0u8; 4];
            let text = icon.glyph().encode_utf8(&mut text);
            let (w, h) = (40u32, 40u32);
            let (mut a, mut v) = (
                screen(w as u16, h as u16),
                fusuma_render_vello_cpu::new(w, h).unwrap(),
            );
            paint(&mut a, text, Font::Icons, size);
            paint(&mut v, text, Font::Icons, size);
            let (pa, pv) = (grey(&a, w, h), grey(&v, w, h));
            let ink = |p: &[u8]| p.iter().map(|&g| u64::from(g)).sum::<u64>();
            assert!(ink(&pv) > 0, "{icon} {size}: Vello drew nothing");
            let ratio = ink(&pa) as f64 / ink(&pv) as f64;
            assert!(
                (0.75..1.25).contains(&ratio),
                "{icon} {size}: ink {ratio:.3}"
            );
            for y in 0..h {
                for x in 0..w {
                    let inside = (2..2 + size).contains(&x) && (2..2 + size).contains(&y);
                    assert!(
                        inside || pa[(y * w + x) as usize] == 0,
                        "{icon} {size}: ink at ({x}, {y})"
                    );
                }
            }
            let m = a.measure_text(text, &TextStyle::new(size, Color::WHITE).font(Font::Icons));
            assert_eq!((m.w, m.h), (size, size), "{icon}: a square");
            let mv = v.measure_text(text, &TextStyle::new(size, Color::WHITE).font(Font::Icons));
            assert_eq!((mv.w, mv.h), (size, size), "{icon}: Vello's square");
        }
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

#[cfg(all(test, feature = "bitmap-extended", not(feature = "fonts")))]
mod bitmap_extended_tests {
    extern crate std;
    use std::{vec, vec::Vec};

    use super::*;

    #[test]
    fn a_text_in_several_scripts_is_drawn_run_by_run_in_the_right_places() {
        use fusuma_render::{Color, Probe, Renderer};
        let style = TextStyle::new(20, Color::WHITE);
        let mut shot = |text: &str| {
            let mut r = Embedded::new(Frame::new(120, 24, vec![0u16; 120 * 24]).unwrap());
            r.draw_text(text, Point::new(0, 0), &style).unwrap();
            (0..120)
                .map(|x| {
                    (0..24)
                        .filter(|y| r.pixel(x, *y).is_some_and(|c| c != Color::BLACK))
                        .count()
                })
                .collect::<Vec<_>>()
        };
        // Each character takes ten pixels whatever font it comes from, so a letter in the middle is where it would be anyway.
        let (plain, ro) = (shot("a?c"), shot("așc"));
        assert_eq!(plain[..10], ro[..10], "the a is the same");
        assert_eq!(
            plain[20..30],
            ro[20..30],
            "and the c, after a character from another font"
        );
        assert_ne!(plain[10..20], ro[10..20], "the ș is not a question mark");
        assert!(ro[10..20].iter().any(|n| *n > 0), "it has ink");
        // Cyrillic and Greek too.
        for t in ["Ж", "λ"] {
            assert_ne!(shot("?")[..10], shot(t)[..10], "{t}");
        }
        assert_eq!(
            Embedded::new(Frame::new(120, 24, vec![0u16; 120 * 24]).unwrap())
                .measure_text("aș日", &style)
                .w,
            30
        );
    }
}
