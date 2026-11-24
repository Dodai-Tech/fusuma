//! Scroll physics: the content resists when pulled past an end (a rubber band), goes on by itself after a flick
//! (a fling, slowed by friction), and springs back from an end. Built on `fusuma-anim`'s `Decay`, `Spring` and
//! `rubber_band`, so a container scrolls the way a thumb expects without any app code.

use fusuma_anim::{Decay, Fx, Spring, SpringConfig, rubber_band};

use crate::tree::{Drag, Kind, NodeId, Tree};

/// How fast a fling slows down: the speed fades by this factor each second (4 is a typical scroll).
const FRICTION: i32 = 4;
/// A release slower than this, in pixels a second, is a stop and not a fling.
const MIN_FLING: i32 = 80;
/// The fastest fling, in pixels a second.
const MAX_FLING: i32 = 6000;
/// `Decay` and `Spring` take at most this long a step: a slow frame is run as several, so a watch drawing 5 frames a
/// second still moves in real time.
const CHUNK_MS: u32 = 100;
/// Positions are fixed-point pixels, which hold this much either way.
const LIMIT: i32 = 30_000;

/// What a scroll container does after the finger let go.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Motion {
    /// Going on after a flick, slowing down.
    Fling { scroll: NodeId, decay: Decay },
    /// Springing back from an end.
    Bounce { scroll: NodeId, spring: Spring },
}

impl Motion {
    const fn scroll(&self) -> NodeId {
        match self {
            Self::Fling { scroll, .. } | Self::Bounce { scroll, .. } => *scroll,
        }
    }
}

fn px(v: i32) -> Fx {
    Fx::from_int(v.clamp(-LIMIT, LIMIT))
}

impl<const N: usize> Tree<N> {
    /// The visible height of container `s`, in pixels.
    fn view_h(&self, s: usize) -> i32 {
        i32::try_from(self.nodes[s].rect.inset(self.nodes[s].style.padding).h).unwrap_or(i32::MAX)
    }

    /// Where container `s` shows the content when the finger would have it at `raw` pixels (unclamped): the offset
    /// inside the range, and how far past an end it is pulled, which resists the more it is pulled. A container whose
    /// content fits does not stretch.
    pub(crate) fn rubber(&self, s: usize, raw: i32) -> (i32, i32) {
        let max = self.scroll_max_at(s).clamp(0, LIMIT);
        if max == 0 {
            return (0, 0);
        }
        let shown = rubber_band(px(raw), Fx::ZERO, px(max), px(self.view_h(s))).round();
        let offset = raw.clamp(0, max);
        (offset, shown.clamp(-LIMIT, LIMIT) - offset)
    }

    /// Puts container `s` at `offset` and `over` past an end, redrawing it if that changed anything.
    pub(crate) fn set_motion_offset(&mut self, s: usize, offset: i32, over: i32) {
        let Kind::Scroll {
            offset: o, over: v, ..
        } = &mut self.nodes[s].kind
        else {
            return;
        };
        if *o == offset && *v == over {
            return;
        }
        let before = *o + *v;
        *o = offset;
        *v = over;
        self.note_scroll(s, before);
    }

    /// Stops a fling or a bounce on container `i`, if one is running there.
    pub(crate) fn cancel_motion(&mut self, i: usize) {
        if self
            .motion
            .is_some_and(|m| self.slot(m.scroll()) == Some(i))
        {
            self.motion = None;
        }
    }

    /// A finger came down: stops whatever is moving and says whether something was, so the lift is not a click.
    pub(crate) fn catch_motion(&mut self) -> bool {
        let Some(m) = self.motion.take() else {
            return false;
        };
        if let Some(s) = self.slot(m.scroll()) {
            let offset = self.scroll_offset_at(s);
            self.set_motion_offset(s, offset, 0);
        }
        true
    }

    /// The finger let go of a dragged container: a fling if it was moving, a bounce if it was pulled past an end.
    pub(crate) fn release_scroll(&mut self, d: Drag) {
        let Some(s) = self.slot(d.scroll) else { return };
        let Kind::Scroll { offset, over, .. } = self.nodes[s].kind else {
            return;
        };
        let vel = d.vel.clamp(-MAX_FLING, MAX_FLING);
        self.motion = if over != 0 {
            let mut spring = Spring::new(SpringConfig::SPATIAL, px(over));
            spring.set_target(Fx::ZERO);
            spring.impulse(px(vel / 4));
            Some(Motion::Bounce {
                scroll: d.scroll,
                spring,
            })
        } else if vel.abs() >= MIN_FLING && self.scroll_max_at(s) > 0 {
            Some(Motion::Fling {
                scroll: d.scroll,
                decay: Decay::new(px(offset), px(vel), Fx::from_int(FRICTION)),
            })
        } else {
            None
        };
    }

