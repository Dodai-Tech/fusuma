//! Drag and drop inside the app: a node that can be picked up and nodes that can take what is dropped on them. The tree does the
//! pointer work (the pick-up, the ghost that follows the finger, the highlight of the target under it) and tells the app what
//! happened; moving the data is the app's.
//!
//! * A node that asked ([`Tree::set_draggable`]) is picked up when a press on it travels [`DRAG_SLOP`] pixels: the press is
//!   then the drag's, so it is not a click and a scroll container under it lets go. [`ActionKind::DragStarted`] is sent.
//! * While it is held, a rounded box with the node's label follows the pointer, and the drop target under it
//!   ([`Tree::set_drop_target`]) is outlined.
//! * Letting go over a target sends [`ActionKind::Dropped`] (from the dragged node, with the target); anywhere else, leaving the
//!   window, Escape or the dragged node going away sends [`ActionKind::DragCancelled`].

use fusuma_render::{Point, Rect, Renderer, Size};

use crate::theme::Theme;
use crate::tree::{Action, ActionKind, Flags, NodeId, Tree};

/// How far a press on a draggable node must travel to pick it up, in pixels.
pub const DRAG_SLOP: i64 = 8;

pub(crate) const DRAG: u8 = 8;
pub(crate) const DROP: u8 = 16;

/// A drag in progress.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DragState {
    pub(crate) source: NodeId,
    /// Where on the node it was picked up, from its top left.
    pub(crate) grab: Point,
    /// Where the pointer is.
    pub(crate) at: Point,
    /// The drop target under the pointer.
    pub(crate) over: Option<NodeId>,
}

impl<const N: usize> Tree<N> {
    /// Lets node `id` be picked up and dragged (see the module docs). A node that is a button still clicks when it is pressed and
    /// released without moving.
    pub fn set_draggable(&mut self, id: NodeId, on: bool) {
        self.set_dnd_bit(id, DRAG, on);
    }

    /// Lets node `id` take what is dropped on it. It is outlined while a drag is over it.
    pub fn set_drop_target(&mut self, id: NodeId, on: bool) {
        self.set_dnd_bit(id, DROP, on);
    }

    fn set_dnd_bit(&mut self, id: NodeId, bit: u8, on: bool) {
        if let Some(i) = self.slot(id) {
            if on {
                self.nodes[i].gestures |= bit;
            } else {
                self.nodes[i].gestures &= !bit;
            }
        }
    }

    /// The node being dragged, if a drag is in progress.
    #[must_use]
    pub fn dragging(&self) -> Option<NodeId> {
        self.dragging.map(|d| d.source)
    }

    /// The drop target under the pointer during a drag.
    #[must_use]
    pub fn drag_over(&self) -> Option<NodeId> {
        self.dragging.and_then(|d| d.over)
    }

    /// The box that follows the pointer during a drag (for a test or a host that draws its own).
    #[must_use]
    pub fn drag_ghost(&self) -> Option<Rect> {
        let d = self.dragging?;
        let size = self.slot(d.source).map(|i| self.nodes[i].rect)?;
        Some(Rect::new(
            d.at.x.saturating_sub(d.grab.x),
            d.at.y.saturating_sub(d.grab.y),
            size.w,
            size.h,
        ))
    }

    /// The press on node `i` has gone far enough: pick it up.
    pub(crate) fn drag_begin(&mut self, i: usize, start: Point, at: Point) {
        let r = self.nodes[i].rect;
        self.dragging = Some(DragState {
            source: self.id_of(i),
            grab: Point::new(start.x - r.x, start.y - r.y),
            at,
            over: None,
        });
        self.push_action(Action {
            source: self.id_of(i),
            kind: ActionKind::DragStarted,
        });
        self.drag_to(at);
    }

