//! The event pass: pointer input goes to the widget under the pointer (or to the one that holds
//! the press), and an event nobody handles goes up to the parent.

use fusuma_render::Point;

use crate::tree::{Drag, Flags, Kind, NodeId, Tree};

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
    /// The mouse wheel (or a trackpad) turned over `Point`: `dy` pixels, positive to scroll the content
    /// up (see later content). A platform converts lines to pixels.
    Wheel(Point, i32),
}

/// How far a press must travel before it is a scroll and not a click, in pixels.
pub(crate) const SCROLL_SLOP: i32 = 8;

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
            if n.kind.pressable()
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

    /// Whether `p` is inside the viewport of every scroll container above node `i`: a widget scrolled out of
    /// sight cannot be hit.
    pub(crate) fn in_viewports(&self, i: usize, p: Point) -> bool {
        let mut cur = self.nodes[i].parent.map(usize::from);
        while let Some(s) = cur {
            if matches!(self.nodes[s].kind, Kind::Scroll { .. }) && !self.nodes[s].rect.contains(p)
            {
                return false;
            }
            cur = self.nodes[s].parent.map(usize::from);
        }
        true
    }

    /// The innermost scroll container under `p`, if any.
    pub(crate) fn scroll_at(&self, p: Point) -> Option<usize> {
        let root = usize::from(self.root?);
        let mut found = None;
        let mut cur = Some(root);
        while let Some(i) = cur {
            if matches!(self.nodes[i].kind, Kind::Scroll { .. })
                && self.nodes[i].rect.contains(p)
                && self.in_viewports(i, p)
                && !self.blocked(i)
            {
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

    pub(crate) fn set_active(&mut self, i: usize, active: bool) {
        if self.nodes[i].active != active {
            self.nodes[i].active = active;
            self.request(i, Flags::PAINT | Flags::ANIM);
        }
    }

    /// What a widget does with an event; true when it took it and the event stops here.
    fn on_pointer(&mut self, i: usize, phase: Phase) -> bool {
        if !self.nodes[i].kind.pressable() {
            return false;
        }
        match phase {
            Phase::Down => {
                if self.nodes[i].disabled_eff {
                    return false;
                }
                self.set_active(i, true);
                if let (Some(p), true) = (self.pointer, self.is_slider(i)) {
                    self.slider_to(i, p);
                }
                if let Some(p) = self.pointer {
                    self.input_press(i, p.x);
                }
                true
            }
            Phase::Up { inside } => {
                if !self.nodes[i].active {
                    return false;
                }
                self.set_active(i, false);
                if inside
                    && !self.nodes[i].disabled_eff
                    && !self.is_slider(i)
                    && !matches!(self.nodes[i].kind, Kind::TextInput { .. })
                {
                    self.activate(i);
                }
                true
            }
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
        // A press outside an open dialog is a request to close it, and reaches nothing behind it.
        if let Pointer::Down(p) = event
            && let Some(m) = self.modal()
            && !self.nodes[m].rect.contains(p)
        {
            self.pointer = Some(p);
            self.dismiss_modal();
            return;
        }
        match event {
            Pointer::Move(p) => {
                self.pointer = Some(p);
                self.gesture_move(p);
                self.drag_scroll(p);
                // A slider that holds the press follows the pointer, even outside its bounds.
                if let Some(held) = self.captured.and_then(|id| self.slot(id))
                    && self.nodes[held].active
                    && self.is_slider(held)
                {
                    self.slider_to(held, p);
                }
                // A text field that holds the press extends its selection to the pointer.
                if let Some(held) = self.captured.and_then(|id| self.slot(id))
                    && self.nodes[held].active
                {
                    self.input_drag(held, p.x);
                }
                // While a scroll waits to be laid out the rows are not where they will be: the hover is worked out
                // after the layout, in `update`.
                if self.shift.is_none() {
                    let h = self.hit(p);
                    self.set_hover(h);
                }
            }
            Pointer::Down(p) => {
                self.pointer = Some(p);
                self.gesture_down(p);
                // A press on a container that is flinging or bouncing stops it, and the lift is not a click.
                self.caught = self.catch_motion();
                self.drag = self.scroll_at(p).map(|s| {
                    let at = self.scroll_offset_at(s);
                    Drag {
                        scroll: self.id_of(s),
                        start_y: p.y,
                        offset0: at,
                        moving: false,
                        raw: at,
                        last_raw: at,
                        vel: 0,
                    }
                });
                let h = self.hit(p);
                self.set_hover(h);
                if let Some(t) = h {
                    // A touch or a click focuses without showing the ring; a key shows it.
                    self.set_focus(Some(t), false);
                    self.captured = Some(self.id_of(t));
                    self.dispatch(t, Phase::Down);
                }
            }
            Pointer::Up(p) => {
                self.pointer = Some(p);
                self.gesture_end();
                let released = self.drag.take();
                let h = self.hit(p);
                let caught = core::mem::take(&mut self.caught);
                if let Some(held) = self.captured.take().and_then(|id| self.slot(id)) {
                    self.dispatch(
                        held,
                        Phase::Up {
                            inside: h == Some(held) && !caught,
                        },
                    );
                }
                if self.shift.is_none() {
                    self.set_hover(h);
                }
                // A scroll that was dragged goes on by itself, or bounces back from an end.
                if let Some(d) = released.filter(|d| d.moving) {
                    self.release_scroll(d);
                }
            }
            Pointer::Wheel(p, dy) => {
                if let Some(s) = self.scroll_at(p) {
                    self.cancel_motion(s);
                    let to = self.scroll_offset_at(s).saturating_add(dy);
                    self.set_scroll(s, to);
                    if self.shift.is_none() {
                        let h = self.hit(p);
                        self.set_hover(h);
                    }
                }
            }
            Pointer::Leave => {
                self.pointer = None;
                self.gesture_end();
                self.drag = None;
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
