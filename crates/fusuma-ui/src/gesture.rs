//! Gestures a widget can ask for: a long press and a swipe. A press that does something else (a button's click, a list
//! scrolling) keeps doing it unless the widget asked for the gesture; when the gesture happens the press is over, so
//! its release is not a click and a scroll container under it lets go of the finger.

use fusuma_render::Point;

use crate::tree::{Action, ActionKind, Flags, NodeId, Tree};

/// How long a press must stay, in milliseconds, to be a long press.
pub const LONG_PRESS_MS: u32 = 500;
/// How far a press may wander and still be a long press, in pixels.
const LONG_SLOP: i64 = 8;
/// How far a press must travel along one axis to be a swipe, in pixels, and how much more along it than across.
pub const SWIPE_PX: i32 = 40;

const LONG: u8 = 1;
const SWIPE_H: u8 = 2;
const SWIPE_V: u8 = 4;

/// The way a swipe went.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwipeDir {
    /// Towards the left edge.
    Left,
    /// Towards the right edge.
    Right,
    /// Towards the top.
    Up,
    /// Towards the bottom.
    Down,
}

/// Which swipes a widget listens for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Swipe {
    /// None.
    Off,
    /// Left and right.
    Horizontal,
    /// Up and down. A scroll container under the press has already moved by the time it is seen: use it on what does
    /// not scroll.
    Vertical,
    /// All four.
    Both,
}

/// A press being watched.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Track {
    node: NodeId,
    start: Point,
    /// How long it has been held, in milliseconds.
    held: u32,
    /// Whether it wandered too far to be a long press.
    wandered: bool,
    /// Whether a gesture already happened.
    done: bool,
}

impl<const N: usize> Tree<N> {
    /// Makes node `id` send [`ActionKind::LongPressed`] when it is pressed and held (see [`LONG_PRESS_MS`]).
    pub fn set_long_press(&mut self, id: NodeId, on: bool) {
        self.set_gesture(id, LONG, on);
    }

    /// Makes node `id` send [`ActionKind::Swiped`] when a press travels far enough along an axis (see [`SWIPE_PX`]).
    pub fn set_swipe(&mut self, id: NodeId, swipe: Swipe) {
        self.set_gesture(
            id,
            SWIPE_H,
            matches!(swipe, Swipe::Horizontal | Swipe::Both),
        );
        self.set_gesture(id, SWIPE_V, matches!(swipe, Swipe::Vertical | Swipe::Both));
    }

    fn set_gesture(&mut self, id: NodeId, bit: u8, on: bool) {
        if let Some(i) = self.slot(id) {
            if on {
                self.nodes[i].gestures |= bit;
            } else {
                self.nodes[i].gestures &= !bit;
            }
        }
    }

    /// The innermost node under `p` that listens for a gesture.
    fn gesture_at(&self, p: Point) -> Option<usize> {
        let root = usize::from(self.root?);
        let mut found = None;
        let mut cur = Some(root);
        while let Some(i) = cur {
            let n = &self.nodes[i];
            if n.gestures != 0
                && !n.disabled_eff
                && n.rect.contains(p)
                && self.in_viewports(i, p)
                && !self.blocked(i)
            {
                found = Some(i);
            }
            cur = self.preorder_next(i, root);
        }
        found
    }

    /// A press began at `p`.
    pub(crate) fn gesture_down(&mut self, p: Point) {
        self.track = self.gesture_at(p).map(|i| Track {
            node: self.id_of(i),
            start: p,
            held: 0,
            wandered: false,
            done: false,
        });
    }

    /// The press ended or the pointer left.
    pub(crate) fn gesture_end(&mut self) {
        self.track = None;
    }

