//! Scrolling by moving the pixels already drawn. A container that only scrolled shows nearly the same picture, a few
//! rows higher or lower: so a renderer that can ([`Renderer::shift_area`]) moves what it has, and the frame draws only
//! the strip that came into view, the scrollbar and whatever else changed. The cost follows the strip, not the window.
//!
//! The tree watches for a scroll that is safe to do this way and keeps the damage in two parts: the *present*
//! region (what the platform sends to the screen: the whole window, since the screen needs the moved pixels) and
//! the *rest* (what really changed besides the move). Any other change that touches the window cancels the shift and
//! the window is redrawn, so a picture is never wrong, only sometimes slower.

use fusuma_render::{Rect, Renderer};

use crate::theme::Theme;
use crate::tree::{Flags, Kind, NodeId, Tree};

/// A scroll container whose pixels may be moved instead of redrawn.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShiftState {
    pub scroll: NodeId,
    /// Where the content was on the screen (offset plus overshoot) when the first change since the last frame began.
    pub from: i32,
}

/// What to do for a frame in which a container was shifted.
struct Plan {
    /// The rows that move, and by how much (0 when it ended up where it began).
    window: Rect,
    dy: i32,
    /// The parts to draw anew: the strip that came in, the scrollbar, and the bands the rounded corners need.
    parts: [Option<Rect>; 4],
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    let o = a.intersect(b);
    o.w > 0 && o.h > 0
}

impl<const N: usize> Tree<N> {
    /// Gives up a shift in progress: the whole window is redrawn.
    pub(crate) fn cancel_shift(&mut self) {
        let Some(s) = self.shift.take() else { return };
        self.shift_laid = false;
        if let Some(i) = self.slot(s.scroll) {
            let seen = self.visible_rect(i);
            self.add_damage(seen);
        }
    }

    /// Whether node `i` is inside the container that is being shifted: its move is part of the shift, not a change.
    pub(crate) fn in_shift(&self, i: usize) -> bool {
        let Some(s) = self.shift else { return false };
        let Some(sc) = self.slot(s.scroll) else {
            return false;
        };
        let mut cur = self.nodes[i].parent.map(usize::from);
        while let Some(p) = cur {
            if p == sc {
                return true;
            }
            cur = self.nodes[p].parent.map(usize::from);
        }
        false
    }

    /// A container scrolled from `old_pos`: marks it for layout and paint the cheap way if the pixels can be moved,
    /// else the usual way.
    pub(crate) fn note_scroll(&mut self, i: usize, old_pos: i32) {
        self.wake_bar(i);
        if self.try_shift(i, old_pos) {
            // Marked without damage: the moved rows are not a change, only the strip is.
            self.request_quiet(i, Flags::PLACE | Flags::PAINT);
        } else {
            self.request(i, Flags::PLACE | Flags::PAINT);
        }
    }

    /// Starts (or continues) a shift of container `i`, from where it was drawn. False when the picture under it
    /// cannot be trusted to move: something there is already waiting to be redrawn, or a layout is pending.
    fn try_shift(&mut self, i: usize, old_pos: i32) -> bool {
        let id = self.id_of(i);
        match self.shift {
            Some(s) if s.scroll == id => return true,
            Some(_) => {
                self.cancel_shift();
                return false;
            }
            None => {}
        }
        let rect = self.nodes[i].rect;
        let root_layout = self
            .root
            .is_some_and(|r| self.nodes[usize::from(r)].sub.has(Flags::LAYOUT));
        if rect.w == 0
            || rect.h == 0
            || root_layout
            || self.modal().is_some()
            || self.damage.is_some_and(|d| overlaps(&d, &rect))
        {
            return false;
        }
        self.shift = Some(ShiftState {
            scroll: id,
            from: old_pos,
        });
        self.shift_laid = false;
        // The screen has to be sent the whole window, because the pixels in it moved.
        let seen = self.visible_rect(i);
        self.add_present_damage(seen);
        true
    }

    /// Marks node `i` and its ancestors as needing `flag` without recording any damage.
    pub(crate) fn request_quiet(&mut self, i: usize, flag: u8) {
        self.nodes[i].own.insert(flag);
        let mut cur = Some(i);
        while let Some(c) = cur {
            self.nodes[c].sub.insert(flag);
            cur = self.nodes[c].parent.map(usize::from);
        }
    }

