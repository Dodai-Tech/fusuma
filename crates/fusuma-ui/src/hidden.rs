//! Hidden nodes: alive but not on the screen. A hidden node and everything in it takes no room in its parent, is not
//! drawn, cannot take the focus or a press, and is not in the semantic tree, yet keeps what it holds (the text typed
//! in a field, how far a list is scrolled, a checkbox's state) for when it is shown again. It is what an accordion that
//! is folded and a tab page that is not on show need, and it costs the node one flag.

use crate::tree::{Flags, NodeId, Tree};

impl<const N: usize> Tree<N> {
    /// Hides or shows node `id` and everything in it. The room it takes is given back or taken again at the next layout.
    pub fn set_hidden(&mut self, id: NodeId, hidden: bool) {
        let Some(i) = self.slot(id) else { return };
        if self.nodes[i].hidden == hidden {
            return;
        }
        self.nodes[i].hidden = hidden;
        if hidden {
            // It leaves the screen: draw what it covered, and let go of the focus and the pointer.
            let rect = self.nodes[i].rect;
            self.add_damage(rect);
            self.leave_hidden(i);
        }
        // Its parent places its children again, and the node itself is measured (it may have changed while hidden).
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        if let Some(p) = self.nodes[i].parent {
            self.request(usize::from(p), Flags::LAYOUT | Flags::PAINT);
        }
    }

    /// Whether node `id` is hidden itself (not through an ancestor).
    #[must_use]
    pub fn is_hidden(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(|i| self.nodes[i].hidden)
    }

    /// Whether node `i` or an ancestor of it is hidden.
    pub(crate) fn in_hidden(&self, i: usize) -> bool {
        let mut cur = Some(i);
        while let Some(c) = cur {
            if self.nodes[c].hidden {
                return true;
            }
            cur = self.nodes[c].parent.map(usize::from);
        }
        false
    }

