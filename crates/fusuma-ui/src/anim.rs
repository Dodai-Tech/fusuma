//! The animation pass: only widgets that asked for frames are stepped.

use fusuma_anim::{Easing, Fx};

use crate::theme::Theme;
use crate::tree::{Flags, Kind, Tree};

impl<const N: usize> Tree<N> {
    /// Steps every animation by `dt_ms` milliseconds. Returns true while something is still
    /// moving, so the caller keeps drawing frames; a tree at rest costs nothing.
    ///
    /// A button fades between its rest look and its hovered look over
    /// [`MotionTimes::fast_ms`](crate::theme::MotionTimes); a scroll container goes on after a flick and springs back
    /// from an end.
    pub fn advance(&mut self, theme: &Theme, dt_ms: u32) -> bool {
        self.clock = self.clock.saturating_add(dt_ms);
        // Scroll containers fling and bounce; a finger dragging one needs frames too.
        let scrolling = self.advance_scroll(dt_ms);
        // After the scroll step, so a bar that a fling moved this frame is shown, not faded.
        let bars = self.advance_bars(theme, dt_ms);
        // Before the buttons: their pass rebuilds the "something below is animated" flags from what is still running.
        let toasts = self.advance_toasts(theme, dt_ms);
        let fading = self.advance_buttons(theme, dt_ms);
        scrolling || bars || fading || toasts
    }

    fn advance_buttons(&mut self, theme: &Theme, dt_ms: u32) -> bool {
        let Some(root) = self.root.map(usize::from) else {
            return false;
        };
        if !self.nodes[root].sub.has(Flags::ANIM) {
            return false;
        }
        let step = if theme.motion.fast_ms == 0 {
            Fx::ONE
        } else {
            Fx::from_ratio(
                i32::try_from(dt_ms).unwrap_or(i32::MAX),
                i32::try_from(theme.motion.fast_ms).unwrap_or(i32::MAX),
            )
        };
        let mut cur = Some(root);
        while let Some(i) = cur {
            // Skip a whole subtree that has nothing animated in it.
            if !self.nodes[i].sub.has(Flags::ANIM) {
                cur = self.next_skipping_children(i, root);
                continue;
            }
            if self.nodes[i].own.has(Flags::ANIM) {
                self.step_node(i, step);
            }
            cur = self.preorder_next(i, root);
        }
        // Rebuild the "something below is animated" flags from what is still running.
        for n in &mut self.nodes {
            n.sub.remove(Flags::ANIM);
        }
        for i in 0..N {
            if self.nodes[i].alive && self.nodes[i].own.has(Flags::ANIM) {
                let mut up = Some(i);
                while let Some(c) = up {
                    self.nodes[c].sub.insert(Flags::ANIM);
                    up = self.nodes[c].parent.map(usize::from);
                }
            }
        }
        self.nodes[root].sub.has(Flags::ANIM)
    }

    /// The next node in depth-first order that is not inside `i`.
    fn next_skipping_children(&self, i: usize, top: usize) -> Option<usize> {
        let mut cur = i;
        while cur != top {
            if let Some(n) = self.nodes[cur].next {
                return Some(usize::from(n));
            }
            cur = usize::from(self.nodes[cur].parent?);
        }
        None
    }

    fn step_node(&mut self, i: usize, step: Fx) {
        let target = if self.nodes[i].disabled_eff || !self.nodes[i].hovered {
            Fx::ZERO
        } else {
            Fx::ONE
        };
        let Kind::Button { blend, .. } = &mut self.nodes[i].kind else {
            // A toast runs its own clock (`advance_toasts`) and keeps its flag.
            if !matches!(self.nodes[i].kind, Kind::Toast { .. }) {
                self.nodes[i].own.remove(Flags::ANIM);
            }
            return;
        };
        let before = *blend;
        *blend = if *blend < target {
            (*blend + step).min(target)
        } else {
            (*blend - step).max(target)
        };
        let (done, moved) = (*blend == target, *blend != before);
        if moved {
            self.request(i, Flags::PAINT);
        }
        if done {
            self.nodes[i].own.remove(Flags::ANIM);
        }
    }

