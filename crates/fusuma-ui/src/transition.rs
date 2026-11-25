//! Page transitions: a node slides in from an edge, or out towards one. The slide moves the node and everything
//! under it from where its parent puts it (`shift`), a little each frame, and only the parent's children are placed
//! again, so a transition costs no measuring. The widgets stay real while they move: they can be pressed where they
//! are drawn.

use fusuma_anim::{Easing, Fx};

use crate::theme::Theme;
use crate::tree::{Action, ActionKind, Flags, NodeId, Tree};

/// Most slides at once (a page going out and one coming in, and a spare pair).
pub const MAX_SLIDES: usize = 4;

/// An edge of the screen a node comes from or goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// The left edge.
    Left,
    /// The right edge.
    Right,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
}

impl Edge {
    /// `distance` pixels towards this edge, as a move.
    fn towards(self, distance: u32) -> (i32, i32) {
        let d = i32::try_from(distance).unwrap_or(i32::MAX);
        match self {
            Self::Left => (-d, 0),
            Self::Right => (d, 0),
            Self::Top => (0, -d),
            Self::Bottom => (0, d),
        }
    }
}

/// A slide in progress.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Slide {
    node: NodeId,
    from: (i32, i32),
    to: (i32, i32),
    /// Milliseconds so far.
    t: u32,
    /// Whether to tell the app when it ends ([`ActionKind::Exited`]).
    exit: bool,
}

impl<const N: usize> Tree<N> {
    /// Slides node `id` in from `edge`: it starts `distance` pixels off its place towards that edge and comes to it
    /// over the theme's normal motion time. Use it on a page just added. The root cannot slide.
    pub fn slide_in(&mut self, id: NodeId, edge: Edge, distance: u32) {
        self.start_slide(id, edge.towards(distance), (0, 0), false);
    }

    /// Slides node `id` out towards `edge`, `distance` pixels, then sends [`ActionKind::Exited`] from it; the app
    /// removes it then.
    pub fn slide_out(&mut self, id: NodeId, edge: Edge, distance: u32) {
        self.start_slide(id, (0, 0), edge.towards(distance), true);
    }

