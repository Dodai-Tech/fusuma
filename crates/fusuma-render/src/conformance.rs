//! The test-suite every backend must pass, so that they all draw the same picture.
//!
//! ```ignore
//! #[test]
//! fn conforms() {
//!     fusuma_render::conformance::run_all(|| MyBackend::new(64, 64));
//! }
//! ```
//!
//! `make` must give a fresh renderer at least 64x64 pixels big. Colors are compared with a
//! tolerance, because a 16-bit display cannot hold every 8-bit value. Alpha and anti-aliasing
//! are only checked when [`Capabilities`] say the backend has them.

use crate::color::Color;
use crate::geometry::{Point, Rect};
use crate::image::Image;
use crate::path::{PathCmd, triangle};
use crate::renderer::{Capabilities, MIN_STATE_DEPTH, Probe, Renderer};
use crate::text::{Align, TextStyle};

const BLACK: Color = Color::BLACK;
const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: Color = Color::rgb(0, 0, 255);
const WHITE: Color = Color::WHITE;
/// How far a channel may be off (RGB565 steps are up to 8 wide).
const TOLERANCE: u8 = 10;

fn near(a: Color, b: Color) -> bool {
    a.r.abs_diff(b.r) <= TOLERANCE
        && a.g.abs_diff(b.g) <= TOLERANCE
        && a.b.abs_diff(b.b) <= TOLERANCE
}

#[track_caller]
fn is<R: Probe>(r: &R, x: i32, y: i32, want: Color, what: &str) {
    let got = r
        .pixel(x, y)
        .unwrap_or_else(|| panic!("{what}: pixel ({x}, {y}) is outside the surface"));
    assert!(
        near(got, want),
        "{what}: pixel ({x}, {y}) is {got:?}, want {want:?}"
    );
}

fn fresh<R: Renderer + Probe>(make: &impl Fn() -> R) -> R {
    let mut r = make();
    let s = r.size();
    assert!(
        s.w >= 64 && s.h >= 64,
        "the surface must be at least 64x64, is {}x{}",
        s.w,
        s.h
    );
    r.clear(BLACK).expect("clear");
    r
}

/// Runs every check on fresh renderers from `make`; panics on the first difference.
///
/// # Panics
/// When the backend does not draw what the trait promises.
pub fn run_all<R: Renderer + Probe>(make: impl Fn() -> R) {
    rect_fills_inside_only(&make);
    rounded_corners_are_cut(&make);
    circle_covers_its_disc(&make);
    stroked_rect_lies_inside(&make);
    filled_path(&make);
    stroked_path(&make);
    clip_limits_drawing(&make);
    clip_limits_clear(&make);
    translate_moves_the_origin(&make);
    save_and_restore(&make);
    clips_nest_and_translate_first(&make);
    state_stack_depth(&make);
    text_is_drawn_inside_its_measure(&make);
    text_alignment(&make);
    empty_text_and_clipped_text(&make);
    images(&make);
    optional_alpha(&make);
    optional_antialiasing(&make);
    gradients(&make);
    shadows(&make);
}

fn rect_fills_inside_only<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.fill_rect(Rect::new(10, 10, 20, 10), 0, RED)
        .expect("fill_rect");
    for (x, y) in [(10, 10), (29, 19), (20, 15)] {
        is(&r, x, y, RED, "rect inside");
    }
    for (x, y) in [(9, 10), (30, 10), (10, 9), (10, 20), (0, 0)] {
        is(&r, x, y, BLACK, "rect outside");
    }
    r.fill_rect(Rect::new(40, 40, 0, 10), 0, RED)
        .expect("empty rect");
    is(&r, 40, 45, BLACK, "an empty rect draws nothing");
}

fn rounded_corners_are_cut<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.fill_rect(Rect::new(8, 8, 40, 40), 12, RED)
        .expect("rounded");
    is(&r, 8, 8, BLACK, "rounded corner");
    is(&r, 47, 47, BLACK, "rounded corner");
    is(&r, 28, 28, RED, "rounded center");
    is(&r, 28, 8, RED, "rounded top edge");
    is(&r, 8, 28, RED, "rounded left edge");
    // A radius bigger than the rectangle is clamped, not an error.
    r.fill_rect(Rect::new(0, 50, 10, 10), 1000, GREEN)
        .expect("clamped radius");
    is(&r, 5, 55, GREEN, "clamped radius center");
}