    /// Steps the scroll physics by `dt_ms`: measures the speed of a finger that is dragging, and moves a fling or a
    /// bounce. True while something is still moving, so the platform keeps asking for frames.
    pub(crate) fn advance_scroll(&mut self, dt_ms: u32) -> bool {
        let mut active = false;
        if let Some(mut d) = self.drag {
            if d.moving {
                if dt_ms > 0 {
                    let inst = (i64::from(d.raw) - i64::from(d.last_raw)) * 1000 / i64::from(dt_ms);
                    d.vel = ((i64::from(d.vel) + inst) / 2)
                        .clamp(-i64::from(MAX_FLING), i64::from(MAX_FLING))
                        as i32;
                    d.last_raw = d.raw;
                }
                // Frames must keep coming while the finger is down, or a finger held still would never slow the
                // measured speed down to nothing.
                active = true;
            } else {
                d.last_raw = d.raw;
            }
            self.drag = Some(d);
        }
        let Some(mut m) = self.motion else {
            return active;
        };
        let Some(s) = self.slot(m.scroll()) else {
            self.motion = None;
            return active;
        };
        // A minute of physics is more than any fling or bounce needs: a huge step is not run chunk by chunk to the end.
        let (mut left, mut running) = (dt_ms.min(60_000), true);
        while left > 0 && running {
            let chunk = left.min(CHUNK_MS);
            left -= chunk;
            match &mut m {
                Motion::Fling { scroll, decay } => {
                    let moving = decay.step(chunk);
                    let pos = decay.position().round();
                    let max = self.scroll_max_at(s);
                    if pos < 0 || pos > max {
                        // Hit an end at speed: stretch a little, then spring back.
                        let edge = pos.clamp(0, max);
                        let reach = self.view_h(s) / 2;
                        let over = (pos - edge).clamp(-reach, reach);
                        let mut spring = Spring::new(SpringConfig::SPATIAL, px(over));
                        spring.set_target(Fx::ZERO);
                        spring.impulse(decay.velocity() / Fx::from_int(4));
                        self.set_motion_offset(s, edge, over);
                        m = Motion::Bounce {
                            scroll: *scroll,
                            spring,
                        };
                    } else {
                        self.set_motion_offset(s, pos, 0);
                        running = moving;
                    }
                }
                Motion::Bounce { spring, .. } => {
                    let moving = spring.step(chunk);
                    let offset = self.scroll_offset_at(s);
                    let over = if moving { spring.position().round() } else { 0 };
                    self.set_motion_offset(s, offset, over);
                    running = moving;
                }
            }
        }
        if running {
            self.motion = Some(m);
            active = true;
        } else {
            self.motion = None;
            let offset = self.scroll_offset_at(s);
            self.set_motion_offset(s, offset, 0);
        }
        active
    }

    /// How far container `id` is pulled past an end, in pixels (negative above the top), by a drag or a bounce; 0
    /// at rest.
    #[must_use]
    pub fn scroll_over(&self, id: NodeId) -> Option<i32> {
        let i = self.slot(id)?;
        match self.nodes[i].kind {
            Kind::Scroll { over, .. } => Some(over),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Point, Rect};
    use fusuma_render_vello_cpu::VelloCpu;

    use crate::event::Pointer;
    use crate::theme::Theme;
    use crate::tree::{ActionKind, CrossAlign, Length, NodeId, Style, Tree};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(200, 300).unwrap()
    }

    struct Ui {
        t: Tree<32>,
        sc: NodeId,
        rows: Vec<NodeId>,
    }