    /// How far a button has faded towards its hovered or pressed look, 0 to 1, eased.
    #[must_use]
    pub fn button_blend(&self, id: crate::tree::NodeId) -> Option<Fx> {
        match self.nodes[self.slot(id)?].kind {
            Kind::Button { blend, .. } => Some(Easing::CubicOut.apply(blend)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, TextRole};
    use crate::tree::Style;
    use fusuma_render::{Point, Rect};
    use fusuma_render_vello_cpu::VelloCpu;

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(8, 8).unwrap()
    }

    fn app() -> (Tree<8>, crate::tree::NodeId, crate::tree::NodeId, Point) {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new().gap(4)).unwrap();
        let label = t.label(Some(root), "hi", TextRole::Body).unwrap();
        let b = t.button(Some(root), "go", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        let r = t.rect(b).unwrap();
        (t, label, b, Point::new(r.x + 5, r.y + 5))
    }

    #[test]
    fn a_tree_at_rest_has_no_animation_to_run() {
        let (mut t, _, b, _) = app();
        assert!(!t.advance(&THEME, 16));
        assert_eq!(t.button_blend(b), Some(Fx::ZERO));
    }

    #[test]
    fn hovering_fades_in_over_the_fast_time_and_leaving_fades_out() {
        let (mut t, _, b, p) = app();
        t.pointer(Pointer::Move(p));
        assert!(
            t.advance(&THEME, THEME.motion.fast_ms / 2),
            "half way: still moving"
        );
        let half = t.button_blend(b).unwrap();
        assert!(half > Fx::ZERO && half < Fx::ONE);
        assert!(
            !t.advance(&THEME, THEME.motion.fast_ms),
            "past the end: done"
        );
        assert_eq!(t.button_blend(b), Some(Fx::ONE));
        assert!(!t.advance(&THEME, 16), "and at rest again");
        t.pointer(Pointer::Leave);
        assert!(t.advance(&THEME, THEME.motion.fast_ms / 2));
        assert!(!t.advance(&THEME, THEME.motion.fast_ms));
        assert_eq!(t.button_blend(b), Some(Fx::ZERO));
    }

    #[test]
    fn a_press_blends_in_too_and_a_change_of_mind_reverses_from_where_it_is() {
        let (mut t, _, b, p) = app();
        t.pointer(Pointer::Down(p));
        t.advance(&THEME, 30);
        let partway = t.button_blend(b).unwrap();
        assert!(partway > Fx::ZERO);
        t.pointer(Pointer::Leave);
        t.advance(&THEME, 10);
        assert!(
            t.button_blend(b).unwrap() < partway,
            "it turns round without a jump"
        );
    }

    #[test]
    fn disabling_fades_a_hovered_button_back_to_rest() {
        let (mut t, _, b, p) = app();
        t.pointer(Pointer::Move(p));
        t.advance(&THEME, 1000);
        t.set_enabled(b, false);
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        assert!(t.advance(&THEME, 10));
        assert!(!t.advance(&THEME, 1000));
        assert_eq!(t.button_blend(b), Some(Fx::ZERO));
    }

    #[test]
    fn the_animation_request_travels_up_and_is_cleared_when_it_ends() {
        let (mut t, label, b, p) = app();
        let root = t.root().unwrap();
        t.clear_flag(Flags::PAINT | Flags::LAYOUT);
        t.pointer(Pointer::Move(p));
        let (r, bi, li) = (
            t.slot(root).unwrap(),
            t.slot(b).unwrap(),
            t.slot(label).unwrap(),
        );
        assert!(t.nodes[r].sub.has(Flags::ANIM) && t.nodes[bi].own.has(Flags::ANIM));
        assert!(
            !t.nodes[li].sub.has(Flags::ANIM),
            "the label's subtree has nothing animated"
        );
        t.advance(&THEME, 1000);
        assert!(!t.nodes[r].sub.has(Flags::ANIM) && !t.nodes[bi].own.has(Flags::ANIM));
    }

    #[test]
    fn zero_and_huge_times_and_a_zero_fast_time_are_safe() {
        let (mut t, _, b, p) = app();
        t.pointer(Pointer::Move(p));
        assert!(t.advance(&THEME, 0) || t.button_blend(b) == Some(Fx::ZERO));
        t.advance(&THEME, u32::MAX);
        assert_eq!(t.button_blend(b), Some(Fx::ONE));
        let mut instant = THEME;
        instant.motion.fast_ms = 0;
        t.pointer(Pointer::Leave);
        assert!(!t.advance(&instant, 0));
        assert_eq!(t.button_blend(b), Some(Fx::ZERO), "a zero duration jumps");
        let mut empty = Tree::<2>::new();
        assert!(!empty.advance(&THEME, 16));
    }

    #[test]
    fn button_blend_is_none_for_other_nodes_and_stale_handles() {
        let (mut t, label, b, _) = app();
        assert_eq!(t.button_blend(label), None);
        t.remove(b);
        assert_eq!(t.button_blend(b), None);
    }
}