fn circle_covers_its_disc<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.fill_circle(Point::new(32, 32), 10, RED).expect("circle");
    for (x, y) in [(32, 32), (32, 24), (24, 32), (40, 32), (32, 40)] {
        is(&r, x, y, RED, "circle inside");
    }
    for (x, y) in [(32, 46), (46, 32), (22, 22), (42, 42), (32, 17)] {
        is(&r, x, y, BLACK, "circle outside");
    }
}

fn stroked_rect_lies_inside<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.stroke_rect(Rect::new(10, 10, 30, 20), 0, 3, RED)
        .expect("stroke_rect");
    for (x, y) in [(10, 10), (11, 20), (25, 10), (25, 28), (38, 20)] {
        is(&r, x, y, RED, "stroke");
    }
    is(&r, 25, 20, BLACK, "inside the stroke");
    is(&r, 9, 20, BLACK, "outside the rect");
    is(&r, 25, 31, BLACK, "outside the rect");
    is(&r, 25, 13, BLACK, "just inside the 3 px stroke");
}

fn filled_path<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    let t = triangle(Point::new(10, 50), Point::new(50, 50), Point::new(30, 10));
    r.fill_path(&t, RED).expect("fill_path");
    is(&r, 30, 37, RED, "triangle center");
    is(&r, 30, 15, RED, "triangle apex");
    for (x, y) in [(12, 12), (48, 12), (30, 54), (5, 40), (56, 40)] {
        is(&r, x, y, BLACK, "outside the triangle");
    }
    // An open path is closed, and two sub-paths fill both shapes.
    let mut r = fresh(make);
    let two = [
        PathCmd::MoveTo(Point::new(2, 2)),
        PathCmd::LineTo(Point::new(20, 2)),
        PathCmd::LineTo(Point::new(20, 20)),
        PathCmd::LineTo(Point::new(2, 20)),
        PathCmd::MoveTo(Point::new(30, 30)),
        PathCmd::LineTo(Point::new(50, 30)),
        PathCmd::LineTo(Point::new(50, 50)),
        PathCmd::LineTo(Point::new(30, 50)),
        PathCmd::Close,
    ];
    r.fill_path(&two, GREEN).expect("two sub-paths");
    is(&r, 10, 10, GREEN, "first square");
    is(&r, 40, 40, GREEN, "second square");
    is(&r, 25, 25, BLACK, "between the squares");
    // A quadratic and a cubic curve stay inside their control hull.
    let mut r = fresh(make);
    let curve = [
        PathCmd::MoveTo(Point::new(10, 50)),
        PathCmd::QuadTo(Point::new(30, 0), Point::new(50, 50)),
        PathCmd::CubicTo(Point::new(40, 60), Point::new(20, 60), Point::new(10, 50)),
        PathCmd::Close,
    ];
    r.fill_path(&curve, BLUE).expect("curves");
    is(&r, 30, 45, BLUE, "inside the curves");
    is(&r, 30, 5, BLACK, "above the arch");
    is(&r, 5, 30, BLACK, "left of the shape");
}

fn stroked_path<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    let line = [
        PathCmd::MoveTo(Point::new(10, 10)),
        PathCmd::LineTo(Point::new(50, 10)),
    ];
    r.stroke_path(&line, 3, RED).expect("stroke_path");
    is(&r, 30, 10, RED, "line");
    is(&r, 30, 20, BLACK, "away from the line");
    is(&r, 30, 5, BLACK, "away from the line");
    let mut r = fresh(make);
    let vertical = [
        PathCmd::MoveTo(Point::new(20, 10)),
        PathCmd::LineTo(Point::new(20, 50)),
    ];
    r.stroke_path(&vertical, 5, BLUE).expect("vertical");
    is(&r, 20, 30, BLUE, "vertical line");
    is(&r, 30, 30, BLACK, "away from the vertical line");
}

