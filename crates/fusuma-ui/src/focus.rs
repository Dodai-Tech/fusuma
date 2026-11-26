//! Keyboard focus: which widget the keys act on, and how keys move it.

use crate::tree::{Flags, Kind, NodeId, Tree};

/// How far a slider moves for one arrow key, in thousandths.
pub(crate) const SLIDER_STEP: u16 = 50;

/// A key the tree understands. The application maps its own keys (a keyboard, a button, a rotary
/// encoder) to these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Tab: the next widget that can be focused, wrapping round at the end.
    Next,
    /// Shift+Tab: the previous one.
    Prev,
    /// The arrow towards the end: like [`Key::Next`], and in a group of radio buttons it also
    /// chooses the one it lands on.
    Down,
    /// The arrow towards the start.
    Up,
    /// Enter or Space: presses the focused widget.
    Activate,
    /// Escape or a back button: asks to close the open dialog (it sends `Dismissed`); nothing else uses it.
    Cancel,
    /// The left arrow: lowers the focused slider a step.
    Left,
    /// The right arrow: raises the focused slider a step.
    Right,
}

impl<const N: usize> Tree<N> {
    /// Whether node `i` can take the focus: something that can be pressed and is enabled.
    pub(crate) fn focusable(&self, i: usize) -> bool {
        let n = &self.nodes[i];
        n.alive && n.kind.pressable() && !n.disabled_eff && !self.blocked(i)
    }

    /// Gives the focus to `target` (or takes it away with `None`), redrawing the two that change.
    pub(crate) fn set_focus(&mut self, target: Option<usize>, visible: bool) {
        let new = target.map(|i| self.id_of(i));
        let changed = self.focus != new || self.focus_visible != visible;
        if !changed {
            return;
        }
        if let Some(old) = self.focus.and_then(|id| self.slot(id)) {
            self.request(old, Flags::PAINT);
        }
        self.preedit = crate::textinput::InputText::EMPTY;
        let is_text =
            |nodes: &[crate::tree::Node], i: usize| matches!(nodes[i].kind, Kind::TextInput { .. });
        let old_text = self
            .focus
            .and_then(|id| self.slot(id))
            .filter(|&i| is_text(&self.nodes, i));
        let new_text = target.filter(|&i| is_text(&self.nodes, i));
        for i in old_text.into_iter().chain(new_text) {
            self.request(i, Flags::LAYOUT | Flags::PAINT);
        }
        self.focus = new;
        self.focus_visible = visible && new.is_some();
        if let Some(i) = target {
            self.request(i, Flags::PAINT);
            if visible {
                // At the next layout, when the positions are fresh.
                self.pending_reveal = Some(self.id_of(i));
                self.request(i, Flags::LAYOUT);
            }
        }
    }

    /// The focusable node after (or before) the focused one, in drawing order, wrapping round.
    fn focus_step(&self, forward: bool) -> Option<usize> {
        let root = usize::from(self.root?);
        let cur = self.focus.and_then(|id| self.slot(id));
        let (mut first, mut last, mut before, mut after) = (None, None, None, None);
        let mut seen = false;
        let mut at = Some(root);
        while let Some(i) = at {
            if self.focusable(i) {
                first = first.or(Some(i));
                last = Some(i);
                if Some(i) == cur {
                    seen = true;
                } else if seen {
                    after = after.or(Some(i));
                } else {
                    before = Some(i);
                }
            }
            at = self.preorder_next(i, root);
        }
        if forward {
            after.or(first)
        } else {
            before.or(last)
        }
    }

    /// Focuses `id` and shows the focus ring. False when it cannot take the focus (stale handle,
    /// disabled, or not something that can be pressed).
    pub fn focus(&mut self, id: NodeId) -> bool {
        match self.slot(id) {
            Some(i) if self.focusable(i) => {
                self.set_focus(Some(i), true);
                true
            }
            _ => false,
        }
    }

    /// Takes the focus away from whatever has it.
    pub fn clear_focus(&mut self) {
        self.set_focus(None, false);
    }