    /// The innermost drop target under `p` that is not the dragged node or inside it.
    fn drop_at(&self, p: Point, source: usize) -> Option<usize> {
        let root = usize::from(self.root?);
        let mut found = None;
        let mut cur = Some(root);
        while let Some(i) = cur {
            let n = &self.nodes[i];
            if i != source
                && n.gestures & DROP != 0
                && !n.disabled_eff
                && !n.hidden
                && n.rect.contains(p)
                && self.in_viewports(i, p)
                && !self.is_inside(i, source)
                && !self.blocked(i)
            {
                found = Some(i);
            }
            cur = self.preorder_next(i, root);
        }
        found
    }

    /// A rectangle wide enough to cover the ghost and its outline.
    fn dnd_damage(r: Rect) -> Rect {
        Rect::new(
            r.x.saturating_sub(4),
            r.y.saturating_sub(4),
            r.w.saturating_add(8),
            r.h.saturating_add(8),
        )
    }

    /// The pointer moved to `p` during a drag.
    pub(crate) fn drag_to(&mut self, p: Point) {
        let Some(mut d) = self.dragging else { return };
        let Some(src) = self.slot(d.source) else {
            // The dragged node went away: nothing to carry any more.
            self.drag_end(None);
            return;
        };
        if let Some(old) = self.drag_ghost() {
            self.add_damage(Self::dnd_damage(old));
        }
        let over = self.drop_at(p, src).map(|i| self.id_of(i));
        if over != d.over {
            for t in [d.over, over].into_iter().flatten() {
                if let Some(i) = self.slot(t) {
                    let r = self.nodes[i].rect;
                    self.add_damage(Self::dnd_damage(r));
                }
            }
        }
        d.at = p;
        d.over = over;
        self.dragging = Some(d);
        if let Some(new) = self.drag_ghost() {
            self.add_damage(Self::dnd_damage(new));
        }
        if let Some(root) = self.root.map(usize::from) {
            self.request(root, Flags::PAINT);
        }
    }

    /// The drag is over: dropped on `target`, or cancelled with `None`.
    pub(crate) fn drag_end(&mut self, target: Option<NodeId>) {
        let Some(d) = self.dragging.take() else {
            return;
        };
        for r in [
            self.slot(d.source).map(|_| ()).and(self.drag_ghost_of(d)),
            d.over
                .and_then(|t| self.slot(t))
                .map(|i| self.nodes[i].rect),
        ]
        .into_iter()
        .flatten()
        {
            self.add_damage(Self::dnd_damage(r));
        }
        let kind = target.map_or(ActionKind::DragCancelled, ActionKind::Dropped);
        self.push_action(Action {
            source: d.source,
            kind,
        });
        if let Some(root) = self.root.map(usize::from) {
            self.request(root, Flags::PAINT);
        }
    }

    fn drag_ghost_of(&self, d: DragState) -> Option<Rect> {
        let size = self.slot(d.source).map(|i| self.nodes[i].rect)?;
        Some(Rect::new(
            d.at.x.saturating_sub(d.grab.x),
            d.at.y.saturating_sub(d.grab.y),
            size.w,
            size.h,
        ))
    }

    /// Lets go at `p`: a drop on the target under it, or a cancel if there is none.
    pub(crate) fn drag_drop(&mut self, p: Point) {
        if self.dragging.is_none() {
            return;
        }
        self.drag_to(p);
        let over = self.drag_over();
        self.drag_end(over);
    }

    /// Cancels a drag in progress (Escape, the pointer leaving the window).
    pub(crate) fn drag_cancel(&mut self) {
        self.drag_end(None);
    }