    /// Where the shifted container ended up, and what has to be drawn anew. `None` when moving the pixels is not
    /// worth it or not possible (it moved a window's height or more).
    fn shift_plan(&self, s: ShiftState, theme: &Theme) -> Option<Plan> {
        let i = self.slot(s.scroll)?;
        let n = &self.nodes[i];
        let Kind::Scroll { offset, over, .. } = n.kind else {
            return None;
        };
        let moved = i64::from(offset) + i64::from(over) - i64::from(s.from);
        let dy = i32::try_from(-moved).ok()?;
        // Rows are clipped to the container's whole rectangle, padding included, so that is what moves. Rounded
        // corners sit at its top and bottom edge: the bands they cover are drawn anew instead of moved.
        let r = n.rect;
        let guard = theme.radius(n.style.round);
        let g = i32::try_from(guard).unwrap_or(0);
        if r.h <= guard.saturating_mul(2) {
            return None;
        }
        let window = Rect::new(r.x, r.y + g, r.w, r.h - guard * 2);
        if dy.unsigned_abs() >= window.h {
            return None;
        }
        let mut parts = [None; 4];
        if dy > 0 {
            parts[0] = Some(Rect::new(window.x, window.y, window.w, dy.unsigned_abs()));
        } else if dy < 0 {
            let by = dy.unsigned_abs();
            parts[0] = Some(Rect::new(
                window.x,
                window.bottom() - by as i32,
                window.w,
                by,
            ));
        }
        // The scrollbar is drawn over the rows, so its old position has to be cleaned up and the new one drawn.
        parts[1] = Some(crate::scrollbar::bar_column(r, theme));
        if guard > 0 {
            parts[2] = Some(Rect::new(r.x, r.y, r.w, guard));
            parts[3] = Some(Rect::new(r.x, r.bottom() - g, r.w, guard));
        }
        Some(Plan { window, dy, parts })
    }

