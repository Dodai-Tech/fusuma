//! The event pass: pointer input goes to the widget under the pointer (or to the one that holds
//! the press), and an event nobody handles goes up to the parent.

use fusuma_render::Point;

use crate::tree::{Action, ActionKind, Flags, Kind, NodeId, Tree};

/// A pointer event: a finger or a mouse, in tree coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pointer {
    /// The pointer went down.
    Down(Point),
    /// The pointer moved, pressed or not.
    Move(Point),
    /// The pointer went up.
    Up(Point),
    /// The pointer left the tree. For a touch screen send this after every [`Pointer::Up`]: a
    /// finger that is lifted hovers over nothing.
    Leave,
}

#[derive(Clone, Copy)]
enum Phase {
    Down,
    Up { inside: bool },
}

impl<const N: usize> Tree<N> {
    /// The index of the topmost enabled widget at `p` that reacts to the pointer.
    pub(crate) fn hit(&self, p: Point) -> Option<usize> {
        let root = usize::from(self.root?);
        let mut found = None;
        let mut cur = Some(root);
        while let Some(i) = cur {
            let n = &self.nodes[i];
            if matches!(n.kind, Kind::Button { .. }) && !n.disabled_eff && n.rect.contains(p) {
                found = Some(i);
            }
            cur = self.preorder_next(i, root);
        }
        found
    }

    /// Makes `new` the hovered widget, and tells the two that changed.
    pub(crate) fn set_hover(&mut self, new: Option<usize>) {
        let new_id = new.map(|i| self.id_of(i));
        if self.hovered == new_id {
            return;
        }
        if let Some(old) = self.hovered.and_then(|id| self.slot(id)) {
            self.nodes[old].hovered = false;
            self.request(old, Flags::PAINT | Flags::ANIM);
        }
        if let Some(i) = new {
            self.nodes[i].hovered = true;
            self.request(i, Flags::PAINT | Flags::ANIM);
        }
        self.hovered = new_id;
    }

    fn set_active(&mut self, i: usize, active: bool) {
        if self.nodes[i].active != active {
            self.nodes[i].active = active;
            self.request(i, Flags::PAINT | Flags::ANIM);
        }
    }

    /// What a widget does with an event; true when it took it and the event stops here.
    fn on_pointer(&mut self, i: usize, phase: Phase) -> bool {
        match (&self.nodes[i].kind, phase) {
            (Kind::Button { .. }, Phase::Down) => {
                if self.nodes[i].disabled_eff {
                    return false;
                }
                self.set_active(i, true);
                true
            }
            (Kind::Button { .. }, Phase::Up { inside }) => {
                if !self.nodes[i].active {
                    return false;
                }
                self.set_active(i, false);
                if inside && !self.nodes[i].disabled_eff {
                    let source = self.id_of(i);
                    self.push_action(Action {
                        source,
                        kind: ActionKind::Clicked,
                    });
                }
                true
            }
            _ => false,
        }
    }

    /// Hands an event to `target`, then to each parent in turn until one takes it.
    fn dispatch(&mut self, target: usize, phase: Phase) {
        let mut cur = Some(target);
        while let Some(i) = cur {
            if self.on_pointer(i, phase) {
                return;
            }
            cur = self.nodes[i].parent.map(usize::from);
        }
    }

    /// The event pass for one pointer event. Changes only state (hover, pressed, queued actions)
    /// and marks what to redo; the passes after it ([`update`](Self::update),
    /// [`advance`](Self::advance), [`paint`](Self::paint)) do the work.
    pub fn pointer(&mut self, event: Pointer) {
        match event {
            Pointer::Move(p) => {
                self.pointer = Some(p);
                let h = self.hit(p);
                self.set_hover(h);
            }
            Pointer::Down(p) => {
                self.pointer = Some(p);
                let h = self.hit(p);
                self.set_hover(h);
                if let Some(t) = h {
                    self.captured = Some(self.id_of(t));
                    self.dispatch(t, Phase::Down);
                }
            }
            Pointer::Up(p) => {
                self.pointer = Some(p);
                let h = self.hit(p);
                if let Some(held) = self.captured.take().and_then(|id| self.slot(id)) {
                    self.dispatch(
                        held,
                        Phase::Up {
                            inside: h == Some(held),
                        },
                    );
                }
                self.set_hover(h);
            }
            Pointer::Leave => {
                self.pointer = None;
                if let Some(held) = self.captured.take().and_then(|id| self.slot(id)) {
                    // A press that the pointer walks away from is cancelled, not a click.
                    self.dispatch(held, Phase::Up { inside: false });
                }
                self.set_hover(None);
            }
        }
    }