    /// The focused widget, if any.
    #[must_use]
    pub fn focused(&self) -> Option<NodeId> {
        self.focus.filter(|&id| self.slot(id).is_some())
    }

    /// Whether `id` has the focus.
    #[must_use]
    pub fn is_focused(&self, id: NodeId) -> bool {
        self.focused() == Some(id)
    }

    /// The key pass: moves the focus or presses the focused widget. Like the pointer pass it only
    /// changes state and queues [`Action`](crate::Action)s.
    pub fn key(&mut self, key: Key) {
        match key {
            Key::Cancel => self.dismiss_modal(),
            Key::Activate => {
                if let Some(i) = self.focus.and_then(|id| self.slot(id))
                    && self.focusable(i)
                {
                    self.set_focus(Some(i), true);
                    self.activate(i);
                }
            }
            Key::Left | Key::Right if self.focused_input_public().is_some() => {
                self.edit(crate::textinput::Edit::Left(false).towards(key == Key::Right));
            }
            Key::Left | Key::Right => {
                if let Some(i) = self.focus.and_then(|id| self.slot(id))
                    && self.focusable(i)
                    && matches!(
                        self.nodes[i].kind,
                        Kind::Slider { .. } | Kind::Splitter { .. }
                    )
                {
                    self.set_focus(Some(i), true);
                    let (Kind::Slider { permille } | Kind::Splitter { permille }) =
                        self.nodes[i].kind
                    else {
                        return;
                    };
                    let to = if key == Key::Left {
                        permille.saturating_sub(SLIDER_STEP)
                    } else {
                        permille.saturating_add(SLIDER_STEP).min(1000)
                    };
                    self.move_slider(i, to);
                }
            }
            Key::Next | Key::Prev | Key::Down | Key::Up => {
                let forward = matches!(key, Key::Next | Key::Down);
                let from = self.focus.and_then(|id| self.slot(id));
                let Some(to) = self.focus_step(forward) else {
                    return;
                };
                self.set_focus(Some(to), true);
                // Arrows inside a group of radio buttons choose as they go.
                let arrow = matches!(key, Key::Up | Key::Down);
                if arrow
                    && let Some(from) = from
                    && from != to
                    && matches!(self.nodes[from].kind, Kind::Radio { .. })
                    && matches!(self.nodes[to].kind, Kind::Radio { .. })
                    && self.nodes[from].parent == self.nodes[to].parent
                {
                    self.activate(to);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{ActionKind, Style};
    use fusuma_render::{Point, Rect};
    use fusuma_render_vello_cpu::VelloCpu;

    struct Ui {
        t: Tree<12>,
        go: NodeId,
        check: NodeId,
        radios: [NodeId; 2],
        other: NodeId,
    }

    fn ui() -> Ui {
        let mut t = Tree::<12>::new();
        let root = t.column(None, Style::new().gap(4)).unwrap();
        let go = t.button(Some(root), "Go", ButtonVariant::Primary).unwrap();
        let check = t.checkbox(Some(root), "Yes", false).unwrap();
        let g = t.column(Some(root), Style::new()).unwrap();
        let radios = [
            t.radio(Some(g), "a", true).unwrap(),
            t.radio(Some(g), "b", false).unwrap(),
        ];
        let other = t
            .button(Some(root), "End", ButtonVariant::Secondary)
            .unwrap();
        t.update(
            &fusuma_render_vello_cpu::new(8, 8).unwrap() as &VelloCpu,
            &Theme::DARK,
            Rect::new(0, 0, 200, 300),
        );
        Ui {
            t,
            go,
            check,
            radios,
            other,
        }
    }

    #[test]
    fn nothing_is_focused_at_first_and_tab_starts_at_the_first_widget() {
        let mut u = ui();
        assert_eq!(u.t.focused(), None);
        u.t.key(Key::Next);
        assert_eq!(u.t.focused(), Some(u.go));
    }

    #[test]
    fn tab_walks_the_widgets_in_order_and_wraps_both_ways() {
        let mut u = ui();
        let order = [u.go, u.check, u.radios[0], u.radios[1], u.other];
        for want in order {
            u.t.key(Key::Next);
            assert_eq!(u.t.focused(), Some(want));
        }
        u.t.key(Key::Next);
        assert_eq!(u.t.focused(), Some(u.go), "wraps to the first");
        u.t.key(Key::Prev);
        assert_eq!(u.t.focused(), Some(u.other), "and back to the last");
        u.t.clear_focus();
        u.t.key(Key::Prev);
        assert_eq!(
            u.t.focused(),
            Some(u.other),
            "shift-tab from nothing starts at the end"
        );
    }

    #[test]
    fn activate_presses_the_focused_widget() {
        let mut u = ui();
        u.t.key(Key::Activate);
        assert_eq!(u.t.pop_action(), None, "nothing focused, nothing pressed");
        u.t.focus(u.go);
        u.t.key(Key::Activate);
        let a = u.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (u.go, ActionKind::Clicked));
        u.t.focus(u.check);
        u.t.key(Key::Activate);
        assert_eq!(u.t.pop_action().unwrap().kind, ActionKind::Toggled(true));
    }

    #[test]
    fn arrows_in_a_radio_group_choose_as_they_go_and_tab_does_not() {
        let mut u = ui();
        u.t.focus(u.radios[0]);
        u.t.key(Key::Next);
        assert_eq!(u.t.focused(), Some(u.radios[1]));
        assert!(!u.t.is_checked(u.radios[1]), "tab only moves");
        u.t.key(Key::Up);
        assert_eq!(u.t.focused(), Some(u.radios[0]));
        u.t.key(Key::Down);
        assert!(u.t.is_checked(u.radios[1]) && !u.t.is_checked(u.radios[0]));
        assert_eq!(u.t.pop_action().unwrap().kind, ActionKind::Selected);
        u.t.key(Key::Down);
        assert_eq!(u.t.focused(), Some(u.other));
        assert_eq!(u.t.pop_action(), None, "leaving the group chooses nothing");
    }

    #[test]
    fn a_disabled_widget_is_skipped_and_loses_the_focus() {
        let mut u = ui();
        u.t.focus(u.check);
        u.t.set_enabled(u.check, false);
        u.t.update(
            &fusuma_render_vello_cpu::new(8, 8).unwrap() as &VelloCpu,
            &Theme::DARK,
            Rect::new(0, 0, 200, 300),
        );
        assert_eq!(u.t.focused(), None);
        u.t.focus(u.go);
        u.t.key(Key::Next);
        assert_eq!(u.t.focused(), Some(u.radios[0]), "skips the disabled one");
    }

    #[test]
    fn removing_the_focused_widget_clears_the_focus() {
        let mut u = ui();
        u.t.focus(u.other);
        u.t.remove(u.other);
        assert_eq!(u.t.focused(), None);
        u.t.key(Key::Next);
        assert_eq!(u.t.focused(), Some(u.go));
    }

    #[test]
    fn a_press_focuses_the_widget_and_a_key_shows_the_ring() {
        let mut u = ui();
        let r = u.t.rect(u.check).unwrap();
        let p = Point::new(r.x + 5, r.y + 5);
        u.t.pointer(Pointer::Down(p));
        assert!(u.t.is_focused(u.check) && !u.t.focus_visible);
        u.t.pointer(Pointer::Up(p));
        u.t.key(Key::Next);
        assert!(u.t.focus_visible);
    }

    #[test]
    fn keys_on_an_empty_tree_do_nothing() {
        let mut t = Tree::<2>::new();
        for k in [Key::Next, Key::Prev, Key::Up, Key::Down, Key::Activate] {
            t.key(k);
        }
        assert_eq!(t.focused(), None);
        assert_eq!(t.pop_action(), None);
    }

    #[test]
    fn a_stale_handle_cannot_take_the_focus() {
        let mut u = ui();
        u.t.remove(u.other);
        assert!(!u.t.focus(u.other));
    }
}