    /// The press moved to `p`.
    pub(crate) fn gesture_move(&mut self, p: Point) {
        let Some(mut t) = self.track else { return };
        let Some(i) = self.slot(t.node).filter(|_| !t.done) else {
            return;
        };
        let (dx, dy) = (
            i64::from(p.x) - i64::from(t.start.x),
            i64::from(p.y) - i64::from(t.start.y),
        );
        if dx.abs() > LONG_SLOP || dy.abs() > LONG_SLOP {
            t.wandered = true;
        }
        let g = self.nodes[i].gestures;
        let reach = i64::from(SWIPE_PX);
        let dir = if g & SWIPE_H != 0 && dx.abs() >= reach && dx.abs() >= 2 * dy.abs() {
            Some(if dx < 0 {
                SwipeDir::Left
            } else {
                SwipeDir::Right
            })
        } else if g & SWIPE_V != 0 && dy.abs() >= reach && dy.abs() >= 2 * dx.abs() {
            Some(if dy < 0 { SwipeDir::Up } else { SwipeDir::Down })
        } else {
            None
        };
        self.track = Some(t);
        if let Some(dir) = dir {
            self.gesture_fire(i, ActionKind::Swiped(dir));
        }
    }

    /// Time passes by `dt_ms`: a press that stays long enough is a long press. True while one is being waited for.
    pub(crate) fn advance_gesture(&mut self, dt_ms: u32) -> bool {
        let Some(mut t) = self.track else {
            return false;
        };
        let Some(i) = self.slot(t.node).filter(|_| !t.done) else {
            return false;
        };
        if self.nodes[i].gestures & LONG == 0 || t.wandered {
            return false;
        }
        t.held = t.held.saturating_add(dt_ms);
        self.track = Some(t);
        if t.held >= LONG_PRESS_MS {
            self.gesture_fire(i, ActionKind::LongPressed);
            return false;
        }
        true
    }

    /// Whether a press is being timed for a long press.
    pub(crate) fn gesture_waiting(&self) -> bool {
        self.track.is_some_and(|t| {
            !t.done
                && !t.wandered
                && self
                    .slot(t.node)
                    .is_some_and(|i| self.nodes[i].gestures & LONG != 0)
        })
    }