fn clip_limits_drawing<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.save().expect("save");
    r.clip(Rect::new(20, 20, 10, 10)).expect("clip");
    r.fill_rect(Rect::new(0, 0, 64, 64), 0, RED).expect("fill");
    r.restore().expect("restore");
    is(&r, 25, 25, RED, "inside the clip");
    is(&r, 20, 20, RED, "clip corner");
    for (x, y) in [(19, 25), (30, 25), (25, 19), (25, 30), (0, 0), (63, 63)] {
        is(&r, x, y, BLACK, "outside the clip");
    }
    let mut r = fresh(make);
    r.save().expect("save");
    r.clip(Rect::new(-50, -50, 40, 40))
        .expect("off-screen clip");
    r.fill_rect(Rect::new(0, 0, 64, 64), 0, RED).expect("fill");
    r.restore().expect("restore");
    is(&r, 5, 5, BLACK, "a clip off the surface hides everything");
}

fn clip_limits_clear<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.save().expect("save");
    r.clip(Rect::new(10, 10, 10, 10)).expect("clip");
    r.clear(BLUE).expect("clear");
    r.restore().expect("restore");
    is(&r, 15, 15, BLUE, "cleared inside the clip");
    is(&r, 5, 5, BLACK, "clear stops at the clip");
    is(&r, 20, 20, BLACK, "clear stops at the clip");
}

fn translate_moves_the_origin<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.save().expect("save");
    r.translate(10, 5).expect("translate");
    r.fill_rect(Rect::new(0, 0, 5, 5), 0, RED).expect("fill");
    r.restore().expect("restore");
    for (x, y) in [(10, 5), (14, 9)] {
        is(&r, x, y, RED, "translated");
    }
    for (x, y) in [(9, 5), (0, 0), (15, 9), (10, 10)] {
        is(&r, x, y, BLACK, "not translated");
    }
    let mut r = fresh(make);
    r.save().expect("save");
    r.translate(10, 10).expect("translate");
    r.translate(5, -3).expect("translate again");
    r.fill_rect(Rect::new(0, 0, 2, 2), 0, GREEN).expect("fill");
    r.restore().expect("restore");
    is(&r, 15, 7, GREEN, "translations add up");
    is(&r, 10, 10, BLACK, "translations add up");
}

fn save_and_restore<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.save().expect("save");
    r.translate(20, 20).expect("translate");
    r.clip(Rect::new(0, 0, 5, 5)).expect("clip");
    r.restore().expect("restore");
    r.fill_rect(Rect::new(0, 0, 64, 64), 0, RED).expect("fill");
    is(&r, 0, 0, RED, "translation undone");
    is(&r, 40, 40, RED, "clip undone");
    assert!(r.restore().is_err(), "restore without save must fail");
}

fn clips_nest_and_translate_first<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    r.save().expect("save");
    r.clip(Rect::new(10, 10, 30, 30)).expect("outer clip");
    r.save().expect("save");
    r.clip(Rect::new(20, 20, 30, 30)).expect("inner clip");
    r.fill_rect(Rect::new(0, 0, 64, 64), 0, RED)
        .expect("fill inner");
    r.restore().expect("restore inner");
    r.fill_rect(Rect::new(0, 0, 10, 10), 0, BLUE)
        .expect("fill outer");
    r.fill_rect(Rect::new(12, 12, 4, 4), 0, BLUE)
        .expect("fill outer");
    r.restore().expect("restore outer");
    is(&r, 25, 25, RED, "nested clips overlap");
    is(&r, 45, 45, BLACK, "outside both");
    is(&r, 15, 15, BLUE, "the outer clip is back after restore");
    is(&r, 5, 5, BLACK, "outer clip still applies");

    // A clip is given in the translated space.
    let mut r = fresh(make);
    r.save().expect("save");
    r.translate(10, 10).expect("translate");
    r.clip(Rect::new(0, 0, 5, 5)).expect("clip");
    r.fill_rect(Rect::new(-100, -100, 500, 500), 0, RED)
        .expect("fill");
    r.restore().expect("restore");
    is(&r, 12, 12, RED, "clip after translate");
    is(&r, 9, 9, BLACK, "clip after translate");
    is(&r, 16, 16, BLACK, "clip after translate");
}

fn state_stack_depth<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    for i in 0..MIN_STATE_DEPTH {
        r.save()
            .unwrap_or_else(|e| panic!("save {i} of {MIN_STATE_DEPTH} failed: {e:?}"));
    }
    let mut extra = 0;
    while r.save().is_ok() {
        extra += 1;
        assert!(extra < 10_000, "the stack never reports being full");
    }
    for _ in 0..MIN_STATE_DEPTH + extra {
        r.restore().expect("restore every save");
    }
    assert!(r.restore().is_err(), "the stack is empty again");
}

