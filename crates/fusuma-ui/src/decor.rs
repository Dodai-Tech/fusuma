//! Decoration a theme asks for besides colours and shapes: corner ticks, a diagonal hatch, a dotted track and a
//! progress bar that runs round the edge of a frame. All integers and straight lines, so it draws the same on every
//! renderer, and each piece has a plain form for a screen that shows no greys (see [`Decor::flat`]).

use fusuma_render::{Color, PathCmd, Point, Rect, Renderer};

/// Which decorations a theme draws. A length of 0 switches that one off, so [`Decor::NONE`] draws nothing and every
/// other theme is unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decor {
    /// Length in pixels of each arm of the corner ticks drawn on panels, cards and dialogs; 0 for none.
    pub ticks: u32,
    /// Distance in pixels between the lines of the diagonal hatch on the chosen item or tab; 0 for a plain fill.
    pub hatch: u32,
    /// Distance in pixels between the dots of the unfilled part of a progress bar; 0 for a plain track.
    pub dots: u32,
    /// A progress widget that has room on all four sides (taller than twice the track) is drawn round its edge,
    /// clockwise from the top left, instead of along its middle.
    pub frame: bool,
}

impl Decor {
    /// No decoration: every widget is as the tokens say.
    pub const NONE: Self = Self {
        ticks: 0,
        hatch: 0,
        dots: 0,
        frame: false,
    };

    /// This decoration for a glass that shows no greys (e-paper, a memory LCD): what is made of lines (ticks, the frame)
    /// stays, what needs a fine pattern (hatch, dots) becomes a solid fill.
    #[must_use]
    pub const fn flat(self) -> Self {
        Self {
            hatch: 0,
            dots: 0,
            ..self
        }
    }
}

/// The two arms of a tick at each corner of `r`, `arm` long and `w` thick, inside the rectangle.
pub(crate) fn ticks<R: Renderer>(
    r: &mut R,
    rect: Rect,
    arm: u32,
    w: u32,
    color: Color,
) -> Result<(), R::Error> {
    if arm == 0 || rect.w < arm.saturating_mul(2) || rect.h < arm.saturating_mul(2) {
        return Ok(());
    }
    let w = w.max(1);
    let (x0, y0) = (rect.x, rect.y);
    let x1 = rect.x + (rect.w - arm) as i32;
    let y1 = rect.y + (rect.h - arm) as i32;
    let xe = rect.x + (rect.w - w) as i32;
    let ye = rect.y + (rect.h - w) as i32;
    // Horizontal arm then vertical arm at each of the four corners.
    for ((hx, hy), (vx, vy)) in [
        ((x0, y0), (x0, y0)),
        ((x1, y0), (xe, y0)),
        ((x0, ye), (x0, y1)),
        ((x1, ye), (xe, y1)),
    ] {
        r.fill_rect(Rect::new(hx, hy, arm, w), 0, color)?;
        r.fill_rect(Rect::new(vx, vy, w, arm), 0, color)?;
    }
    Ok(())
}

/// Lines at 45 degrees, `step` pixels apart, inside `rect`.
pub(crate) fn hatch<R: Renderer>(
    r: &mut R,
    rect: Rect,
    step: u32,
    color: Color,
) -> Result<(), R::Error> {
    if step == 0 || rect.is_empty() {
        return Ok(());
    }
    let (w, h) = (rect.w as i32, rect.h as i32);
    let step = step as i32;
    let mut k = 0;
    // Each line runs from the bottom-left towards the top-right and is cut to the rectangle by the clip.
    while k < w + h {
        let a = Point::new(rect.x + k, rect.y);
        let b = Point::new(rect.x + k - h, rect.y + h);
        r.save()?;
        r.clip(rect)?;
        let drawn = r.stroke_path(&[PathCmd::MoveTo(a), PathCmd::LineTo(b)], 1, color);
        r.restore()?;
        drawn?;
        k += step;
    }
    Ok(())
}

/// The unfilled part of a track as dots: a 1 pixel column every `step` pixels, `height` tall.
pub(crate) fn dotted<R: Renderer>(
    r: &mut R,
    track: Rect,
    from: i32,
    step: u32,
    color: Color,
) -> Result<(), R::Error> {
    let step = step.max(2) as i32;
    let mut x = from;
    while x < track.right() {
        r.fill_rect(Rect::new(x, track.y, 1, track.h), 0, color)?;
        x += step;
    }
    Ok(())
}

/// A progress bar round the edge of `rect`: the track all the way round in `track`, then `permille` thousandths of the
/// way round clockwise from the top left in `fill`, `t` pixels thick.
pub(crate) fn frame_progress<R: Renderer>(
    r: &mut R,
    rect: Rect,
    permille: u16,
    t: u32,
    fill: Color,
    track: Color,
) -> Result<(), R::Error> {
    let t = t.max(1).min(rect.w / 2).min(rect.h / 2);
    if t == 0 {
        return Ok(());
    }
    r.stroke_rect(rect, 0, t, track)?;
    let (w, h) = (i64::from(rect.w), i64::from(rect.h));
    let total = 2 * (w + h) - 4 * i64::from(t);
    let mut left = total * i64::from(permille.min(1000)) / 1000;
    let (x, y, t) = (i64::from(rect.x), i64::from(rect.y), i64::from(t));
    // (start x, start y, direction x, direction y, length) of the four sides, clockwise.
    let sides = [
        (x, y, 1, 0, w),
        (x + w - t, y + t, 0, 1, h - t),
        (x + w - t - 1, y + h - t, -1, 0, w - t),
        (x, y + h - t - 1, 0, -1, h - 2 * t),
    ];
    for (sx, sy, dx, dy, len) in sides {
        if left <= 0 {
            break;
        }
        let n = left.min(len);
        left -= n;
        // The run of `n` pixels from the start, as one rectangle.
        let (rx, ry, rw, rh) = match (dx, dy) {
            (1, _) => (sx, sy, n, t),
            (-1, _) => (sx - n + 1, sy, n, t),
            (_, 1) => (sx, sy, t, n),
            _ => (sx, sy - n + 1, t, n),
        };
        r.fill_rect(
            Rect::new(rx as i32, ry as i32, rw as u32, rh as u32),
            0,
            fill,
        )?;
    }
    Ok(())
}