    /// The widget that holds the current press, if any.
    #[must_use]
    pub const fn captured(&self) -> Option<NodeId> {
        self.captured
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ButtonVariant, TextRole, Theme};
    use crate::tree::{ActionKind, Style};
    use fusuma_render::Rect;
    use fusuma_render_vello_cpu::VelloCpu;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(8, 8).unwrap()
    }

    /// A column with a heading and two buttons, laid out in a 200 x 200 area.
    fn app() -> (Tree<8>, NodeId, NodeId, NodeId) {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new().gap(10)).unwrap();
        let title = t.label(Some(root), "Title", TextRole::Title).unwrap();
        let a = t.button(Some(root), "A", ButtonVariant::Primary).unwrap();
        let b = t.button(Some(root), "B", ButtonVariant::Secondary).unwrap();
        t.update(&screen(), &Theme::DARK, Rect::new(0, 0, 200, 200));
        (t, title, a, b)
    }

    fn centre<const N: usize>(t: &Tree<N>, id: NodeId) -> Point {
        let r = t.rect(id).unwrap();
        Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
    }

    #[test]
    fn a_press_and_release_on_a_button_is_a_click() {
        let (mut t, _, a, _) = app();
        let p = centre(&t, a);
        t.pointer(Pointer::Down(p));
        assert!(t.is_active(a) && t.is_hovered(a) && t.captured() == Some(a));
        assert_eq!(t.pop_action(), None, "nothing until the release");
        t.pointer(Pointer::Up(p));
        assert!(!t.is_active(a) && t.captured().is_none());
        let action = t.pop_action().unwrap();
        assert_eq!((action.source, action.kind), (a, ActionKind::Clicked));
        assert_eq!(t.pop_action(), None);
    }

    #[test]
    fn releasing_somewhere_else_is_not_a_click_and_still_lets_go() {
        let (mut t, _, a, b) = app();
        t.pointer(Pointer::Down(centre(&t, a)));
        t.pointer(Pointer::Move(centre(&t, b)));
        assert!(t.is_active(a), "the press stays on the button that got it");
        assert!(t.is_hovered(b) && !t.is_hovered(a));
        t.pointer(Pointer::Up(centre(&t, b)));
        assert!(!t.is_active(a) && !t.is_active(b));
        assert_eq!(
            t.pop_action(),
            None,
            "B did not get the press, A did not get the release"
        );
    }

    #[test]
    fn pressing_on_nothing_clicks_nothing() {
        let (mut t, title, _, _) = app();
        t.pointer(Pointer::Down(centre(&t, title)));
        t.pointer(Pointer::Up(centre(&t, title)));
        t.pointer(Pointer::Down(Point::new(-50, 900)));
        t.pointer(Pointer::Up(Point::new(-50, 900)));
        assert_eq!(t.pop_action(), None);
        assert!(t.captured().is_none());
    }

    #[test]
    fn hover_follows_the_pointer_and_leaving_clears_it() {
        let (mut t, _, a, b) = app();
        t.pointer(Pointer::Move(centre(&t, a)));
        assert!(t.is_hovered(a) && !t.is_hovered(b));
        t.pointer(Pointer::Move(centre(&t, b)));
        assert!(!t.is_hovered(a) && t.is_hovered(b));
        t.pointer(Pointer::Leave);
        assert!(!t.is_hovered(a) && !t.is_hovered(b));
    }

    #[test]
    fn leaving_during_a_press_cancels_it() {
        let (mut t, _, a, _) = app();
        t.pointer(Pointer::Down(centre(&t, a)));
        t.pointer(Pointer::Leave);
        assert!(!t.is_active(a) && t.captured().is_none());
        t.pointer(Pointer::Up(centre(&t, a)));
        assert_eq!(t.pop_action(), None);
    }

    #[test]
    fn a_touch_sequence_taps_a_button_and_leaves_nothing_hovered() {
        let (mut t, _, a, _) = app();
        let p = centre(&t, a);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            t.pointer(e);
        }
        assert!(t.pop_action().is_some());
        assert!(!t.is_hovered(a) && !t.is_active(a));
    }

    #[test]
    fn disabled_buttons_ignore_the_pointer() {
        let (mut t, _, a, b) = app();
        t.set_enabled(a, false);
        t.update(&screen(), &Theme::DARK, Rect::new(0, 0, 200, 200));
        t.pointer(Pointer::Move(centre(&t, a)));
        assert!(!t.is_hovered(a));
        t.pointer(Pointer::Down(centre(&t, a)));
        t.pointer(Pointer::Up(centre(&t, a)));
        assert_eq!(t.pop_action(), None);
        t.pointer(Pointer::Down(centre(&t, b)));
        t.pointer(Pointer::Up(centre(&t, b)));
        assert_eq!(t.pop_action().unwrap().source, b);
    }

    #[test]
    fn disabling_a_button_mid_press_cancels_the_press() {
        let (mut t, _, a, _) = app();
        let p = centre(&t, a);
        t.pointer(Pointer::Down(p));
        t.set_enabled(a, false);
        t.update(&screen(), &Theme::DARK, Rect::new(0, 0, 200, 200));
        assert!(!t.is_active(a) && t.captured().is_none());
        t.pointer(Pointer::Up(p));
        assert_eq!(t.pop_action(), None);
    }

    #[test]
    fn removing_the_pressed_button_is_harmless() {
        let (mut t, _, a, _) = app();
        let p = centre(&t, a);
        t.pointer(Pointer::Down(p));
        t.remove(a);
        assert!(t.captured().is_none());
        t.pointer(Pointer::Up(p));
        assert_eq!(t.pop_action(), None);
    }

    #[test]
    fn a_layout_change_under_a_still_pointer_moves_the_hover() {
        let (mut t, _, a, b) = app();
        let over_b = centre(&t, b);
        t.pointer(Pointer::Move(over_b));
        assert!(t.is_hovered(b));
        // Remove A: B moves up, so the pointer is now over empty space below it.
        t.remove(a);
        t.update(&screen(), &Theme::DARK, Rect::new(0, 0, 200, 200));
        assert!(!t.is_hovered(b), "B moved out from under the pointer");
        // Put a button under the pointer again.
        let root = t.root().unwrap();
        let c = t.button(Some(root), "C", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &Theme::DARK, Rect::new(0, 0, 200, 200));
        t.pointer(Pointer::Move(centre(&t, c)));
        assert!(t.is_hovered(c));
    }

    #[test]
    fn events_on_an_empty_tree_do_nothing() {
        let mut t = Tree::<2>::new();
        for e in [
            Pointer::Down(Point::new(1, 1)),
            Pointer::Move(Point::new(1, 1)),
            Pointer::Up(Point::new(1, 1)),
            Pointer::Leave,
        ] {
            t.pointer(e);
        }
        assert_eq!(t.pop_action(), None);
    }

    #[test]
    fn an_unhandled_event_bubbles_to_the_parent_chain() {
        // Only buttons take pointer events today; this checks that the walk up ends at the root
        // without taking anything and without panicking, from a deep node.
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let row = t.row(Some(root), Style::new()).unwrap();
        let label = t.label(Some(row), "x", TextRole::Body).unwrap();
        let i = t.slot(label).unwrap();
        t.dispatch(i, Phase::Down);
        t.dispatch(i, Phase::Up { inside: true });
        assert_eq!(t.pop_action(), None);
    }
}