    /// The focus, a hover or a press that was inside a node that has just been hidden is let go of.
    fn leave_hidden(&mut self, i: usize) {
        let within = |t: &Self, n: usize| n == i || t.is_inside(n, i);
        if let Some(f) = self.focus.and_then(|id| self.slot(id))
            && within(self, f)
        {
            self.focus = None;
        }
        if let Some(h) = self.hovered.and_then(|id| self.slot(id))
            && within(self, h)
        {
            self.set_hover(None);
        }
        if let Some(c) = self.captured.and_then(|id| self.slot(id))
            && within(self, c)
        {
            self.set_active(c, false);
            self.captured = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Color, Point, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::focus::Key;
    use crate::theme::{ButtonVariant, TextRole, Theme};
    use crate::tree::{ActionKind, CrossAlign, Length, Style};

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 220;
    const H: u32 = 260;
    const BOUNDS: Rect = Rect::new(0, 0, W, H);

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    struct Ui {
        t: Tree<32>,
        head: NodeId,
        section: NodeId,
        field: NodeId,
        check: NodeId,
        list: NodeId,
        inner_button: NodeId,
        foot: NodeId,
    }

    /// A header button, a section (a text field, a checkbox, a scrolling list and a button) and a footer button.
    fn ui() -> Ui {
        let mut t = Tree::<32>::new();
        let root = t
            .column(
                None,
                Style::new().gap(6).padding(8).align(CrossAlign::Start),
            )
            .unwrap();
        let head = t
            .button(Some(root), "Head", ButtonVariant::Primary)
            .unwrap();
        let section = t
            .card(
                Some(root),
                Style::new().padding(6).gap(4).width(Length::Fixed(180)),
            )
            .unwrap();
        let field = t.text_input(Some(section), "abc").unwrap();
        let check = t.checkbox(Some(section), "Keep", false).unwrap();
        let list = t
            .scroll(
                Some(section),
                Style::new()
                    .height(Length::Fixed(50))
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        for k in 0..8 {
            t.list_item(Some(list), &format!("row {k}"), false).unwrap();
        }
        let inner_button = t
            .button(Some(section), "Inner", ButtonVariant::Secondary)
            .unwrap();
        let foot = t
            .button(Some(root), "Foot", ButtonVariant::Primary)
            .unwrap();
        t.update(&screen(), &THEME, BOUNDS);
        Ui {
            t,
            head,
            section,
            field,
            check,
            list,
            inner_button,
            foot,
        }
    }

    fn relayout(u: &mut Ui) {
        u.t.update(&screen(), &THEME, BOUNDS);
    }

    fn picture(u: &mut Ui) -> Vec<Color> {
        let mut s = screen();
        relayout(u);
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        (0..H as i32)
            .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
            .map(|(x, y)| s.pixel(x, y).unwrap())
            .collect()
    }

    fn tap(u: &mut Ui, p: Point) {
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            u.t.pointer(e);
        }
    }

    #[test]
    fn a_hidden_section_gives_its_room_back_and_taking_it_again_restores_every_place() {
        let mut u = ui();
        let before: Vec<Option<Rect>> =
            [u.head, u.section, u.field, u.list, u.inner_button, u.foot]
                .map(|id| u.t.rect(id))
                .into();
        let section = u.t.rect(u.section).unwrap();
        u.t.set_hidden(u.section, true);
        relayout(&mut u);
        assert!(u.t.is_hidden(u.section));
        let s = u.t.rect(u.section).unwrap();
        assert_eq!((s.w, s.h), (0, 0), "no room");
        let (head, foot) = (u.t.rect(u.head).unwrap(), u.t.rect(u.foot).unwrap());
        assert_eq!(
            foot.y,
            head.bottom() + 6,
            "the footer moved up to the header, one gap away"
        );
        assert!(foot.y < section.y + section.h as i32);
        u.t.set_hidden(u.section, false);
        relayout(&mut u);
        let after: Vec<Option<Rect>> = [u.head, u.section, u.field, u.list, u.inner_button, u.foot]
            .map(|id| u.t.rect(id))
            .into();
        assert_eq!(after, before, "every place is back");
    }

    #[test]
    fn what_it_holds_is_kept_text_a_checkbox_and_how_far_a_list_was_scrolled() {
        let mut u = ui();
        u.t.focus(u.field);
        u.t.type_text("def");
        u.t.set_checked(u.check, true);
        u.t.scroll_to(u.list, 40);
        relayout(&mut u);
        u.t.set_hidden(u.section, true);
        relayout(&mut u);
        u.t.set_hidden(u.section, false);
        relayout(&mut u);
        assert_eq!(u.t.text(u.field), Some("abcdef"));
        assert!(u.t.is_checked(u.check));
        assert_eq!(u.t.scroll_offset(u.list), Some(40));
    }

    #[test]
    fn it_is_not_drawn_and_the_page_is_what_it_would_be_without_it() {
        let mut u = ui();
        let with = picture(&mut u);
        u.t.set_hidden(u.section, true);
        let without = picture(&mut u);
        assert_ne!(with, without);
        // The same page built without the section at all.
        let mut t = Tree::<32>::new();
        let root = t
            .column(
                None,
                Style::new().gap(6).padding(8).align(CrossAlign::Start),
            )
            .unwrap();
        t.button(Some(root), "Head", ButtonVariant::Primary)
            .unwrap();
        t.button(Some(root), "Foot", ButtonVariant::Primary)
            .unwrap();
        let mut s = screen();
        t.update(&s, &THEME, BOUNDS);
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        let bare: Vec<Color> = (0..H as i32)
            .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
            .map(|(x, y)| s.pixel(x, y).unwrap())
            .collect();
        assert!(
            without == bare,
            "hidden is the same as absent, to the pixel"
        );
        u.t.set_hidden(u.section, false);
        assert!(picture(&mut u) == with, "and showing it is exactly back");
    }

    #[test]
    fn a_partial_redraw_after_hiding_and_showing_is_the_same_picture_as_a_full_one() {
        let mut u = ui();
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        let _ = u.t.take_damage();
        for hidden in [true, false, true, false] {
            u.t.set_hidden(u.section, hidden);
            u.t.update(&s, &THEME, BOUNDS);
            if let Some(d) = u.t.take_damage_in(BOUNDS) {
                s.save().unwrap();
                s.clip(d).unwrap();
                s.clear(THEME.colors.background).unwrap();
                u.t.paint_region(&mut s, &THEME, d).unwrap();
                s.restore().unwrap();
            }
            let part: Vec<Color> = (0..H as i32)
                .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
                .map(|(x, y)| s.pixel(x, y).unwrap())
                .collect();
            assert!(part == picture(&mut u), "hidden = {hidden}");
        }
    }

    #[test]
    fn nothing_inside_can_be_pressed_focused_or_found_while_it_is_hidden() {
        let mut u = ui();
        let at = u.t.rect(u.inner_button).unwrap();
        let p = Point::new(at.x + 3, at.y + 3);
        u.t.focus(u.field);
        u.t.set_hidden(u.section, true);
        relayout(&mut u);
        assert_eq!(u.t.focused(), None, "the focus let go");
        tap(&mut u, p);
        assert_eq!(u.t.pop_action(), None, "nothing is at that spot now");
        let f = u.t.rect(u.foot).unwrap();
        tap(&mut u, Point::new(f.x + 3, f.y + 3));
        assert_eq!(
            u.t.pop_action().map(|a| a.source),
            Some(u.foot),
            "the footer, moved up, works"
        );
        // The keyboard walks past it.
        u.t.key(Key::Next);
        u.t.key(Key::Next);
        let order: Vec<_> = [u.t.focused()].into();
        assert!(
            order[0] != Some(u.inner_button)
                && order[0] != Some(u.field)
                && order[0] != Some(u.check)
        );
        assert!(!u.t.focus(u.field), "it cannot be given the focus");
        // And it is not in the description of the screen.
        assert!(u.t.semantic(u.inner_button).is_none() && u.t.semantic(u.section).is_none());
        let names: Vec<_> = u.t.semantics().map(|s| s.name.to_owned()).collect();
        assert!(
            !names.iter().any(|n| n == "Inner" || n == "Keep"),
            "{names:?}"
        );
        // Shown again, it all works.
        u.t.set_hidden(u.section, false);
        relayout(&mut u);
        let at = u.t.rect(u.inner_button).unwrap();
        tap(&mut u, Point::new(at.x + 3, at.y + 3));
        let a = u.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (u.inner_button, ActionKind::Clicked));
        assert!(u.t.focus(u.field));
    }

    #[test]
    fn a_press_that_is_held_when_it_is_hidden_ends_and_a_hidden_toast_or_dialog_inside_is_not_drawn()
     {
        let mut u = ui();
        let at = u.t.rect(u.inner_button).unwrap();
        let p = Point::new(at.x + 3, at.y + 3);
        u.t.pointer(Pointer::Down(p));
        assert!(u.t.is_active(u.inner_button));
        u.t.set_hidden(u.section, true);
        assert!(!u.t.is_active(u.inner_button));
        u.t.pointer(Pointer::Up(p));
        u.t.pointer(Pointer::Leave);
        assert_eq!(
            u.t.pop_action(),
            None,
            "a press that was cut short is no click"
        );
        // A toast and a dialog inside a hidden node.
        u.t.set_hidden(u.section, false);
        let toast = u.t.toast(Some(u.section)).unwrap();
        let dialog =
            u.t.dialog(Some(u.section), Style::new().padding(8))
                .unwrap();
        u.t.label(Some(dialog), "Sure?", TextRole::Body).unwrap();
        u.t.show_toast(toast, "Hi", 5000);
        u.t.open_dialog(dialog);
        relayout(&mut u);
        u.t.advance(&THEME, 1000);
        let shown = picture(&mut u);
        u.t.set_hidden(u.section, true);
        let hidden = picture(&mut u);
        assert_ne!(shown, hidden);
        // With the toast and dialog inside, hiding still equals absence of the whole section: no scrim left behind.
        let corner = hidden[0];
        assert_eq!(corner, THEME.colors.background, "no dimming layer");
    }

    #[test]
    fn stale_handles_and_the_wrong_node_do_nothing() {
        let mut u = ui();
        let gone = u.field;
        u.t.remove(gone);
        u.t.set_hidden(gone, true);
        assert!(!u.t.is_hidden(gone));
        u.t.set_hidden(u.section, true);
        u.t.set_hidden(u.section, true);
        u.t.set_hidden(u.section, false);
        u.t.set_hidden(u.section, false);
        assert!(!u.t.is_hidden(u.section));
        // Hiding a node whose parent is hidden: the node is hidden, and so it stays when the parent is shown.
        u.t.set_hidden(u.section, true);
        u.t.set_hidden(u.inner_button, true);
        u.t.set_hidden(u.section, false);
        relayout(&mut u);
        assert!(u.t.is_hidden(u.inner_button));
        assert_eq!(u.t.rect(u.inner_button).map(|r| (r.w, r.h)), Some((0, 0)));
    }
}