fn lit_in<R: Probe>(r: &R, area: Rect) -> usize {
    let mut n = 0;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if r.pixel(x, y)
                .is_some_and(|c| c.r > 100 || c.g > 100 || c.b > 100)
            {
                n += 1;
            }
        }
    }
    n
}

fn text_is_drawn_inside_its_measure<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    let style = TextStyle::new(20, WHITE);
    let size = r.measure_text("Hi", &style);
    assert!(size.w > 0 && size.h > 0, "measure_text gave {size:?}");
    assert!(
        r.measure_text("Hello", &style).w > size.w,
        "longer text is wider"
    );
    assert!(
        r.measure_text("Hi", &TextStyle::new(10, WHITE)).h <= size.h,
        "a smaller size is not taller"
    );
    r.draw_text("Hi", Point::new(10, 10), &style)
        .expect("draw_text");
    let area = Rect::new(10, 10, size.w, size.h);
    assert!(
        lit_in(&r, area) >= 5,
        "the text lights some pixels inside its box"
    );
    assert_eq!(
        lit_in(&r, Rect::new(0, 0, 64, 9)),
        0,
        "nothing above the text"
    );
    assert_eq!(
        lit_in(&r, Rect::new(0, 0, 9, 64)),
        0,
        "nothing left of the text"
    );
    let to_the_right = Rect::new(
        area.right() + 1,
        0,
        64u32.saturating_sub(area.right() as u32 + 1),
        64,
    );
    assert_eq!(lit_in(&r, to_the_right), 0, "nothing right of the text box");
}

fn text_alignment<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let style = TextStyle::new(20, WHITE);
    let width = fresh(make).measure_text("Hi", &style).w as i32;
    let mut r = fresh(make);
    r.draw_text("Hi", Point::new(40, 10), &style.aligned(Align::Center))
        .expect("center");
    assert!(
        lit_in(&r, Rect::new(0, 0, 40, 64)) > 0,
        "centered text has pixels left of the anchor"
    );
    assert!(
        lit_in(&r, Rect::new(40, 0, 24, 64)) > 0,
        "centered text has pixels right of the anchor"
    );
    let mut r = fresh(make);
    r.draw_text("Hi", Point::new(60, 10), &style.aligned(Align::Right))
        .expect("right");
    assert_eq!(
        lit_in(&r, Rect::new(61, 0, 3, 64)),
        0,
        "right-aligned text ends at the anchor"
    );
    assert!(
        lit_in(&r, Rect::new(60 - width, 10, width as u32, 20)) > 0,
        "right-aligned text is left of the anchor"
    );
}

fn empty_text_and_clipped_text<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    let style = TextStyle::new(20, WHITE);
    assert_eq!(r.measure_text("", &style).w, 0, "empty text has no width");
    r.draw_text("", Point::new(5, 5), &style)
        .expect("empty text");
    assert_eq!(
        lit_in(&r, Rect::new(0, 0, 64, 64)),
        0,
        "empty text draws nothing"
    );
    r.save().expect("save");
    r.clip(Rect::new(0, 0, 12, 64)).expect("clip");
    r.draw_text("Hello", Point::new(5, 10), &style)
        .expect("clipped text");
    r.restore().expect("restore");
    assert!(
        lit_in(&r, Rect::new(0, 0, 12, 64)) > 0,
        "the visible part is drawn"
    );
    assert_eq!(
        lit_in(&r, Rect::new(12, 0, 52, 64)),
        0,
        "the clip hides the rest"
    );
}