    fn start_slide(&mut self, id: NodeId, from: (i32, i32), to: (i32, i32), exit: bool) {
        let Some(i) = self.slot(id) else { return };
        let Some(parent) = self.nodes[i].parent else {
            return;
        };
        // From where it is now, if it is already moving: a turn-round does not jump.
        let from = if self.moves.iter().flatten().any(|m| m.node == id) {
            {
                let (x, y) = self.nodes[i].shift;
                (i32::from(x), i32::from(y))
            }
        } else {
            from
        };
        self.moves.iter_mut().for_each(|m| {
            if m.is_some_and(|m| m.node == id) {
                *m = None;
            }
        });
        let slide = Slide {
            node: id,
            from,
            to,
            t: 0,
            exit,
        };
        // A free slot, else the oldest is finished at once.
        let at = self
            .moves
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                if let Some(old) = self.moves[0].take() {
                    self.finish_slide(old);
                }
                0
            });
        self.moves[at] = Some(slide);
        self.set_shift(i, from);
        self.request_quiet(usize::from(parent), Flags::PLACE);
    }

    /// Moves node `i` to `to` from where its parent puts it, and places it again.
    pub(crate) fn set_shift(&mut self, i: usize, to: (i32, i32)) {
        // A slide is a few hundred pixels at most: two bytes an axis are plenty, and a longer one is cut.
        let to16 = |v: i32| i16::try_from(v).unwrap_or(if v < 0 { i16::MIN } else { i16::MAX });
        let to = (to16(to.0), to16(to.1));
        if self.nodes[i].shift != to {
            self.nodes[i].shift = to;
            if let Some(p) = self.nodes[i].parent {
                self.request_quiet(usize::from(p), Flags::PLACE);
            }
        }
    }

    /// Puts a slide at its end.
    fn finish_slide(&mut self, s: Slide) {
        let Some(i) = self.slot(s.node) else { return };
        self.set_shift(i, s.to);
        if s.exit {
            self.push_action(Action {
                source: s.node,
                kind: ActionKind::Exited,
            });
        }
    }

    /// Moves the slides on by `dt_ms`. True while one is going.
    pub(crate) fn advance_slides(&mut self, theme: &Theme, dt_ms: u32) -> bool {
        let ms = theme.motion.normal_ms;
        for k in 0..MAX_SLIDES {
            let Some(mut s) = self.moves[k] else { continue };
            let Some(i) = self.slot(s.node) else {
                self.moves[k] = None;
                continue;
            };
            s.t = s.t.saturating_add(dt_ms);
            if s.t >= ms {
                self.moves[k] = None;
                self.finish_slide(s);
                continue;
            }
            let p = Easing::CubicOut.apply(Fx::from_ratio(s.t as i32, ms as i32));
            let at = |a: i32, b: i32| {
                let d = i64::from(b) - i64::from(a);
                let moved = d * i64::from(p.raw()) / i64::from(Fx::ONE.raw());
                (i64::from(a) + moved).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
            };
            self.set_shift(i, (at(s.from.0, s.to.0), at(s.from.1, s.to.1)));
            self.moves[k] = Some(s);
        }
        self.moves.iter().any(Option::is_some)
    }

    /// How far node `id` is moved from its place by a slide, in pixels.
    #[must_use]
    pub fn slide_offset(&self, id: NodeId) -> Option<(i32, i32)> {
        let (x, y) = self.nodes[self.slot(id)?].shift;
        Some((i32::from(x), i32::from(y)))
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Color, Point, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{CrossAlign, Style};

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 200;
    const H: u32 = 160;
    const BOUNDS: Rect = Rect::new(0, 0, W, H);

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    struct Ui {
        t: Tree<16>,
        root: NodeId,
        old: NodeId,
        old_button: NodeId,
    }

    /// A column with one page: a column holding a heading and a button.
    fn ui() -> Ui {
        let mut t = Tree::<16>::new();
        let root = t
            .column(None, Style::new().padding(6).align(CrossAlign::Start))
            .unwrap();
        let old = page(&mut t, root, "Old page");
        let old_button = t.children(old).nth(1).unwrap();
        t.update(&screen(), &THEME, BOUNDS);
        Ui {
            t,
            root,
            old,
            old_button,
        }
    }

    fn page(t: &mut Tree<16>, parent: NodeId, title: &str) -> NodeId {
        let p = t
            .column(
                Some(parent),
                Style::new()
                    .gap(4)
                    .background(crate::theme::Surface::Surface),
            )
            .unwrap();
        t.label(Some(p), title, crate::theme::TextRole::Title)
            .unwrap();
        t.button(Some(p), "Go", ButtonVariant::Primary).unwrap();
        p
    }

    fn pixels(s: &VelloCpu) -> Vec<Color> {
        (0..H as i32)
            .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
            .map(|(x, y)| s.pixel(x, y).unwrap())
            .collect()
    }

    fn full(t: &mut Tree<16>) -> Vec<Color> {
        let mut s = screen();
        t.update(&s, &THEME, BOUNDS);
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        pixels(&s)
    }

    fn frame(t: &mut Tree<16>, s: &mut VelloCpu) {
        t.update(s, &THEME, BOUNDS);
        if let Some(d) = t.take_damage_in(BOUNDS) {
            s.save().unwrap();
            s.clip(d).unwrap();
            s.clear(THEME.colors.background).unwrap();
            t.paint_region(s, &THEME, d).unwrap();
            s.restore().unwrap();
        }
    }

    #[test]
    fn a_page_slides_in_from_the_edge_to_its_place_and_every_frame_is_a_whole_picture() {
        let mut u = ui();
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        u.t.remove(u.old);
        let new = page(&mut u.t, u.root, "New page");
        frame(&mut u.t, &mut s);
        let home = u.t.rect(new).unwrap();
        u.t.slide_in(new, Edge::Right, 120);
        frame(&mut u.t, &mut s);
        assert_eq!(
            u.t.rect(new).unwrap().x,
            home.x + 120,
            "it starts off its place"
        );
        assert_eq!(u.t.slide_offset(new), Some((120, 0)));
        let mut xs = vec![u.t.rect(new).unwrap().x];
        let mut guard = 0;
        while u.t.advance(&THEME, 16) {
            frame(&mut u.t, &mut s);
            assert_eq!(pixels(&s), full(&mut u.t), "after {} frames", xs.len());
            xs.push(u.t.rect(new).unwrap().x);
            guard += 1;
            assert!(guard < 100);
        }
        frame(&mut u.t, &mut s);
        assert!(
            xs.windows(2).all(|w| w[1] <= w[0]),
            "it only comes closer: {xs:?}"
        );
        assert!(xs.len() > 4, "over several frames: {xs:?}");
        assert_eq!(
            u.t.rect(new).unwrap(),
            home,
            "and ends exactly in its place"
        );
        assert_eq!(u.t.slide_offset(new), Some((0, 0)));
        assert!(!u.t.needs_frame());
    }

    #[test]
    fn what_slides_can_be_pressed_where_it_is_drawn() {
        let mut u = ui();
        u.t.slide_in(u.old, Edge::Left, 80);
        u.t.update(&screen(), &THEME, BOUNDS);
        u.t.advance(&THEME, 40);
        u.t.update(&screen(), &THEME, BOUNDS);
        let r = u.t.rect(u.old_button).unwrap();
        let p = Point::new(r.x + 3, r.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            u.t.pointer(e);
        }
        assert_eq!(u.t.pop_action().map(|a| a.source), Some(u.old_button));
    }

    #[test]
    fn sliding_out_ends_with_exited_once_and_the_page_can_then_be_removed() {
        let mut u = ui();
        u.t.slide_out(u.old, Edge::Top, 90);
        let mut frames = 0;
        while u.t.advance(&THEME, 16) {
            u.t.update(&screen(), &THEME, BOUNDS);
            assert_eq!(u.t.pop_action(), None, "not before the end");
            frames += 1;
            assert!(frames < 100);
        }
        let a = u.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (u.old, ActionKind::Exited));
        assert_eq!(u.t.pop_action(), None, "once");
        u.t.update(&screen(), &THEME, BOUNDS);
        assert_eq!(u.t.slide_offset(u.old), Some((0, -90)));
        u.t.remove(u.old);
        u.t.update(&screen(), &THEME, BOUNDS);
        assert!(!u.t.advance(&THEME, 100));
    }

    #[test]
    fn one_page_out_and_one_in_together_and_a_turn_round_does_not_jump() {
        let mut u = ui();
        let new = page(&mut u.t, u.root, "Next");
        u.t.update(&screen(), &THEME, BOUNDS);
        u.t.slide_out(u.old, Edge::Left, 100);
        u.t.slide_in(new, Edge::Right, 100);
        u.t.advance(&THEME, 100);
        u.t.update(&screen(), &THEME, BOUNDS);
        let (a, b) = (
            u.t.slide_offset(u.old).unwrap(),
            u.t.slide_offset(new).unwrap(),
        );
        assert!(a.0 < 0 && a.0 > -100 && b.0 > 0 && b.0 < 100, "{a:?} {b:?}");
        // Turn the incoming one round half way: it goes on from where it is.
        let before = u.t.slide_offset(new).unwrap();
        u.t.slide_out(new, Edge::Right, 100);
        assert_eq!(u.t.slide_offset(new), Some(before), "no jump");
        while u.t.advance(&THEME, 16) {}
        assert_eq!(u.t.slide_offset(new), Some((100, 0)));
        assert_eq!(u.t.slide_offset(u.old), Some((-100, 0)));
    }

    #[test]
    fn a_motion_budget_of_none_is_at_once_and_odd_calls_are_harmless() {
        let mut u = ui();
        let off = THEME.with_motion(crate::MotionBudget::Off);
        u.t.slide_in(u.old, Edge::Bottom, 50);
        assert!(!u.t.advance(&off, 0), "done at the first step");
        assert_eq!(u.t.slide_offset(u.old), Some((0, 0)));
        // The root cannot slide; a stale handle and more slides than room are fine.
        u.t.slide_in(u.root, Edge::Left, 10);
        assert_eq!(u.t.slide_offset(u.root), Some((0, 0)));
        let gone = u.old_button;
        u.t.remove(gone);
        u.t.slide_in(gone, Edge::Left, 10);
        assert_eq!(u.t.slide_offset(gone), None);
        let mut pages = Vec::new();
        for _ in 0..(MAX_SLIDES + 2) {
            let p = u.t.spacer(Some(u.root), Style::new()).unwrap();
            u.t.slide_out(p, Edge::Left, 30);
            pages.push(p);
        }
        while u.t.advance(&THEME, 50) {}
        let mut exited = 0;
        while let Some(a) = u.t.pop_action() {
            exited += usize::from(a.kind == ActionKind::Exited);
        }
        assert!(exited >= MAX_SLIDES, "{exited}");
    }
}