    /// A 100 px window onto `n` rows of 44 px with 4 px between: ten rows make 476 px, so 376 px to scroll.
    fn ui(n: usize) -> Ui {
        let mut t = Tree::<32>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let sc = t
            .scroll(
                Some(root),
                Style::new()
                    .height(Length::Fixed(100))
                    .gap(4)
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        let rows = (0..n)
            .map(|_| t.list_item(Some(sc), "row", false).unwrap())
            .collect();
        t.update(&screen(), &THEME, Rect::new(0, 0, 200, 300));
        Ui { t, sc, rows }
    }

    fn offset(u: &Ui) -> i32 {
        u.t.scroll_offset(u.sc).unwrap()
    }

    fn over(u: &Ui) -> i32 {
        u.t.scroll_over(u.sc).unwrap()
    }

    /// A finger on the window that moves `dy` pixels every 16 ms frame for `frames` frames, then lifts.
    fn flick(u: &mut Ui, dy: i32, frames: i32) {
        let (x, mut y) = (50, 90);
        u.t.pointer(Pointer::Down(Point::new(x, y)));
        for _ in 0..frames {
            y += dy;
            u.t.pointer(Pointer::Move(Point::new(x, y)));
            u.t.advance(&THEME, 16);
        }
        u.t.pointer(Pointer::Up(Point::new(x, y)));
    }

    /// Steps frames of `dt` ms until nothing moves, at most `limit_ms`; the time it took.
    fn settle(u: &mut Ui, dt: u32, limit_ms: u32) -> u32 {
        let mut t = 0;
        while t < limit_ms {
            t += dt;
            u.t.relayout(&screen(), &THEME);
            if !u.t.advance(&THEME, dt) {
                break;
            }
        }
        t
    }

    #[test]
    fn pulling_past_the_top_resists_more_and_more_and_never_passes_the_window() {
        let mut u = ui(10);
        u.t.pointer(Pointer::Down(Point::new(50, 50)));
        u.t.pointer(Pointer::Move(Point::new(50, 150)));
        let first = over(&u);
        assert!(first < 0 && first > -100, "pulled 100 px, shown {first}");
        assert!(-first < 100, "resists: less than the pull");
        u.t.pointer(Pointer::Move(Point::new(50, 450)));
        let far = over(&u);
        assert!(
            far < first && -far < 100,
            "a longer pull stretches further but never past the window: {far}"
        );
        assert!(
            first - far < 300,
            "and the extra 300 px of pull gave far less than 300"
        );
        assert_eq!(offset(&u), 0);
    }

    #[test]
    fn letting_go_past_an_end_springs_back_to_rest() {
        let mut u = ui(10);
        u.t.pointer(Pointer::Down(Point::new(50, 50)));
        u.t.pointer(Pointer::Move(Point::new(50, 200)));
        let held = over(&u);
        u.t.pointer(Pointer::Up(Point::new(50, 200)));
        assert_eq!(over(&u), held, "nothing moves until time passes");
        u.t.relayout(&screen(), &THEME);
        u.t.advance(&THEME, 16);
        let first = over(&u);
        assert!(
            first != 0 && first.abs() < held.abs(),
            "one frame later it is on its way, not snapped: {held} to {first}"
        );
        u.t.relayout(&screen(), &THEME);
        u.t.advance(&THEME, 100);
        assert!(
            over(&u).abs() < first.abs(),
            "still moving back: {first} to {}",
            over(&u)
        );
        let took = settle(&mut u, 16, 3000);
        assert!((50..2000).contains(&took), "a spring, settled in {took} ms");
        assert_eq!((offset(&u), over(&u)), (0, 0));
        assert!(
            !u.t.advance(&THEME, 16),
            "and a tree at rest asks for no more frames"
        );
    }

    #[test]
    fn a_flick_goes_on_after_the_finger_lifts_slows_down_and_stops_inside_the_range() {
        let mut u = ui(10);
        flick(&mut u, -15, 5);
        let lifted = offset(&u);
        assert_eq!(lifted, 75);
        let mut last = lifted;
        let mut steps = Vec::new();
        for _ in 0..300 {
            u.t.relayout(&screen(), &THEME);
            if !u.t.advance(&THEME, 16) {
                break;
            }
            steps.push(offset(&u) - last);
            last = offset(&u);
        }
        assert!(
            last > lifted + 100,
            "it went on by itself: {lifted} to {last}"
        );
        assert!(last <= u.t.scroll_max(u.sc).unwrap() && over(&u) == 0);
        assert!(steps.iter().all(|s| *s >= 0), "never backwards");
        assert!(
            steps.last().unwrap() < steps.first().unwrap(),
            "slowing down: {:?}",
            &steps[..4]
        );
    }

    #[test]
    fn a_fast_flick_hits_the_end_stretches_and_settles_exactly_on_it() {
        let mut u = ui(10);
        flick(&mut u, -40, 5);
        let max = u.t.scroll_max(u.sc).unwrap();
        let mut stretched = 0;
        for _ in 0..400 {
            u.t.relayout(&screen(), &THEME);
            if !u.t.advance(&THEME, 16) {
                break;
            }
            stretched = stretched.max(over(&u));
        }
        assert!(stretched > 0, "it stretched past the bottom");
        assert_eq!((offset(&u), over(&u)), (max, 0), "and settled on the end");
    }

    #[test]
    fn a_finger_held_still_before_it_lifts_is_a_stop_and_not_a_fling() {
        let mut u = ui(10);
        let (x, y) = (50, 90);
        u.t.pointer(Pointer::Down(Point::new(x, y)));
        u.t.pointer(Pointer::Move(Point::new(x, y - 60)));
        u.t.advance(&THEME, 16);
        for _ in 0..20 {
            u.t.advance(&THEME, 16); // held still for a third of a second: frames still come
        }
        u.t.pointer(Pointer::Up(Point::new(x, y - 60)));
        let at = offset(&u);
        assert!(!u.t.advance(&THEME, 16), "nothing is moving");
        assert_eq!(offset(&u), at);
    }

    #[test]
    fn a_finger_dragging_keeps_the_frames_coming() {
        let mut u = ui(10);
        u.t.pointer(Pointer::Down(Point::new(50, 90)));
        assert!(
            !u.t.advance(&THEME, 16),
            "a press that has not moved yet is not a scroll"
        );
        u.t.pointer(Pointer::Move(Point::new(50, 40)));
        assert!(u.t.advance(&THEME, 16), "a drag does");
    }

    #[test]
    fn touching_a_flinging_list_stops_it_and_the_lift_does_not_pick_a_row() {
        let mut u = ui(10);
        flick(&mut u, -15, 5);
        for _ in 0..3 {
            u.t.relayout(&screen(), &THEME);
            u.t.advance(&THEME, 16);
        }
        let moving = offset(&u);
        u.t.pointer(Pointer::Down(Point::new(50, 20)));
        let caught = offset(&u);
        assert!(
            !u.t.advance(&THEME, 100) || offset(&u) == caught,
            "it stopped"
        );
        u.t.advance(&THEME, 100);
        assert_eq!(offset(&u), caught);
        assert!(caught >= moving);
        u.t.pointer(Pointer::Up(Point::new(50, 20)));
        assert_eq!(u.t.pop_action(), None, "stopping it is not a tap");
        assert!(u.rows.iter().all(|r| !u.t.is_checked(*r)));
        // The next tap works as usual.
        u.t.pointer(Pointer::Down(Point::new(50, 20)));
        u.t.pointer(Pointer::Up(Point::new(50, 20)));
        assert_eq!(u.t.pop_action().map(|a| a.kind), Some(ActionKind::Selected));
    }

    #[test]
    fn slow_frames_move_in_real_time_not_in_slow_motion() {
        let (mut fast, mut slow) = (ui(10), ui(10));
        flick(&mut fast, -15, 5);
        flick(&mut slow, -15, 5);
        for _ in 0..50 {
            fast.t.relayout(&screen(), &THEME);
            fast.t.advance(&THEME, 20);
        }
        for _ in 0..4 {
            slow.t.relayout(&screen(), &THEME);
            slow.t.advance(&THEME, 250); // a watch drawing four frames a second
        }
        let (a, b) = (offset(&fast), offset(&slow));
        assert!(
            (a - b).abs() <= a.max(b) / 20 + 2,
            "after one second: {a} against {b}"
        );
    }

    #[test]
    fn the_wheel_and_code_take_over_from_a_fling() {
        for how in 0..2 {
            let mut u = ui(10);
            flick(&mut u, -15, 5);
            u.t.relayout(&screen(), &THEME);
            u.t.advance(&THEME, 16);
            if how == 0 {
                u.t.pointer(Pointer::Wheel(Point::new(50, 50), 0));
            } else {
                u.t.scroll_to(u.sc, 10);
            }
            let at = offset(&u);
            u.t.relayout(&screen(), &THEME);
            assert!(!u.t.advance(&THEME, 100), "no fling goes on");
            assert_eq!(offset(&u), at, "case {how}");
        }
    }

    #[test]
    fn content_that_fits_does_not_stretch_and_a_removed_container_does_not_panic() {
        let mut u = ui(1);
        u.t.pointer(Pointer::Down(Point::new(50, 20)));
        u.t.pointer(Pointer::Move(Point::new(50, 220)));
        assert_eq!(over(&u), 0);
        u.t.pointer(Pointer::Up(Point::new(50, 220)));
        let mut u = ui(10);
        flick(&mut u, -15, 5);
        u.t.remove(u.sc);
        assert!(!u.t.advance(&THEME, 16));
        assert_eq!(u.t.scroll_over(u.sc), None);
        assert_eq!(
            ui(10).t.scroll_over(ui(10).rows[0]),
            None,
            "only a scroll container has an overshoot"
        );
    }
}