fn images<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    #[rustfmt::skip]
    let data = [
        255, 0, 0, 255,   0, 255, 0, 255,
        0, 0, 255, 255,   255, 255, 255, 0,
    ];
    let img = Image::new(2, 2, &data).expect("image");
    r.draw_image(&img, Point::new(10, 10)).expect("draw_image");
    is(&r, 10, 10, RED, "image pixel");
    is(&r, 11, 10, GREEN, "image pixel");
    is(&r, 10, 11, BLUE, "image pixel");
    is(
        &r,
        11,
        11,
        BLACK,
        "a transparent pixel leaves the target alone",
    );
    is(&r, 12, 10, BLACK, "outside the image");
    is(&r, 9, 10, BLACK, "outside the image");
    r.save().expect("save");
    r.translate(30, 30).expect("translate");
    r.draw_image(&img, Point::new(1, 1))
        .expect("translated image");
    r.restore().expect("restore");
    is(&r, 31, 31, RED, "images follow the translation");
    let empty = Image::new(0, 0, &[]).expect("empty image");
    r.draw_image(&empty, Point::new(0, 0))
        .expect("an empty image is fine");
    r.draw_image(&img, Point::new(-1, 0))
        .expect("partly off the surface is fine");
    r.draw_image(&img, Point::new(1000, 1000))
        .expect("fully off the surface is fine");
    is(&r, 0, 0, GREEN, "the visible part of a partly hidden image");
    is(&r, 0, 1, BLACK, "its transparent pixel");
}

fn optional_alpha<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    let caps: Capabilities = r.capabilities();
    r.fill_rect(Rect::new(0, 0, 20, 20), 0, RED).expect("base");
    r.fill_rect(Rect::new(0, 0, 20, 20), 0, Color::rgba(255, 255, 255, 128))
        .expect("overlay");
    r.fill_rect(Rect::new(30, 0, 10, 10), 0, Color::TRANSPARENT)
        .expect("transparent");
    is(&r, 35, 5, BLACK, "a fully transparent fill draws nothing");
    if caps.alpha {
        let c = r.pixel(10, 10).expect("pixel");
        assert!(
            c.r >= 245 && (100..=160).contains(&c.g) && (100..=160).contains(&c.b),
            "50% white over red is pink, is {c:?}"
        );
    }
}

fn optional_antialiasing<R: Renderer + Probe>(make: &impl Fn() -> R) {
    let mut r = fresh(make);
    if !r.capabilities().antialiasing {
        return;
    }
    r.fill_circle(Point::new(32, 32), 14, RED).expect("circle");
    let mut between = 0;
    for y in 16..48 {
        for x in 16..48 {
            let c = r.pixel(x, y).expect("pixel");
            if c.r > 30 && c.r < 225 && c.g < 30 && c.b < 30 {
                between += 1;
            }
        }
    }
    assert!(
        between >= 8,
        "an anti-aliased circle has soft edge pixels, found {between}"
    );
}

/// Within `tol` of `want` on every colour channel.
fn within(c: Color, want: Color, tol: u8) -> bool {
    c.r.abs_diff(want.r) <= tol && c.g.abs_diff(want.g) <= tol && c.b.abs_diff(want.b) <= tol
}