    /// Draws the outline of the target under the drag and the box that follows the pointer, over everything; within `region` if
    /// there is one.
    pub(crate) fn paint_drag<R: Renderer>(
        &self,
        r: &mut R,
        theme: &Theme,
        region: Option<Rect>,
    ) -> Result<(), R::Error> {
        let Some(d) = self.dragging else {
            return Ok(());
        };
        let c = &theme.colors;
        let radius = theme.radii.md;
        if let Some(t) = d.over.and_then(|t| self.slot(t)) {
            let rect = self.nodes[t].rect;
            if region.is_none_or(|g| !rect.intersect(&Self::dnd_damage(g)).is_empty()) {
                r.stroke_rect(
                    rect,
                    radius,
                    theme.border_width.saturating_mul(2).max(2),
                    c.primary,
                )?;
            }
        }
        if let Some(ghost) = self.drag_ghost() {
            if region.is_some_and(|g| ghost.intersect(&g).is_empty()) {
                return Ok(());
            }
            r.fill_rect(
                ghost,
                radius,
                crate::theme::mix(c.surface_raised, c.primary, 300),
            )?;
            r.stroke_rect(ghost, radius, theme.border_width.max(1), c.primary)?;
            // What it carries: the node's own text, if it has any.
            if let Some(i) = self.slot(d.source)
                && let Some(text) = self.nodes[i].kind.text()
                && !text.as_str().is_empty()
            {
                let style = theme.text_style(crate::theme::TextRole::Body);
                let size: Size = r.measure_text(text.as_str(), &style);
                let at = crate::paint::centred(ghost, size);
                Self::in_clip(r, ghost, |r| r.draw_text(text.as_str(), at, &style))?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{CrossAlign, Length, Style};

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(300, 300).unwrap()
    }

    struct Ui {
        t: Tree<16>,
        card: NodeId,
        bin: NodeId,
        other: NodeId,
        plain: NodeId,
        list: NodeId,
        row: NodeId,
    }

    /// A draggable card, a bin and another box that take drops, a plain box that does not, and a scrolling list.
    fn ui() -> Ui {
        let mut t = Tree::<16>::new();
        let root = t
            .column(
                None,
                Style::new().gap(10).padding(10).align(CrossAlign::Stretch),
            )
            .unwrap();
        let card = t
            .button(Some(root), "Card", ButtonVariant::Primary)
            .unwrap();
        let bin = t
            .row(
                Some(root),
                Style::new()
                    .height(Length::Fixed(50))
                    .background(crate::Surface::Raised),
            )
            .unwrap();
        let other = t
            .row(Some(root), Style::new().height(Length::Fixed(50)))
            .unwrap();
        let plain = t
            .row(Some(root), Style::new().height(Length::Fixed(30)))
            .unwrap();
        let list = t
            .scroll(
                Some(root),
                Style::new()
                    .height(Length::Fixed(60))
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        for _ in 0..5 {
            t.list_item(Some(list), "row", false).unwrap();
        }
        let row = t.children(list).next().unwrap();
        t.set_draggable(card, true);
        t.set_drop_target(bin, true);
        t.set_drop_target(other, true);
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 300));
        Ui {
            t,
            card,
            bin,
            other,
            plain,
            list,
            row,
        }
    }

    fn centre(t: &Tree<16>, id: NodeId) -> Point {
        let r = t.rect(id).unwrap();
        Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
    }

    fn actions(t: &mut Tree<16>) -> std::vec::Vec<(NodeId, ActionKind)> {
        std::iter::from_fn(|| t.pop_action())
            .map(|a| (a.source, a.kind))
            .collect()
    }

    #[test]
    fn a_press_that_travels_picks_the_node_up_and_letting_go_over_a_target_drops_it_there() {
        let mut u = ui();
        let from = centre(&u.t, u.card);
        u.t.pointer(Pointer::Move(from));
        u.t.pointer(Pointer::Down(from));
        // Within the slop it is still a press.
        u.t.pointer(Pointer::Move(Point::new(from.x + 5, from.y)));
        assert_eq!(u.t.dragging(), None);
        assert!(u.t.is_active(u.card));
        // Past it: picked up, the press is the drag's.
        u.t.pointer(Pointer::Move(Point::new(from.x + 12, from.y + 5)));
        assert_eq!(u.t.dragging(), Some(u.card));
        assert!(!u.t.is_active(u.card), "no longer a press");
        assert_eq!(actions(&mut u.t), [(u.card, ActionKind::DragStarted)]);
        // Over the bin: it is the target; the ghost keeps the grab offset.
        let over = centre(&u.t, u.bin);
        u.t.pointer(Pointer::Move(over));
        assert_eq!(u.t.drag_over(), Some(u.bin));
        let ghost = u.t.drag_ghost().unwrap();
        let card = u.t.rect(u.card).unwrap();
        assert_eq!((ghost.w, ghost.h), (card.w, card.h));
        assert_eq!(
            over.x - ghost.x,
            from.x - card.x,
            "held where it was picked up"
        );
        // Over the box that is not a target, nothing; over another target, that one.
        u.t.pointer(Pointer::Move(centre(&u.t, u.plain)));
        assert_eq!(u.t.drag_over(), None);
        u.t.pointer(Pointer::Move(centre(&u.t, u.other)));
        assert_eq!(u.t.drag_over(), Some(u.other));
        // Let go on the bin: dropped, once, with the target; and no click on the card.
        u.t.pointer(Pointer::Move(over));
        u.t.pointer(Pointer::Up(over));
        u.t.pointer(Pointer::Leave);
        assert_eq!(actions(&mut u.t), [(u.card, ActionKind::Dropped(u.bin))]);
        assert_eq!(u.t.dragging(), None);
        assert_eq!(u.t.drag_ghost(), None);
    }

    #[test]
    fn a_drag_over_nothing_the_pointer_leaving_and_escape_all_cancel_it_and_a_plain_press_still_clicks()
     {
        let mut u = ui();
        let from = centre(&u.t, u.card);
        let go = |u: &mut Ui| {
            u.t.pointer(Pointer::Move(from));
            u.t.pointer(Pointer::Down(from));
            u.t.pointer(Pointer::Move(Point::new(from.x + 20, from.y)));
            let _ = actions(&mut u.t);
        };
        // Let go over a box that takes nothing.
        go(&mut u);
        let at = centre(&u.t, u.plain);
        u.t.pointer(Pointer::Move(at));
        u.t.pointer(Pointer::Up(at));
        assert_eq!(actions(&mut u.t), [(u.card, ActionKind::DragCancelled)]);
        // The pointer leaves the window.
        go(&mut u);
        u.t.pointer(Pointer::Leave);
        assert_eq!(actions(&mut u.t), [(u.card, ActionKind::DragCancelled)]);
        // Escape.
        u.t.pointer(Pointer::Move(from));
        go(&mut u);
        u.t.key(crate::Key::Cancel);
        assert_eq!(actions(&mut u.t), [(u.card, ActionKind::DragCancelled)]);
        assert_eq!(u.t.dragging(), None);
        u.t.pointer(Pointer::Up(from));
        assert_eq!(
            actions(&mut u.t),
            [],
            "the release after a cancel is nothing"
        );
        u.t.pointer(Pointer::Leave);
        // A press and release without moving is the click it always was; a drop outside any drag does nothing.
        u.t.pointer(Pointer::Move(from));
        u.t.pointer(Pointer::Down(from));
        u.t.pointer(Pointer::Up(from));
        u.t.pointer(Pointer::Leave);
        assert_eq!(actions(&mut u.t), [(u.card, ActionKind::Clicked)]);
    }

    #[test]
    fn a_node_that_is_not_draggable_is_not_picked_up_a_target_cannot_take_itself_and_a_disabled_one_takes_nothing()
     {
        let mut u = ui();
        // The scrolling list's row: no flag, so a drag over it scrolls the list and picks up nothing.
        let from = centre(&u.t, u.row);
        u.t.pointer(Pointer::Move(from));
        u.t.pointer(Pointer::Down(from));
        u.t.pointer(Pointer::Move(Point::new(from.x, from.y - 30)));
        assert_eq!(u.t.dragging(), None);
        u.t.pointer(Pointer::Up(Point::new(from.x, from.y - 30)));
        u.t.pointer(Pointer::Leave);
        let _ = (actions(&mut u.t), u.list);
        // The card is also a target: it never takes itself.
        u.t.set_drop_target(u.card, true);
        let from = centre(&u.t, u.card);
        u.t.pointer(Pointer::Move(from));
        u.t.pointer(Pointer::Down(from));
        u.t.pointer(Pointer::Move(Point::new(from.x + 20, from.y)));
        u.t.pointer(Pointer::Move(from));
        assert_eq!(u.t.drag_over(), None, "not over itself");
        // A disabled target takes nothing.
        u.t.set_enabled(u.bin, false);
        u.t.update(&screen(), &THEME, Rect::new(0, 0, 300, 300));
        u.t.pointer(Pointer::Move(centre(&u.t, u.bin)));
        assert_eq!(u.t.drag_over(), None);
        // Turning the flags off.
        u.t.pointer(Pointer::Up(centre(&u.t, u.bin)));
        let _ = actions(&mut u.t);
        u.t.set_draggable(u.card, false);
        u.t.pointer(Pointer::Down(from));
        u.t.pointer(Pointer::Move(Point::new(from.x + 30, from.y)));
        assert_eq!(u.t.dragging(), None);
    }

    #[test]
    fn removing_the_dragged_node_mid_drag_cancels_it_and_the_ghost_and_outline_are_drawn_where_they_should_be()
     {
        let mut u = ui();
        let from = centre(&u.t, u.card);
        let mut scr = screen();
        u.t.paint_frame(&mut scr, &THEME, Rect::new(0, 0, 300, 300))
            .unwrap();
        u.t.pointer(Pointer::Move(from));
        u.t.pointer(Pointer::Down(from));
        let over = centre(&u.t, u.bin);
        u.t.pointer(Pointer::Move(over));
        u.t.update(&scr, &THEME, Rect::new(0, 0, 300, 300));
        let shown =
            u.t.paint_frame(&mut scr, &THEME, Rect::new(0, 0, 300, 300))
                .unwrap();
        assert!(shown.is_some(), "the drag asks for a frame");
        // The ghost is a box of the primary tint at the pointer, and the bin has an outline in the primary colour.
        let g = u.t.drag_ghost().unwrap();
        let c = THEME.colors;
        let edge = |x: i32, y: i32| (x - 1..=x + 2).any(|px| scr.pixel(px, y) == Some(c.primary));
        assert!(edge(g.x, g.y + g.h as i32 / 2), "its outline");
        let bin = u.t.rect(u.bin).unwrap();
        assert!(edge(bin.x, bin.y + 20), "the target's outline");
        // Moving on repaints the old places too (nothing is left behind).
        let away = Point::new(over.x + 80, centre(&u.t, u.plain).y);
        u.t.pointer(Pointer::Move(away));
        u.t.update(&scr, &THEME, Rect::new(0, 0, 300, 300));
        u.t.paint_frame(&mut scr, &THEME, Rect::new(0, 0, 300, 300))
            .unwrap();
        assert!(
            !(bin.x - 1..=bin.x + 2).any(|px| scr.pixel(px, bin.y + 20) == Some(c.primary)),
            "the outline is gone with the drag over it"
        );
        let mut full = screen();
        full.clear(c.background).unwrap();
        u.t.paint(&mut full, &THEME).unwrap();
        let (mut diff, mut total) = (0, 0);
        for y in (0..300).step_by(3) {
            for x in (0..300).step_by(3) {
                total += 1;
                if scr.pixel(x, y) != full.pixel(x, y) {
                    diff += 1;
                }
            }
        }
        assert_eq!(
            diff, 0,
            "{diff} of {total} sampled pixels differ between a partial and a full redraw"
        );
        // The node goes away mid-drag.
        u.t.remove(u.card);
        u.t.pointer(Pointer::Move(over));
        assert_eq!(u.t.dragging(), None);
        let got = std::iter::from_fn(|| u.t.pop_action())
            .map(|a| a.kind)
            .collect::<std::vec::Vec<_>>();
        assert!(got.contains(&ActionKind::DragCancelled), "{got:?}");
    }
}