    /// Lays out nothing and draws what changed since the last frame, onto `renderer`, which is cleared to the theme's
    /// background where it draws. Returns the rectangle to show on the screen (the platform sends that), or `None`
    /// when nothing changed. A container that only scrolled has its pixels moved if the renderer can
    /// ([`Renderer::shift_area`]); everything else is redrawn in the changed parts only. Call
    /// [`update`](Self::update) first.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn paint_frame<R: Renderer>(
        &mut self,
        renderer: &mut R,
        theme: &Theme,
        screen: Rect,
    ) -> Result<Option<Rect>, R::Error> {
        let shift = self.shift.take();
        self.shift_laid = false;
        let rest = self.rest.take();
        let pieces = self.pieces;
        self.pieces.clear();
        self.drawn.clear();
        let Some(present) = self
            .damage
            .take()
            .map(|d| d.intersect(&screen))
            .filter(|d| d.w > 0 && d.h > 0)
        else {
            return Ok(None);
        };
        // The parts to draw: the whole changed area, unless the pixels of a scroll could be moved.
        let mut draws = [None; 6];
        draws[0] = Some(present);
        // Several separate changes are drawn apart, so what lies between them is left alone.
        if pieces.as_slice().len() > 1 {
            for (slot, part) in draws.iter_mut().zip(pieces.as_slice()) {
                *slot = Some(part.intersect(&screen)).filter(|p| p.w > 0 && p.h > 0);
            }
        }
        if let Some(plan) = shift.and_then(|s| self.shift_plan(s, theme)) {
            let moved = plan.dy == 0 || renderer.shift_area(plan.window, plan.dy);
            if moved {
                draws = [None; 6];
                for (slot, part) in draws.iter_mut().zip(plan.parts.iter().chain([&rest])) {
                    *slot = part
                        .map(|p| p.intersect(&screen))
                        .filter(|p| p.w > 0 && p.h > 0);
                }
            }
        }
        let mut result = Ok(());
        for part in draws.into_iter().flatten() {
            self.drawn.add(part);
            renderer.will_draw(part);
            result = result.and_then(|()| renderer.save());
            result = result.and_then(|()| renderer.clip(part));
            result = result.and_then(|()| renderer.clear(theme.colors.background));
            if result.is_ok() {
                result = self.paint_region(renderer, theme, part);
            }
            let restored = renderer.restore();
            result = result.and(restored);
        }
        result?;
        Ok(Some(present))
    }

    /// Whether a shift is waiting to be drawn, for the tests.
    #[cfg(test)]
    pub(crate) fn shift_active(&self) -> bool {
        self.shift.is_some()
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Color, Point, Probe, Rect, Renderer, Size, TextStyle};
    use fusuma_render_embedded::{Embedded, Frame};
    use fusuma_render_vello_cpu::{Scene, VelloCpu};

    use crate::event::Pointer;
    use crate::theme::{Round, Surface, Theme};
    use crate::tree::{CrossAlign, Length, NodeId, Style, Tree};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    /// What the same tests need of a renderer beyond [`Renderer`]: a new one, and its start and end of a frame.
    trait Back: Renderer + Probe {
        fn make(w: u32, h: u32) -> Self;
        /// The start of a platform's frame.
        fn begin(&mut self);
        /// The end of a frame, after which a platform reads its pixels (`drew`: the frame drew something).
        fn after(&mut self, drew: bool);
        /// A platform that does not know `paint_frame` hints the region it was told.
        fn hint(&mut self, region: Rect);
    }

    impl Back for VelloCpu {
        fn make(w: u32, h: u32) -> Self {
            fusuma_render_vello_cpu::new(w, h).unwrap()
        }
        fn begin(&mut self) {
            self.with_scene(Scene::reset);
        }
        fn after(&mut self, drew: bool) {
            // Vello renders when the pixels are first read; a platform does that at the end of every frame that
            // drew. After a frame that drew nothing, reading them must not render the empty scene over the picture.
            if drew {
                let _ = self.pixel(0, 0);
            } else {
                self.with_scene(|sc| Scene::add_damage(sc, (0, 0, 0, 0)));
            }
        }
        fn hint(&mut self, region: Rect) {
            self.with_scene(|s| Scene::set_damage(s, (region.x, region.y, region.w, region.h)));
        }
    }

    type Integer = Embedded<Frame<Vec<u16>>>;

    impl Back for Integer {
        fn make(w: u32, h: u32) -> Self {
            Embedded::new(Frame::new(w as u16, h as u16, vec![0u16; (w * h) as usize]).unwrap())
        }
        fn begin(&mut self) {}
        fn after(&mut self, _drew: bool) {}
        fn hint(&mut self, _region: Rect) {}
    }

    /// Every test below, on both renderers: the toolkit's scroll shift must be exact on each.
    macro_rules! on_both {
        ($($name:ident),* $(,)?) => {
            mod vello_cpu { $(#[test] fn $name() { super::$name::<super::VelloCpu>(); })* }
            mod integer { $(#[test] fn $name() { super::$name::<super::Integer>(); })* }
        };
    }

    const W: u32 = 160;
    const H: u32 = 220;

    /// A renderer that records what the toolkit asked of it, and can refuse to shift.
    struct Spy<B: Back> {
        inner: B,
        allow: bool,
        shifts: Vec<(Rect, i32)>,
        draws: Vec<Rect>,
    }

    impl<B: Back> Spy<B> {
        fn new(allow: bool) -> Self {
            Self {
                inner: B::make(W, H),
                allow,
                shifts: Vec::new(),
                draws: Vec::new(),
            }
        }

        /// The start of a platform's frame: the scene forgets what it drew before.
        fn begin(&mut self) {
            self.inner.begin();
            self.shifts.clear();
            self.draws.clear();
        }
    }

    impl<B: Back> Renderer for Spy<B> {
        type Error = <B as Renderer>::Error;
        fn size(&self) -> Size {
            self.inner.size()
        }
        fn capabilities(&self) -> fusuma_render::Capabilities {
            self.inner.capabilities()
        }
        fn save(&mut self) -> Result<(), Self::Error> {
            self.inner.save()
        }
        fn restore(&mut self) -> Result<(), Self::Error> {
            self.inner.restore()
        }
        fn clip(&mut self, rect: Rect) -> Result<(), Self::Error> {
            self.inner.clip(rect)
        }
        fn translate(&mut self, dx: i32, dy: i32) -> Result<(), Self::Error> {
            self.inner.translate(dx, dy)
        }
        fn clear(&mut self, color: Color) -> Result<(), Self::Error> {
            self.inner.clear(color)
        }
        fn fill_rect(&mut self, rect: Rect, radius: u32, color: Color) -> Result<(), Self::Error> {
            self.inner.fill_rect(rect, radius, color)
        }
        fn stroke_rect(
            &mut self,
            rect: Rect,
            radius: u32,
            width: u32,
            color: Color,
        ) -> Result<(), Self::Error> {
            self.inner.stroke_rect(rect, radius, width, color)
        }
        fn fill_circle(
            &mut self,
            center: Point,
            radius: u32,
            color: Color,
        ) -> Result<(), Self::Error> {
            self.inner.fill_circle(center, radius, color)
        }
        fn fill_path(
            &mut self,
            path: &[fusuma_render::PathCmd],
            color: Color,
        ) -> Result<(), Self::Error> {
            self.inner.fill_path(path, color)
        }
        fn stroke_path(
            &mut self,
            path: &[fusuma_render::PathCmd],
            width: u32,
            color: Color,
        ) -> Result<(), Self::Error> {
            self.inner.stroke_path(path, width, color)
        }
        fn draw_text(
            &mut self,
            text: &str,
            at: Point,
            style: &TextStyle,
        ) -> Result<(), Self::Error> {
            self.inner.draw_text(text, at, style)
        }
        fn measure_text(&self, text: &str, style: &TextStyle) -> Size {
            self.inner.measure_text(text, style)
        }
        fn draw_image(
            &mut self,
            image: &fusuma_render::Image<'_>,
            at: Point,
        ) -> Result<(), Self::Error> {
            self.inner.draw_image(image, at)
        }
        fn shift_area(&mut self, rect: Rect, dy: i32) -> bool {
            if !self.allow {
                return false;
            }
            self.shifts.push((rect, dy));
            self.inner.shift_area(rect, dy)
        }
        fn will_draw(&mut self, rect: Rect) {
            self.draws.push(rect);
            self.inner.will_draw(rect);
        }
    }

    impl<B: Back> Probe for Spy<B> {
        fn pixel(&self, x: i32, y: i32) -> Option<Color> {
            self.inner.pixel(x, y)
        }
    }

    struct Ui {
        t: Tree<48>,
        sc: NodeId,
    }

    /// A page: a header, a window of `rows` rows (with the style given) and a footer. The window is 120 px high.
    fn ui<B: Back>(rows: usize, window: Style) -> Ui {
        let mut t = Tree::<48>::new();
        let root = t
            .column(None, Style::new().gap(6).align(CrossAlign::Stretch))
            .unwrap();
        t.label(Some(root), "Header", crate::theme::TextRole::Title)
            .unwrap();
        let sc = t.scroll(Some(root), window).unwrap();
        for k in 0..rows {
            let name = [
                "alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india",
                "juliet", "kilo", "lima",
            ][k % 12];
            t.list_item(Some(sc), name, k == 2).unwrap();
        }
        t.label(Some(root), "Footer", crate::theme::TextRole::Caption)
            .unwrap();
        let mut u = Ui { t, sc };
        u.t.update(&Spy::<B>::new(true), &THEME, Rect::new(0, 0, W, H));
        u
    }

    fn plain() -> Style {
        Style::new()
            .height(Length::Fixed(120))
            .gap(4)
            .align(CrossAlign::Stretch)
    }

    fn frame<B: Back>(u: &mut Ui, r: &mut Spy<B>) -> Option<Rect> {
        r.begin();
        u.t.update(&*r, &THEME, Rect::new(0, 0, W, H));
        let shown = u.t.paint_frame(r, &THEME, Rect::new(0, 0, W, H)).unwrap();
        r.inner.after(shown.is_some());
        shown
    }

    /// A reference picture of the tree as it is now, drawn all at once.
    fn full<B: Back>(u: &mut Ui) -> Spy<B> {
        let mut r = Spy::<B>::new(false);
        r.begin();
        u.t.update(&r, &THEME, Rect::new(0, 0, W, H));
        r.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut r, &THEME).unwrap();
        r
    }

    fn same<B: Back>(a: &Spy<B>, b: &Spy<B>, what: &str) {
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                assert_eq!(a.pixel(x, y), b.pixel(x, y), "{what}: ({x}, {y})");
            }
        }
    }

    /// Scrolls a window by `dy` pixels with the wheel and checks the picture against a full redraw.
    fn wheel_then_compare<B: Back>(window: Style, dy: i32) -> (Spy<B>, Option<Rect>) {
        let mut u = ui::<B>(10, window);
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r).unwrap();
        let first = u.t.rect(u.sc).unwrap();
        u.t.pointer(Pointer::Wheel(Point::new(first.x + 20, first.y + 20), dy));
        let shown = frame(&mut u, &mut r);
        let reference = full(&mut u);
        same(&r, &reference, &format!("dy {dy}"));
        (r, shown)
    }

    fn a_scroll_moves_the_pixels_and_redraws_only_the_strip_and_the_scrollbar<B: Back>() {
        let (r, shown) = wheel_then_compare::<B>(plain(), 13);
        let window = Rect::new(0, 0, W, H);
        let shown = shown.unwrap();
        assert!(shown.w * shown.h <= window.w * window.h, "{shown:?}");
        assert_eq!(
            r.shifts.len(),
            1,
            "the pixels were moved once: {:?}",
            r.shifts
        );
        assert_eq!(r.shifts[0].1, -13, "up by the distance scrolled");
        let area = |rects: &[Rect]| {
            rects
                .iter()
                .map(|d| u64::from(d.w) * u64::from(d.h))
                .sum::<u64>()
        };
        let win = u64::from(r.shifts[0].0.w) * u64::from(r.shifts[0].0.h);
        assert!(
            area(&r.draws) * 4 < win,
            "only a strip was drawn: {:?} of a {win} px window",
            r.draws
        );
    }

    fn the_picture_is_exactly_a_full_redraw_for_every_distance_and_direction<B: Back>() {
        for dy in [1, 2, 7, 30, 59, 100, 119, 121, 400] {
            wheel_then_compare::<B>(plain(), dy);
        }
    }

    fn scrolling_back_up_and_across_many_frames_stays_exact<B: Back>() {
        let mut u = ui::<B>(12, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        let wheel = Point::new(at.x + 20, at.y + 20);
        for (k, dy) in [9, 9, -4, 17, -30, 5, 80, -80, 3].into_iter().enumerate() {
            u.t.pointer(Pointer::Wheel(wheel, dy));
            frame(&mut u, &mut r);
            same(&r, &full(&mut u), &format!("step {k}"));
        }
    }

    fn several_scrolls_before_one_frame_are_one_move<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        let wheel = Point::new(at.x + 20, at.y + 20);
        for dy in [4, 6, -2, 5] {
            u.t.pointer(Pointer::Wheel(wheel, dy));
        }
        frame(&mut u, &mut r);
        assert_eq!(
            r.shifts,
            vec![(r.shifts[0].0, -13)],
            "4 + 6 - 2 + 5 = 13 in one move"
        );
        same(&r, &full(&mut u), "combined");
    }

    fn a_renderer_that_cannot_shift_gets_the_whole_window_drawn_and_the_same_picture<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(false);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        u.t.pointer(Pointer::Wheel(Point::new(at.x + 20, at.y + 20), 13));
        let shown = frame(&mut u, &mut r).unwrap();
        assert!(
            r.shifts.is_empty() && r.draws == vec![shown],
            "{:?} {:?}",
            r.draws,
            shown
        );
        same(&r, &full(&mut u), "no shift");
    }

    fn a_change_inside_the_window_cancels_the_shift_and_the_window_is_redrawn<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        u.t.pointer(Pointer::Wheel(Point::new(at.x + 20, at.y + 20), 13));
        // Something in the window changes in the same frame: a row's text.
        let row = u.t.children(u.sc).next().unwrap();
        u.t.set_text(row, "changed!");
        let shown = frame(&mut u, &mut r).unwrap();
        assert!(r.shifts.is_empty(), "no move: the window is not trusted");
        assert!(
            shown.h >= at.h,
            "the whole window is shown: {shown:?} for {at:?}"
        );
        same(&r, &full(&mut u), "cancelled");
    }

    fn a_change_before_the_scroll_over_the_window_means_no_shift_either<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let row = u.t.children(u.sc).nth(1).unwrap();
        u.t.set_text(row, "edited");
        let at = u.t.rect(u.sc).unwrap();
        u.t.pointer(Pointer::Wheel(Point::new(at.x + 20, at.y + 20), 13));
        frame(&mut u, &mut r).unwrap();
        assert!(r.shifts.is_empty());
        same(&r, &full(&mut u), "dirty first");
    }

    fn a_change_elsewhere_is_drawn_next_to_the_shifted_window<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        u.t.pointer(Pointer::Wheel(Point::new(at.x + 20, at.y + 20), 13));
        let footer = u.t.rect(u.t.root().unwrap()).map(|_| ()).and(Some(()));
        let _ = footer;
        let last = u.t.children(u.t.root().unwrap()).last().unwrap();
        u.t.set_text(last, "Updated footer");
        frame(&mut u, &mut r);
        assert_eq!(r.shifts.len(), 1, "the scroll was still a move");
        let foot = u.t.rect(last).unwrap();
        assert!(
            r.draws
                .iter()
                .any(|d| d.y >= foot.y - 1 && d.bottom() >= foot.bottom()),
            "{:?} for {foot:?}",
            r.draws
        );
        same(&r, &full(&mut u), "footer");
    }

    fn rounded_corners_at_the_edge_of_the_window_are_not_moved_with_the_rows<B: Back>() {
        let style = plain().background(Surface::Surface).round(Round::Large);
        for dy in [3_i32, 20, -20] {
            let mut u = ui::<B>(10, style);
            let mut r = Spy::<B>::new(true);
            frame(&mut u, &mut r);
            if dy < 0 {
                u.t.scroll_to(u.sc, 60);
                frame(&mut u, &mut r);
            }
            let at = u.t.rect(u.sc).unwrap();
            u.t.pointer(Pointer::Wheel(Point::new(at.x + 20, at.y + 20), dy));
            frame(&mut u, &mut r);
            same(&r, &full(&mut u), &format!("rounded, dy {dy}"));
        }
    }

    fn padding_and_a_background_do_not_break_the_move<B: Back>() {
        let style = plain()
            .padding(10)
            .background(Surface::Surface)
            .round(Round::Medium);
        for dy in [5, 33, 90] {
            wheel_then_compare::<B>(style, dy);
        }
    }

    fn a_touch_drag_and_a_fling_are_moves_too_and_stay_exact<B: Back>() {
        let mut u = ui::<B>(12, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        let x = at.x + 30;
        let mut y = at.y + 100;
        u.t.pointer(Pointer::Down(Point::new(x, y)));
        for _ in 0..6 {
            y -= 11;
            u.t.pointer(Pointer::Move(Point::new(x, y)));
            u.t.advance(&THEME, 16);
            frame(&mut u, &mut r);
            same(&r, &full(&mut u), "drag");
        }
        u.t.pointer(Pointer::Up(Point::new(x, y)));
        let mut moved = 0;
        for k in 0..60 {
            if !u.t.advance(&THEME, 16) && !u.t.needs_frame() {
                break;
            }
            frame(&mut u, &mut r);
            moved += r.shifts.len();
            same(&r, &full(&mut u), &format!("fling {k}"));
        }
        assert!(moved > 3, "the fling frames were moves: {moved}");
        assert_eq!(u.t.scroll_over(u.sc), Some(0));
    }

    fn a_pull_past_the_end_and_the_bounce_back_stay_exact<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        u.t.pointer(Pointer::Down(Point::new(at.x + 30, at.y + 20)));
        for k in 1..=5 {
            u.t.pointer(Pointer::Move(Point::new(at.x + 30, at.y + 20 + k * 14)));
            frame(&mut u, &mut r);
            same(&r, &full(&mut u), "pull");
        }
        u.t.pointer(Pointer::Up(Point::new(at.x + 30, at.y + 90)));
        for k in 0..200 {
            let moving = u.t.advance(&THEME, 16);
            frame(&mut u, &mut r);
            same(&r, &full(&mut u), &format!("bounce {k}"));
            if !moving && !u.t.needs_frame() {
                break;
            }
        }
        assert_eq!(u.t.scroll_over(u.sc), Some(0));
    }

    fn the_old_paint_calls_still_work_after_a_scroll<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let at = u.t.rect(u.sc).unwrap();
        u.t.pointer(Pointer::Wheel(Point::new(at.x + 20, at.y + 20), 13));
        // A platform that does not know `paint_frame`: it takes the damage and paints the region.
        r.begin();
        u.t.update(&r, &THEME, Rect::new(0, 0, W, H));
        let region = u.t.take_damage_in(Rect::new(0, 0, W, H)).unwrap();
        assert!(!u.t.shift_active(), "taking the damage gives up the shift");
        r.save().unwrap();
        r.clip(region).unwrap();
        r.clear(THEME.colors.background).unwrap();
        u.t.paint_region(&mut r, &THEME, region).unwrap();
        r.restore().unwrap();
        // What a platform that does not know `paint_frame` does: it hints the region it was told.
        r.inner.hint(region);
        same(&r, &full(&mut u), "old API");
    }

    fn two_distant_changes_are_drawn_apart_and_the_picture_stays_exact<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        frame(&mut u, &mut r);
        let root = u.t.root().unwrap();
        let first = u.t.children(root).next().unwrap();
        let last = u.t.children(root).last().unwrap();
        u.t.set_text(first, "Head!");
        u.t.set_text(last, "Foot!");
        let shown = frame(&mut u, &mut r).unwrap();
        let rects = u.t.frame_rects().to_vec();
        assert_eq!(rects.len(), 2, "{rects:?}");
        let area: u64 = rects.iter().map(|d| u64::from(d.w) * u64::from(d.h)).sum();
        assert!(
            area * 2 < u64::from(shown.w) * u64::from(shown.h),
            "{rects:?} in {shown:?}"
        );
        assert_eq!(r.draws, rects, "each piece was announced and drawn alone");
        same(&r, &full(&mut u), "two pieces");
        // Many more changes than there is room for still draw everything that changed.
        for k in 0..30 {
            let row = u.t.children(u.sc).nth(k % 5).unwrap();
            u.t.set_text(row, if k % 2 == 0 { "odd" } else { "even" });
            u.t.set_text(first, if k % 2 == 0 { "A" } else { "B" });
            u.t.set_text(last, if k % 3 == 0 { "x" } else { "y" });
            frame(&mut u, &mut r);
            assert!(u.t.frame_rects().len() <= crate::MAX_PIECES);
            same(&r, &full(&mut u), &format!("run {k}"));
        }
    }

    fn a_tree_with_nothing_to_do_draws_nothing<B: Back>() {
        let mut u = ui::<B>(10, plain());
        let mut r = Spy::<B>::new(true);
        assert!(frame(&mut u, &mut r).is_some());
        assert_eq!(frame(&mut u, &mut r), None);
        assert!(r.draws.is_empty() && r.shifts.is_empty());
    }

    on_both!(
        two_distant_changes_are_drawn_apart_and_the_picture_stays_exact,
        a_scroll_moves_the_pixels_and_redraws_only_the_strip_and_the_scrollbar,
        the_picture_is_exactly_a_full_redraw_for_every_distance_and_direction,
        scrolling_back_up_and_across_many_frames_stays_exact,
        several_scrolls_before_one_frame_are_one_move,
        a_renderer_that_cannot_shift_gets_the_whole_window_drawn_and_the_same_picture,
        a_change_inside_the_window_cancels_the_shift_and_the_window_is_redrawn,
        a_change_before_the_scroll_over_the_window_means_no_shift_either,
        a_change_elsewhere_is_drawn_next_to_the_shifted_window,
        rounded_corners_at_the_edge_of_the_window_are_not_moved_with_the_rows,
        padding_and_a_background_do_not_break_the_move,
        a_touch_drag_and_a_fling_are_moves_too_and_stay_exact,
        a_pull_past_the_end_and_the_bounce_back_stay_exact,
        the_old_paint_calls_still_work_after_a_scroll,
        a_tree_with_nothing_to_do_draws_nothing,
    );
}