fn gradients<R: Renderer + Probe>(make: &impl Fn() -> R) {
    use crate::effects::Gradient;
    // Vertical: black at the top to white at the bottom of a 40 high box; nothing outside it.
    let mut r = fresh(make);
    r.fill_gradient(
        Rect::new(10, 10, 20, 40),
        0,
        Gradient::vertical(BLACK, Color::WHITE),
    )
    .expect("gradient");
    assert!(
        within(r.pixel(20, 10).expect("top"), BLACK, 12),
        "the top is the start colour"
    );
    assert!(
        within(r.pixel(20, 49).expect("bottom"), Color::WHITE, 12),
        "the bottom is the end colour"
    );
    let mut last = 0u8;
    for y in 10..50 {
        let c = r.pixel(20, y).expect("pixel");
        assert!(
            c.r >= last.saturating_sub(2),
            "the gradient only gets lighter: {c:?} at {y}"
        );
        assert!(
            c.r.abs_diff(c.g) <= 9 && c.g.abs_diff(c.b) <= 9,
            "stays gray: {c:?}"
        );
        last = c.r;
    }
    let mid = r.pixel(20, 30).expect("middle");
    assert!(
        (100..=156).contains(&mid.r),
        "about half way in the middle: {mid:?}"
    );
    assert_eq!(
        r.pixel(20, 10),
        r.pixel(10, 10),
        "the same colour along a row"
    );
    for (x, y) in [(9, 30), (30, 30), (20, 9), (20, 50)] {
        is(&r, x, y, BLACK, "outside the gradient");
    }
    // Horizontal: red to blue.
    let mut r = fresh(make);
    r.fill_gradient(Rect::new(0, 0, 40, 10), 0, Gradient::horizontal(RED, BLUE))
        .expect("horizontal");
    assert!(
        within(r.pixel(0, 5).expect("left"), RED, 12)
            && within(r.pixel(39, 5).expect("right"), BLUE, 12)
    );
    assert_eq!(
        r.pixel(10, 0),
        r.pixel(10, 9),
        "the same colour down a column"
    );
    assert!(
        r.pixel(20, 5).expect("m").r < r.pixel(5, 5).expect("l").r,
        "less red going right"
    );
    // Round corners are cut, the middle of the edges is not.
    let mut r = fresh(make);
    r.fill_gradient(Rect::new(8, 8, 40, 40), 12, Gradient::vertical(RED, RED))
        .expect("rounded");
    is(&r, 8, 8, BLACK, "rounded corner of a gradient");
    is(&r, 47, 47, BLACK, "rounded corner of a gradient");
    is(&r, 28, 28, RED, "rounded centre");
    is(&r, 28, 8, RED, "top edge");
    // Nothing to draw, and nothing to fail on.
    let mut r = fresh(make);
    r.fill_gradient(Rect::new(5, 5, 0, 9), 3, Gradient::vertical(RED, BLUE))
        .expect("empty");
    r.fill_gradient(Rect::new(5, 5, 9, 0), 3, Gradient::vertical(RED, BLUE))
        .expect("empty");
    is(&r, 5, 5, BLACK, "an empty gradient draws nothing");
    // Clipped like everything else.
    r.save().expect("save");
    r.clip(Rect::new(0, 0, 10, 10)).expect("clip");
    r.fill_gradient(Rect::new(0, 0, 40, 40), 0, Gradient::vertical(RED, RED))
        .expect("clipped");
    r.restore().expect("restore");
    is(&r, 5, 5, RED, "inside the clip");
    is(&r, 20, 20, BLACK, "outside the clip");
}

fn shadows<R: Renderer + Probe>(make: &impl Fn() -> R) {
    use crate::effects::Shadow;
    let mut r = fresh(make);
    let alpha = r.capabilities().alpha;
    let rect = Rect::new(20, 20, 24, 20);
    r.draw_shadow(rect, 4, Shadow::drop(8, 4, 200))
        .expect("shadow");
    // Black on black cannot be seen: draw it on white.
    let mut r = fresh(make);
    r.clear(Color::WHITE).expect("clear");
    r.draw_shadow(rect, 4, Shadow::drop(8, 4, 200))
        .expect("shadow");
    if !alpha {
        is(&r, 32, 32, Color::WHITE, "no alpha, no shadow");
        return;
    }
    // Darkest under the box, softer away from it, nothing far away; and lower than the box by the offset.
    let under = r.pixel(32, 34).expect("under").r;
    assert!(under < 120, "the middle of the shadow is dark: {under}");
    let near_edge = r.pixel(32, 44).expect("edge").r;
    let far = r.pixel(32, 60).expect("far").r;
    assert!(far > 250, "far from the box there is no shadow: {far}");
    assert!(
        under < near_edge && near_edge < far,
        "it fades out: {under} {near_edge} {far}"
    );
    // The offset: just inside the box's bottom edge is darker than just inside its top edge (the shadow is 4 lower).
    let top = r.pixel(32, 22).expect("top").r;
    let bottom = r.pixel(32, 38).expect("bottom").r;
    assert!(
        bottom + 8 < top,
        "shifted down: near the top {top}, near the bottom {bottom}"
    );
    // Left and right are alike (no horizontal offset).
    let (l, rt) = (r.pixel(14, 30).expect("l").r, r.pixel(49, 30).expect("r").r);
    assert!(l.abs_diff(rt) <= 12, "{l} {rt}");
    // Clipped like everything else, and an invisible shadow draws nothing.
    let mut r = fresh(make);
    r.clear(Color::WHITE).expect("clear");
    r.save().expect("save");
    r.clip(Rect::new(0, 0, 10, 10)).expect("clip");
    r.draw_shadow(rect, 4, Shadow::drop(8, 4, 200))
        .expect("clipped");
    r.restore().expect("restore");
    r.draw_shadow(rect, 4, Shadow::drop(8, 4, 0))
        .expect("invisible");
    is(
        &r,
        32,
        34,
        Color::WHITE,
        "a shadow outside the clip, and an invisible one",
    );
}