    /// The gesture happened: tell the app and end the press.
    fn gesture_fire(&mut self, i: usize, kind: ActionKind) {
        if let Some(t) = &mut self.track {
            t.done = true;
        }
        let source = self.id_of(i);
        self.push_action(Action { source, kind });
        // The widget under the finger lets go without a click, and a scroll container stops following it.
        if let Some(held) = self.captured.take().and_then(|id| self.slot(id)) {
            self.set_active(held, false);
        }
        self.drag = None;
        self.request(i, Flags::PAINT);
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Point, Rect};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{CrossAlign, Length, Style};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(8, 8).unwrap()
    }

    struct Ui {
        t: Tree<16>,
        button: NodeId,
        card: NodeId,
        inner: NodeId,
        list: NodeId,
        row: NodeId,
    }

    /// A button, a card (a row with a button in it) and a scrolling list with a row.
    fn ui() -> Ui {
        let mut t = Tree::<16>::new();
        let root = t
            .column(
                None,
                Style::new().gap(8).padding(8).align(CrossAlign::Stretch),
            )
            .unwrap();
        let button = t
            .button(Some(root), "Hold me", ButtonVariant::Primary)
            .unwrap();
        let card = t
            .row(
                Some(root),
                Style::new().padding(10).height(Length::Fixed(60)),
            )
            .unwrap();
        let inner = t
            .button(Some(card), "In card", ButtonVariant::Secondary)
            .unwrap();
        let list = t
            .scroll(
                Some(root),
                Style::new()
                    .height(Length::Fixed(80))
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        for _ in 0..6 {
            t.list_item(Some(list), "row", false).unwrap();
        }
        let row = t.children(list).next().unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 300));
        Ui {
            t,
            button,
            card,
            inner,
            list,
            row,
        }
    }

    fn centre(t: &Tree<16>, id: NodeId) -> Point {
        let r = t.rect(id).unwrap();
        Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
    }

    fn kinds(t: &mut Tree<16>) -> Vec<ActionKind> {
        std::iter::from_fn(|| t.pop_action())
            .map(|a| a.kind)
            .collect()
    }

    #[test]
    fn a_held_press_is_a_long_press_and_its_release_is_not_a_click() {
        let mut u = ui();
        u.t.set_long_press(u.button, true);
        let p = centre(&u.t, u.button);
        u.t.pointer(Pointer::Down(p));
        assert!(
            u.t.advance(&THEME, LONG_PRESS_MS - 1),
            "still waiting: frames are wanted"
        );
        assert!(u.t.needs_frame());
        assert_eq!(kinds(&mut u.t), []);
        u.t.advance(&THEME, 2);
        let a = u.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (u.button, ActionKind::LongPressed));
        assert!(!u.t.is_active(u.button), "the press is over");
        u.t.advance(&THEME, 5000);
        u.t.pointer(Pointer::Up(p));
        assert_eq!(kinds(&mut u.t), [], "no click, and no second long press");
    }

    #[test]
    fn without_the_flag_a_long_press_is_still_a_click_as_before() {
        let mut u = ui();
        let p = centre(&u.t, u.button);
        u.t.pointer(Pointer::Down(p));
        u.t.advance(&THEME, 3000);
        u.t.pointer(Pointer::Up(p));
        assert_eq!(kinds(&mut u.t), [ActionKind::Clicked]);
        // Turning it off again.
        u.t.set_long_press(u.button, true);
        u.t.set_long_press(u.button, false);
        u.t.pointer(Pointer::Down(p));
        u.t.advance(&THEME, 3000);
        u.t.pointer(Pointer::Up(p));
        assert_eq!(kinds(&mut u.t), [ActionKind::Clicked]);
    }

    #[test]
    fn a_press_that_wanders_or_ends_early_is_no_long_press() {
        let mut u = ui();
        u.t.set_long_press(u.button, true);
        let p = centre(&u.t, u.button);
        u.t.pointer(Pointer::Down(p));
        u.t.advance(&THEME, 300);
        u.t.pointer(Pointer::Move(Point::new(p.x + 12, p.y)));
        u.t.pointer(Pointer::Move(p));
        u.t.advance(&THEME, 600);
        assert_eq!(kinds(&mut u.t), [], "it wandered once: not a long press");
        u.t.pointer(Pointer::Up(p));
        assert_eq!(
            kinds(&mut u.t),
            [ActionKind::Clicked],
            "the release over the button is a click"
        );
        // A short press.
        u.t.pointer(Pointer::Down(p));
        u.t.advance(&THEME, 200);
        u.t.pointer(Pointer::Up(p));
        u.t.advance(&THEME, 2000);
        assert_eq!(kinds(&mut u.t), [ActionKind::Clicked]);
        assert!(!u.t.advance(&THEME, 100), "nothing is being timed");
    }

    #[test]
    fn a_swipe_fires_once_at_the_distance_and_the_release_is_not_a_click() {
        let mut u = ui();
        u.t.set_swipe(u.card, Swipe::Horizontal);
        let p = centre(&u.t, u.inner);
        u.t.pointer(Pointer::Down(p));
        assert!(
            u.t.is_active(u.inner),
            "the button under it is pressed meanwhile"
        );
        u.t.pointer(Pointer::Move(Point::new(p.x + SWIPE_PX - 1, p.y)));
        assert_eq!(kinds(&mut u.t), [], "not far enough");
        u.t.pointer(Pointer::Move(Point::new(p.x + SWIPE_PX + 1, p.y + 3)));
        let a = u.t.pop_action().unwrap();
        assert_eq!(
            (a.source, a.kind),
            (u.card, ActionKind::Swiped(SwipeDir::Right))
        );
        assert!(!u.t.is_active(u.inner), "the button let go");
        u.t.pointer(Pointer::Move(Point::new(p.x + 100, p.y)));
        u.t.pointer(Pointer::Up(Point::new(p.x + 100, p.y)));
        assert_eq!(kinds(&mut u.t), [], "once, and no click");
        // The other way.
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x - SWIPE_PX, p.y)));
        assert_eq!(kinds(&mut u.t), [ActionKind::Swiped(SwipeDir::Left)]);
        u.t.pointer(Pointer::Up(p));
    }

    #[test]
    fn a_swipe_needs_to_go_mostly_along_one_axis_the_axis_asked_for() {
        let mut u = ui();
        u.t.set_swipe(u.card, Swipe::Horizontal);
        let p = centre(&u.t, u.card);
        // Diagonal: more across than half of the way along.
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x + 50, p.y + 30)));
        assert_eq!(kinds(&mut u.t), []);
        u.t.pointer(Pointer::Up(p));
        // A vertical swipe on a horizontal-only widget.
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x, p.y - 60)));
        assert_eq!(kinds(&mut u.t), []);
        u.t.pointer(Pointer::Up(p));
        u.t.set_swipe(u.card, Swipe::Vertical);
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x, p.y - 60)));
        assert_eq!(kinds(&mut u.t), [ActionKind::Swiped(SwipeDir::Up)]);
        u.t.pointer(Pointer::Up(p));
        u.t.set_swipe(u.card, Swipe::Both);
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x + 1, p.y + 60)));
        assert_eq!(kinds(&mut u.t), [ActionKind::Swiped(SwipeDir::Down)]);
        u.t.pointer(Pointer::Up(p));
        u.t.set_swipe(u.card, Swipe::Off);
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x + 90, p.y)));
        assert_eq!(kinds(&mut u.t), []);
        u.t.pointer(Pointer::Up(p));
    }

    #[test]
    fn a_row_in_a_scrolling_list_swipes_sideways_and_the_list_still_scrolls_up_and_down() {
        let mut u = ui();
        u.t.set_swipe(u.row, Swipe::Horizontal);
        let p = centre(&u.t, u.row);
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x - 60, p.y + 2)));
        assert_eq!(kinds(&mut u.t), [ActionKind::Swiped(SwipeDir::Left)]);
        u.t.pointer(Pointer::Up(Point::new(p.x - 60, p.y)));
        assert_eq!(
            u.t.scroll_offset(u.list),
            Some(0),
            "the list did not scroll"
        );
        // A vertical drag scrolls and swipes nothing.
        u.t.pointer(Pointer::Down(p));
        for k in 1..=8 {
            u.t.pointer(Pointer::Move(Point::new(p.x, p.y - k * 6)));
        }
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 300));
        assert!(u.t.scroll_offset(u.list).unwrap() > 20);
        assert_eq!(kinds(&mut u.t), []);
        u.t.pointer(Pointer::Up(Point::new(p.x, p.y - 48)));
    }

    #[test]
    fn disabled_nodes_stale_handles_and_a_press_elsewhere_are_ignored() {
        let mut u = ui();
        u.t.set_swipe(u.card, Swipe::Horizontal);
        u.t.set_long_press(u.card, true);
        u.t.set_enabled(u.card, false);
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 300));
        let p = centre(&u.t, u.card);
        u.t.pointer(Pointer::Down(p));
        u.t.pointer(Pointer::Move(Point::new(p.x + 80, p.y)));
        u.t.advance(&THEME, 2000);
        assert_eq!(kinds(&mut u.t), []);
        u.t.pointer(Pointer::Up(p));
        // A press elsewhere does not belong to the card.
        u.t.set_enabled(u.card, true);
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 300));
        let b = centre(&u.t, u.button);
        u.t.pointer(Pointer::Down(b));
        u.t.pointer(Pointer::Move(Point::new(b.x + 80, b.y)));
        u.t.advance(&THEME, 2000);
        assert_eq!(kinds(&mut u.t), []);
        u.t.pointer(Pointer::Up(b));
        let gone = u.card;
        u.t.remove(gone);
        u.t.set_swipe(gone, Swipe::Both);
        u.t.set_long_press(gone, true);
        assert!(!u.t.advance(&THEME, 100));
    }
}
