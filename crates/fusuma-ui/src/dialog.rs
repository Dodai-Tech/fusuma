//! Dialogs: a modal panel in the middle of the screen, drawn over everything else. While one is open the widgets
//! behind it cannot be pressed, scrolled or focused; the focus stays inside it and goes back where it was when it
//! closes. The toolkit never closes one by itself: a press outside it, or Cancel (Escape), sends
//! [`ActionKind::Dismissed`](crate::ActionKind::Dismissed) and the app decides.

use fusuma_render::{Color, Rect, Renderer};

use crate::theme::Theme;
use crate::tree::{Action, ActionKind, BuildError, Flags, Kind, NodeId, Style, Tree};

/// The dimming layer behind an open dialog. Only drawn on a renderer that blends alpha: on one that does not, a
/// translucent black would be painted opaque and hide the whole screen.
const SCRIM: Color = Color::rgba(0, 0, 0, 150);

impl<const N: usize> Tree<N> {
    /// Adds a dialog under `parent` (usually the root), closed. Its children are the content; its `style` sets the
    /// padding, the gap and, if wanted, a fixed width. It is laid out out of the flow of its parent, centred on the
    /// screen at the size of its content (cut to the screen less a margin), and drawn last. Show it with
    /// [`open_dialog`](Self::open_dialog).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn dialog(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Dialog {
                open: false,
                prev: None,
            },
            style,
        )
    }

    /// Whether dialog `id` is open.
    #[must_use]
    pub fn is_dialog_open(&self, id: NodeId) -> bool {
        self.slot(id)
            .is_some_and(|i| matches!(self.nodes[i].kind, Kind::Dialog { open: true, .. }))
    }

    /// Opens dialog `id`: the screen is dimmed behind it, the focus moves to its first widget, and the widgets
    /// behind it stop answering.
    pub fn open_dialog(&mut self, id: NodeId) {
        let Some(i) = self.slot(id) else { return };
        let prev = self.focused();
        let Kind::Dialog { open, prev: p } = &mut self.nodes[i].kind else {
            return;
        };
        if *open {
            return;
        }
        *open = true;
        *p = prev;
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        self.damage_all();
        if let Some(first) = self.first_focusable_in(i) {
            let visible = self.focus_visible;
            self.set_focus(Some(first), visible);
        }
    }

    /// Closes dialog `id`, and gives the focus back to what had it before the dialog opened, if that can take it.
    pub fn close_dialog(&mut self, id: NodeId) {
        let Some(i) = self.slot(id) else { return };
        let Kind::Dialog { open, prev } = &mut self.nodes[i].kind else {
            return;
        };
        if !*open {
            return;
        }
        let back = prev.take();
        *open = false;
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        self.damage_all();
        // Focus is only taken back if it was inside the dialog: a program that moved it elsewhere keeps it there.
        let inside = self
            .focus
            .and_then(|f| self.slot(f))
            .is_some_and(|f| self.is_in_or_self(f, i));
        if inside {
            let target = back
                .and_then(|b| self.slot(b))
                .filter(|&b| self.focusable(b));
            let visible = self.focus_visible;
            self.set_focus(target, visible);
        }
    }

    /// Everything on the screen has to be drawn again: a dialog dims all of it.
    fn damage_all(&mut self) {
        if let Some(r) = self.root.map(usize::from) {
            let rect = self.nodes[r].rect;
            self.add_damage(rect);
        }
    }

    /// The first widget in dialog `d` that can take the focus.
    fn first_focusable_in(&self, d: usize) -> Option<usize> {
        let mut cur = self.nodes[d].first.map(usize::from);
        while let Some(i) = cur {
            if self.focusable(i) {
                return Some(i);
            }
            cur = self.preorder_next(i, d);
        }
        None
    }

    /// The next node in tree order that is not inside node `i`.
    pub(crate) fn after_subtree(&self, i: usize, top: usize) -> Option<usize> {
        let mut cur = i;
        while cur != top {
            if let Some(n) = self.nodes[cur].next {
                return Some(usize::from(n));
            }
            cur = usize::from(self.nodes[cur].parent?);
        }
        None
    }

    /// The dialog that holds the modal state: the last open one in tree order that is not itself inside a closed
    /// dialog. `None` when no dialog is open.
    pub(crate) fn modal(&self) -> Option<usize> {
        let root = usize::from(self.root?);
        let mut found = None;
        let mut cur = Some(root);
        while let Some(i) = cur {
            match self.nodes[i].kind {
                Kind::Dialog { open: true, .. } => {
                    found = Some(i);
                    cur = self.preorder_next(i, root);
                }
                // What is inside a closed dialog is not there at all.
                Kind::Dialog { open: false, .. } => cur = self.after_subtree(i, root),
                _ => cur = self.preorder_next(i, root),
            }
        }
        found
    }

    /// Whether node `i` is inside (or is) node `ancestor`.
    pub(crate) fn is_in_or_self(&self, i: usize, ancestor: usize) -> bool {
        let mut cur = Some(i);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.nodes[c].parent.map(usize::from);
        }
        false
    }

    /// Whether node `i` cannot be reached right now: it is in a closed dialog, or a dialog is open and it is not in it.
    pub(crate) fn blocked(&self, i: usize) -> bool {
        let modal = self.modal();
        let mut inside_modal = false;
        let mut cur = Some(i);
        while let Some(c) = cur {
            match self.nodes[c].kind {
                Kind::Dialog { open: false, .. } => return true,
                Kind::Dialog { open: true, .. } if Some(c) == modal => inside_modal = true,
                _ => {}
            }
            cur = self.nodes[c].parent.map(usize::from);
        }
        modal.is_some() && !inside_modal
    }

    /// Tells the app that the user asked to close the open dialog.
    pub(crate) fn dismiss_modal(&mut self) {
        if let Some(m) = self.modal() {
            let source = self.id_of(m);
            self.push_action(Action {
                source,
                kind: ActionKind::Dismissed,
            });
        }
    }

    /// Draws the open dialogs over what is already drawn: a dimming layer over the whole screen where the renderer
    /// can blend, then the dialog and what is in it. `region` limits the drawing to what overlaps it.
    pub(crate) fn paint_dialogs<R: Renderer>(
        &self,
        r: &mut R,
        theme: &Theme,
        region: Option<Rect>,
    ) -> Result<(), R::Error> {
        let Some(root) = self.root.map(usize::from) else {
            return Ok(());
        };
        let alpha = r.capabilities().alpha;
        let mut cur = Some(root);
        while let Some(i) = cur {
            match self.nodes[i].kind {
                Kind::Dialog { open: true, .. } => {
                    if alpha {
                        r.fill_rect(self.nodes[root].rect, 0, SCRIM)?;
                    }
                    let seen = region.is_none_or(|g| {
                        let o = self.nodes[i].rect.intersect(&g);
                        o.w > 0 && o.h > 0
                    });
                    if seen {
                        self.paint_walk(i, r, theme, region)?;
                    }
                    cur = self.preorder_next(i, root);
                }
                Kind::Dialog { open: false, .. } => cur = self.after_subtree(i, root),
                _ => cur = self.preorder_next(i, root),
            }
        }
        Ok(())
    }

    /// The panel of an open dialog: a raised surface with an outline.
    pub(crate) fn paint_dialog_panel<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let rect = self.nodes[i].rect;
        let radius = theme.radius(crate::theme::Round::Large);
        r.fill_rect(rect, radius, theme.colors.surface_raised)?;
        r.stroke_rect(rect, radius, theme.border_width, theme.colors.border_strong)
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{
        Capabilities, Color, Image, PathCmd, Point, Probe, Rect, Renderer, Size, TextStyle,
    };
    use fusuma_render_vello_cpu::VelloCpu;

    use crate::event::Pointer;
    use crate::focus::Key;
    use crate::theme::{ButtonVariant, TextRole, Theme};
    use crate::tree::{ActionKind, CrossAlign, Length, NodeId, Style, Tree};

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 200;
    const H: u32 = 240;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    struct Ui {
        t: Tree<32>,
        behind: NodeId,
        other: NodeId,
        dialog: NodeId,
        cancel: NodeId,
        ok: NodeId,
        list: NodeId,
    }

    /// A page with two buttons and a scrolling list, and a closed dialog with a label and two buttons.
    fn ui() -> Ui {
        let mut t = Tree::<32>::new();
        let root = t
            .column(
                None,
                Style::new().gap(8).padding(8).align(CrossAlign::Stretch),
            )
            .unwrap();
        let behind = t
            .button(Some(root), "Behind", ButtonVariant::Primary)
            .unwrap();
        let other = t
            .button(Some(root), "Other", ButtonVariant::Secondary)
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
        let dialog = t
            .dialog(
                Some(root),
                Style::new()
                    .width(Length::Fixed(140))
                    .padding(10)
                    .gap(6)
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        t.label(Some(dialog), "Sure?", TextRole::Title).unwrap();
        let row = t
            .row(Some(dialog), Style::new().gap(6).align(CrossAlign::Stretch))
            .unwrap();
        let cancel = t.button(Some(row), "No", ButtonVariant::Secondary).unwrap();
        let ok = t.button(Some(row), "Yes", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, W, H));
        Ui {
            t,
            behind,
            other,
            dialog,
            cancel,
            ok,
            list,
        }
    }

    fn relayout(u: &mut Ui) {
        u.t.update(&screen(), &THEME, Rect::new(0, 0, W, H));
    }

    fn centre(u: &Ui, id: NodeId) -> Point {
        let r = u.t.rect(id).unwrap();
        Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
    }

    fn tap(u: &mut Ui, p: Point) {
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            u.t.pointer(e);
        }
    }

    fn open(u: &mut Ui) {
        u.t.open_dialog(u.dialog);
        relayout(u);
    }

    #[test]
    fn a_closed_dialog_takes_no_room_and_its_widgets_cannot_be_reached() {
        let u = ui();
        assert!(!u.t.is_dialog_open(u.dialog));
        let r = u.t.rect(u.dialog).unwrap();
        assert_eq!((r.w, r.h), (0, 0));
        // The list is where it would be with no dialog: right under the two buttons.
        let (a, b) = (u.t.rect(u.other).unwrap(), u.t.rect(u.list).unwrap());
        assert_eq!(b.y, a.bottom() + 8, "no gap was reserved for the dialog");
    }

    #[test]
    fn a_closed_dialogs_buttons_do_not_answer_a_press_where_they_would_be() {
        let mut u = ui();
        let at = Point::new(100, 120);
        // Where the closed dialog's (zero-sized) children could land: a tap there reaches whatever is behind, or nothing.
        tap(&mut u, at);
        assert!(
            u.t.pop_action()
                .is_none_or(|a| a.source != u.ok && a.source != u.cancel)
        );
        assert_eq!(u.t.pop_action(), None);
    }

    #[test]
    fn an_open_dialog_is_centred_at_its_size_and_cut_to_the_screen_less_a_margin() {
        let mut u = ui();
        open(&mut u);
        let r = u.t.rect(u.dialog).unwrap();
        assert_eq!(r.w, 140, "the width it asked for");
        assert!(
            (r.x + (r.w / 2) as i32 - (W / 2) as i32).abs() <= 1,
            "{r:?} is centred across"
        );
        assert!(
            (r.y + (r.h / 2) as i32 - (H / 2) as i32).abs() <= 1,
            "{r:?} is centred down"
        );
        // Too wide for the screen: cut to the screen less a margin on each side.
        u.t.set_style(
            u.dialog,
            Style::new()
                .width(Length::Fixed(900))
                .padding(10)
                .gap(6)
                .align(CrossAlign::Stretch),
        );
        relayout(&mut u);
        let r = u.t.rect(u.dialog).unwrap();
        assert_eq!(r.w, W - 2 * THEME.spacing.lg);
        assert_eq!(r.x, THEME.spacing.lg as i32);
    }

    #[test]
    fn while_a_dialog_is_open_nothing_behind_it_answers_and_what_is_in_it_does() {
        let mut u = ui();
        open(&mut u);
        let p = centre(&u, u.behind);
        tap(&mut u, p);
        let a = u.t.pop_action().unwrap();
        assert_eq!(
            (a.source, a.kind),
            (u.dialog, ActionKind::Dismissed),
            "a press outside only asks to close"
        );
        assert_eq!(u.t.pop_action(), None, "and Behind was not clicked");
        u.t.pointer(Pointer::Move(p));
        assert!(!u.t.is_hovered(u.behind), "nor hovered");
        let p = centre(&u, u.ok);
        tap(&mut u, p);
        let a = u.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (u.ok, ActionKind::Clicked));
    }

    #[test]
    fn a_press_on_the_panel_itself_does_nothing_and_does_not_dismiss() {
        let mut u = ui();
        open(&mut u);
        let r = u.t.rect(u.dialog).unwrap();
        tap(&mut u, Point::new(r.x + 3, r.y + 3));
        assert_eq!(u.t.pop_action(), None);
    }

    #[test]
    fn the_wheel_and_a_drag_cannot_scroll_what_is_behind_a_dialog() {
        let mut u = ui();
        let at = u.t.rect(u.list).unwrap();
        open(&mut u);
        u.t.pointer(Pointer::Wheel(Point::new(at.x + 10, at.y + 10), 40));
        assert_eq!(u.t.scroll_offset(u.list), Some(0));
    }

    #[test]
    fn cancel_asks_to_close_the_open_dialog_and_means_nothing_without_one() {
        let mut u = ui();
        u.t.key(Key::Cancel);
        assert_eq!(u.t.pop_action(), None);
        open(&mut u);
        u.t.key(Key::Cancel);
        let a = u.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (u.dialog, ActionKind::Dismissed));
        assert!(u.t.is_dialog_open(u.dialog), "nothing closes by itself");
    }

    #[test]
    fn the_focus_moves_into_the_dialog_stays_there_and_comes_back() {
        let mut u = ui();
        assert!(u.t.focus(u.other));
        open(&mut u);
        assert_eq!(
            u.t.focused(),
            Some(u.cancel),
            "the first widget in the dialog"
        );
        for want in [u.ok, u.cancel, u.ok] {
            u.t.key(Key::Next);
            assert_eq!(u.t.focused(), Some(want), "Tab wraps inside the dialog");
        }
        u.t.key(Key::Prev);
        u.t.key(Key::Prev);
        assert_eq!(u.t.focused(), Some(u.ok), "and Shift+Tab too");
        assert!(
            !u.t.focus(u.behind),
            "a widget behind cannot take the focus"
        );
        u.t.close_dialog(u.dialog);
        relayout(&mut u);
        assert_eq!(u.t.focused(), Some(u.other), "back where it was");
        assert!(!u.t.is_dialog_open(u.dialog));
        let p = centre(&u, u.behind);
        tap(&mut u, p);
        assert_eq!(
            u.t.pop_action().map(|a| a.source),
            Some(u.behind),
            "and the page answers again"
        );
    }

    #[test]
    fn closing_gives_the_focus_back_only_if_it_was_in_the_dialog_and_the_old_one_still_exists() {
        let mut u = ui();
        assert!(u.t.focus(u.other));
        open(&mut u);
        u.t.remove(u.other);
        u.t.close_dialog(u.dialog);
        relayout(&mut u);
        assert_eq!(u.t.focused(), None, "the old widget is gone");
        // Opened with nothing focused: closing leaves nothing focused.
        u.t.clear_focus();
        open(&mut u);
        u.t.close_dialog(u.dialog);
        assert_eq!(u.t.focused(), None);
    }

    #[test]
    fn opening_and_closing_twice_and_calls_on_the_wrong_node_are_harmless() {
        let mut u = ui();
        open(&mut u);
        u.t.open_dialog(u.dialog);
        u.t.close_dialog(u.dialog);
        u.t.close_dialog(u.dialog);
        u.t.open_dialog(u.behind);
        u.t.close_dialog(u.behind);
        assert!(!u.t.is_dialog_open(u.behind) && !u.t.is_dialog_open(u.dialog));
        let stale = u.ok;
        u.t.remove(stale);
        u.t.open_dialog(stale);
        assert!(!u.t.is_dialog_open(stale));
    }

    #[test]
    fn a_dialog_opened_second_is_the_one_that_is_modal_and_closing_it_returns_to_the_first() {
        let mut u = ui();
        let root = u.t.root().unwrap();
        let second =
            u.t.dialog(
                Some(root),
                Style::new().width(Length::Fixed(100)).padding(6),
            )
            .unwrap();
        let ok2 =
            u.t.button(Some(second), "Fine", ButtonVariant::Primary)
                .unwrap();
        relayout(&mut u);
        open(&mut u);
        u.t.open_dialog(second);
        relayout(&mut u);
        assert_eq!(u.t.focused(), Some(ok2));
        // A point on the first dialog's panel that the narrower second one does not cover.
        let first = u.t.rect(u.dialog).unwrap();
        tap(
            &mut u,
            Point::new(first.x + 3, first.y + (first.h / 2) as i32),
        );
        assert_eq!(
            u.t.pop_action().map(|a| (a.source, a.kind)),
            Some((second, ActionKind::Dismissed)),
            "the top one is modal"
        );
        u.t.close_dialog(second);
        relayout(&mut u);
        assert_eq!(u.t.focused(), Some(u.cancel), "back in the first dialog");
        let p = centre(&u, u.ok);
        tap(&mut u, p);
        assert_eq!(u.t.pop_action().map(|a| a.source), Some(u.ok));
    }

    fn paint(u: &mut Ui, s: &mut impl Renderer) {
        u.t.update(&*s, &THEME, Rect::new(0, 0, W, H));
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(s, &THEME).unwrap();
    }

    #[test]
    fn an_open_dialog_dims_the_screen_and_is_drawn_over_what_follows_it_in_the_tree() {
        let mut u = ui();
        let mut s = screen();
        paint(&mut u, &mut s);
        let behind = u.t.rect(u.behind).unwrap();
        let spot = (behind.x + 2, behind.y + 2);
        let plain = s.pixel(spot.0, spot.1).unwrap();
        open(&mut u);
        paint(&mut u, &mut s);
        let dimmed = s.pixel(spot.0, spot.1).unwrap();
        assert!(
            dimmed.r < plain.r && dimmed.g < plain.g,
            "{plain:?} became {dimmed:?}"
        );
        let d = u.t.rect(u.dialog).unwrap();
        assert_eq!(
            s.pixel(d.x + 5, d.y + (d.h / 2) as i32),
            Some(THEME.colors.surface_raised),
            "the panel is not dimmed"
        );
        let corner = s.pixel(1, 1).unwrap();
        assert_ne!(
            corner, THEME.colors.background,
            "the whole screen is dimmed, not only the widgets"
        );
        u.t.close_dialog(u.dialog);
        paint(&mut u, &mut s);
        assert_eq!(
            s.pixel(spot.0, spot.1),
            Some(plain),
            "closing restores the picture"
        );
    }

    #[test]
    fn a_dialog_that_comes_first_in_the_tree_is_still_drawn_over_everything_after_it() {
        use crate::theme::Surface;
        let mut t = Tree::<16>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let dialog = t
            .dialog(
                Some(root),
                Style::new().width(Length::Fixed(120)).padding(10),
            )
            .unwrap();
        t.label(Some(dialog), "Hello", TextRole::Title).unwrap();
        // A panel as big as the screen, after the dialog in the tree: it covers the dialog's place.
        t.column(
            Some(root),
            Style::new()
                .height(Length::Fixed(240))
                .background(Surface::Surface),
        )
        .unwrap();
        t.open_dialog(dialog);
        let mut s = screen();
        t.update(&s, &THEME, Rect::new(0, 0, W, H));
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        let d = t.rect(dialog).unwrap();
        assert_eq!(
            s.pixel(d.x + 5, d.y + (d.h / 2) as i32),
            Some(THEME.colors.surface_raised),
            "the dialog is on top of the panel that follows it"
        );
    }

    /// A renderer that says it cannot blend, like the integer one: it must get no dimming layer.
    struct NoAlpha(VelloCpu);

    impl Renderer for NoAlpha {
        type Error = <VelloCpu as Renderer>::Error;
        fn size(&self) -> Size {
            self.0.size()
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                alpha: false,
                antialiasing: true,
            }
        }
        fn save(&mut self) -> Result<(), Self::Error> {
            self.0.save()
        }
        fn restore(&mut self) -> Result<(), Self::Error> {
            self.0.restore()
        }
        fn clip(&mut self, r: Rect) -> Result<(), Self::Error> {
            self.0.clip(r)
        }
        fn translate(&mut self, dx: i32, dy: i32) -> Result<(), Self::Error> {
            self.0.translate(dx, dy)
        }
        fn clear(&mut self, c: Color) -> Result<(), Self::Error> {
            self.0.clear(c)
        }
        fn fill_rect(&mut self, r: Rect, rad: u32, c: Color) -> Result<(), Self::Error> {
            self.0.fill_rect(r, rad, c)
        }
        fn stroke_rect(&mut self, r: Rect, rad: u32, w: u32, c: Color) -> Result<(), Self::Error> {
            self.0.stroke_rect(r, rad, w, c)
        }
        fn fill_circle(&mut self, p: Point, rad: u32, c: Color) -> Result<(), Self::Error> {
            self.0.fill_circle(p, rad, c)
        }
        fn fill_path(&mut self, p: &[PathCmd], c: Color) -> Result<(), Self::Error> {
            self.0.fill_path(p, c)
        }
        fn stroke_path(&mut self, p: &[PathCmd], w: u32, c: Color) -> Result<(), Self::Error> {
            self.0.stroke_path(p, w, c)
        }
        fn draw_text(&mut self, t: &str, p: Point, s: &TextStyle) -> Result<(), Self::Error> {
            self.0.draw_text(t, p, s)
        }
        fn measure_text(&self, t: &str, s: &TextStyle) -> Size {
            self.0.measure_text(t, s)
        }
        fn draw_image(&mut self, i: &Image<'_>, p: Point) -> Result<(), Self::Error> {
            self.0.draw_image(i, p)
        }
    }

    #[test]
    fn a_renderer_that_cannot_blend_gets_no_dimming_layer_so_the_screen_is_not_blacked_out() {
        let mut u = ui();
        let mut s = NoAlpha(screen());
        open(&mut u);
        paint(&mut u, &mut s);
        let behind = u.t.rect(u.behind).unwrap();
        let before = THEME
            .button(ButtonVariant::Primary, crate::theme::ButtonState::Idle)
            .background;
        assert_eq!(
            s.0.pixel(behind.x + 3, behind.y + 3),
            Some(before),
            "the page behind is as it was"
        );
        let d = u.t.rect(u.dialog).unwrap();
        assert_eq!(
            s.0.pixel(d.x + 5, d.y + (d.h / 2) as i32),
            Some(THEME.colors.surface_raised),
            "and the dialog is still drawn"
        );
    }

    #[test]
    fn a_partial_redraw_with_a_dialog_open_is_the_same_picture_as_a_full_one() {
        let mut u = ui();
        open(&mut u);
        let mut full = screen();
        paint(&mut u, &mut full);
        // Now only the part of the dialog around its buttons.
        let mut part = screen();
        paint(&mut u, &mut part);
        let b = u.t.rect(u.ok).unwrap();
        let region = Rect::new(b.x - 5, b.y - 5, b.w + 10, b.h + 10);
        part.save().unwrap();
        part.clip(region).unwrap();
        part.clear(THEME.colors.background).unwrap();
        u.t.paint_region(&mut part, &THEME, region).unwrap();
        part.restore().unwrap();
        for y in 0..H as i32 {
            for x in 0..W as i32 {
                assert_eq!(part.pixel(x, y), full.pixel(x, y), "({x}, {y})");
            }
        }
    }

    #[test]
    fn opening_and_closing_damage_the_whole_screen_and_a_scroll_under_a_dialog_is_not_shifted() {
        let mut u = ui();
        let mut s = screen();
        paint(&mut u, &mut s);
        assert_eq!(u.t.take_damage(), None);
        u.t.open_dialog(u.dialog);
        relayout(&mut u);
        assert_eq!(
            u.t.take_damage(),
            Some(Rect::new(0, 0, W, H)),
            "dimming reaches every pixel"
        );
        paint(&mut u, &mut s);
        u.t.close_dialog(u.dialog);
        relayout(&mut u);
        assert_eq!(u.t.take_damage(), Some(Rect::new(0, 0, W, H)));
        // A scroll from code while a dialog is open is redrawn, not shifted.
        paint(&mut u, &mut s);
        u.t.open_dialog(u.dialog);
        relayout(&mut u);
        paint(&mut u, &mut s);
        u.t.scroll_to(u.list, 30);
        assert!(!u.t.shift_active());
    }

    #[test]
    fn removing_an_open_dialog_frees_the_screen() {
        let mut u = ui();
        open(&mut u);
        u.t.remove(u.dialog);
        relayout(&mut u);
        let p = centre(&u, u.behind);
        tap(&mut u, p);
        assert_eq!(u.t.pop_action().map(|a| a.source), Some(u.behind));
    }
}
