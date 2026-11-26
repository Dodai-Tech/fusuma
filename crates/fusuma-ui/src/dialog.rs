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

/// How many pixels lower than its place a dialog sits while it is coming in (or going out) at `shown` thousandths.
fn slide_of(shown: u16) -> i32 {
    let left = fusuma_anim::Fx::ONE
        - fusuma_anim::Easing::CubicOut.apply(fusuma_anim::Fx::from_ratio(i32::from(shown), 1000));
    (left * fusuma_anim::Fx::from_int(SLIDE))
        .round()
        .clamp(0, SLIDE)
}

/// How far a dialog slides while it comes in, in pixels.
const SLIDE: i32 = 16;

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
                popover: None,
                shown: 0,
                sheet: false,
            },
            style,
        )
    }

    /// Adds a bottom sheet: a dialog that rises from the bottom edge, as wide as the screen and as tall as its content (at most
    /// three quarters of the screen). It is modal like [`dialog`](Self::dialog): the screen is dimmed behind it, a press outside
    /// dismisses it, and it is shown with [`open_dialog`](Self::open_dialog).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn bottom_sheet(
        &mut self,
        parent: Option<NodeId>,
        style: Style,
    ) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Dialog {
                open: false,
                prev: None,
                popover: None,
                shown: 0,
                sheet: true,
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
        let Kind::Dialog { open, prev: p, .. } = &mut self.nodes[i].kind else {
            return;
        };
        if *open {
            return;
        }
        *open = true;
        *p = prev;
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        if !self.is_popover(i) {
            self.damage_all();
        }
        if let Some(first) = self.first_focusable_in(i) {
            let visible = self.focus_visible;
            self.set_focus(Some(first), visible);
        }
    }

    /// Closes dialog `id`, and gives the focus back to what had it before the dialog opened, if that can take it.
    pub fn close_dialog(&mut self, id: NodeId) {
        let Some(i) = self.slot(id) else { return };
        let Kind::Dialog { open, prev, .. } = &mut self.nodes[i].kind else {
            return;
        };
        if !*open {
            return;
        }
        let back = prev.take();
        *open = false;
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        if !self.is_popover(i) {
            self.damage_all();
        }
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

    /// Whether dialog `i` is a menu.
    pub(crate) fn is_popover(&self, i: usize) -> bool {
        matches!(
            self.nodes[i].kind,
            Kind::Dialog {
                popover: Some(_),
                ..
            }
        )
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
                _ if self.nodes[c].hidden => return true,
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
            // A menu closes itself; the app learns it was dismissed.
            if self.is_popover(m) {
                self.close_dialog(source);
            }
        }
    }

    /// Draws the open dialogs over what is already drawn: a dimming layer over the whole screen where the renderer
    /// can blend, then the dialog and what is in it. A dialog that is opening or closing fades its dimming layer and
    /// slides. `region` limits the drawing to what overlaps it.
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
            if self.nodes[i].hidden {
                cur = self.after_subtree(i, root);
                continue;
            }
            match self.nodes[i].kind {
                Kind::Dialog {
                    open,
                    popover,
                    shown,
                    ..
                } if open || (popover.is_none() && shown > 0) => {
                    // A menu is simply there; a dialog is as far in as `shown` says.
                    let p = if popover.is_some() { 1000 } else { shown };
                    if alpha && popover.is_none() {
                        let a = u32::from(SCRIM.a) * u32::from(p) / 1000;
                        r.fill_rect(
                            self.nodes[root].rect,
                            0,
                            Color::rgba(SCRIM.r, SCRIM.g, SCRIM.b, a as u8),
                        )?;
                    }
                    let seen = region.is_none_or(|g| {
                        let o = self.nodes[i].rect.intersect(&g);
                        o.w > 0 && o.h > 0
                    });
                    if seen {
                        let slide = if popover.is_some() { 0 } else { slide_of(p) };
                        r.save()?;
                        let moved = r
                            .translate(0, slide)
                            .and_then(|()| self.paint_walk(i, r, theme, region));
                        let restored = r.restore();
                        moved?;
                        restored?;
                    }
                    cur = self.preorder_next(i, root);
                }
                Kind::Dialog { .. } => cur = self.after_subtree(i, root),
                _ => cur = self.preorder_next(i, root),
            }
        }
        Ok(())
    }

    /// Whether dialog `i` still has to be drawn although it is closed: it is sliding out.
    pub(crate) fn dialog_visible(&self, i: usize) -> bool {
        matches!(
            self.nodes[i].kind,
            Kind::Dialog {
                popover: None,
                shown: 1..,
                ..
            }
        )
    }

    /// Moves the openness of the dialogs towards open or closed. True while one is on its way.
    pub(crate) fn advance_dialogs(&mut self, theme: &Theme, dt_ms: u32) -> bool {
        let ms = theme.motion.normal_ms;
        let mut running = false;
        for i in 0..N {
            if !self.nodes[i].alive {
                continue;
            }
            let Kind::Dialog {
                open,
                popover: None,
                shown,
                ..
            } = self.nodes[i].kind
            else {
                continue;
            };
            let target: u16 = if open { 1000 } else { 0 };
            if shown == target {
                self.nodes[i].own.remove(Flags::ANIM);
                continue;
            }
            // No time asked for, no time passes; a budget of no motion jumps.
            let step = if ms == 0 {
                1000
            } else {
                (u64::from(dt_ms) * 1000 / u64::from(ms)).min(1000) as u16
            };
            let now = if shown < target {
                shown.saturating_add(step).min(target)
            } else {
                shown.saturating_sub(step).max(target)
            };
            if let Kind::Dialog { shown, .. } = &mut self.nodes[i].kind {
                *shown = now;
            }
            if now != shown {
                self.damage_all();
                self.request_quiet(i, Flags::PAINT);
            }
            if now == 0 {
                // Gone: it takes no room again.
                self.request(i, Flags::LAYOUT | Flags::PAINT);
            }
            if now == target {
                self.nodes[i].own.remove(Flags::ANIM);
            } else {
                self.nodes[i].own.insert(Flags::ANIM);
                running = true;
            }
        }
        running
    }

    /// The panel of an open dialog: a raised surface with an outline.
    pub(crate) fn paint_dialog_panel<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let rect = self.nodes[i].rect;
        let kind = if self.is_popover(i) {
            crate::theme::PanelKind::Menu
        } else {
            crate::theme::PanelKind::Dialog
        };
        let look = theme.panel_in(self.nodes[i].class, kind);
        let radius = look.radius;
        r.fill_rect(rect, radius, look.fill)?;
        if let Some(border) = look.border {
            r.stroke_rect(rect, radius, theme.border_width, border)?;
        }
        Ok(())
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
        // Fully in: the entrance animation has its own tests.
        u.t.advance(&THEME, 1000);
        relayout(u);
    }

    #[test]
    fn a_bottom_sheet_sits_on_the_bottom_edge_as_wide_as_the_screen_and_is_modal() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new().gap(4)).unwrap();
        let behind = t
            .button(Some(root), "Behind", ButtonVariant::Primary)
            .unwrap();
        let sheet = t
            .bottom_sheet(Some(root), Style::new().padding(10).gap(6))
            .unwrap();
        let ok = t.button(Some(sheet), "Ok", ButtonVariant::Primary).unwrap();
        let scr = screen();
        let bounds = Rect::new(0, 0, W, H);
        t.update(&scr, &THEME, bounds);
        assert_eq!(
            t.rect(sheet).map(|r| (r.w, r.h)),
            Some((0, 0)),
            "closed: no room"
        );
        t.open_dialog(sheet);
        t.advance(&THEME, 1000);
        t.update(&scr, &THEME, bounds);
        let r = t.rect(sheet).unwrap();
        assert_eq!((r.x, r.w), (0, W), "as wide as the screen");
        assert_eq!(r.bottom(), H as i32, "on the bottom edge");
        assert!(
            r.h > 20 && r.h <= H / 4 * 3,
            "as tall as its content, at most three quarters: {r:?}"
        );
        let ok_rect = t.rect(ok).unwrap();
        assert!(
            r.contains(Point::new(ok_rect.x + 2, ok_rect.y + 2)),
            "its content is inside"
        );
        // Modal: a press on what is behind goes nowhere; one on the sheet's button answers.
        let b = t.rect(behind).unwrap();
        for e in [
            Pointer::Down(Point::new(b.x + 3, b.y + 3)),
            Pointer::Up(Point::new(b.x + 3, b.y + 3)),
            Pointer::Leave,
        ] {
            t.pointer(e);
        }
        assert!(t.pop_action().is_none_or(|a| a.source != behind));
        let p = Point::new(ok_rect.x + 3, ok_rect.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            t.pointer(e);
        }
        assert!(std::iter::from_fn(|| t.pop_action()).any(|a| a.source == ok));
        // A tall one is cut to three quarters of the screen.
        t.set_style(sheet, Style::new().padding(10).height(Length::Fixed(900)));
        t.update(&scr, &THEME, bounds);
        assert_eq!(t.rect(sheet).unwrap().h, H / 4 * 3);
    }

    #[test]
    fn a_bottom_sheet_in_bounds_at_the_edge_of_the_range_does_not_overflow() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let sheet = t
            .bottom_sheet(Some(root), Style::new().height(Length::Fixed(500)))
            .unwrap();
        t.open_dialog(sheet);
        t.advance(&THEME, 1000);
        t.update(&screen(), &THEME, Rect::new(0, i32::MIN, 100, u32::MAX));
        t.update(&screen(), &THEME, Rect::new(0, i32::MAX - 10, 100, 5));
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
        u.t.advance(&THEME, 1000);
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
        t.advance(&THEME, 1000);
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

    /// How dark the page behind the dialog is at `spot`, 0 to 765.
    fn dark(u: &mut Ui, spot: (i32, i32)) -> (u32, VelloCpu) {
        let mut s = screen();
        paint(u, &mut s);
        let c = s.pixel(spot.0, spot.1).unwrap();
        (u32::from(c.r) + u32::from(c.g) + u32::from(c.b), s)
    }

    #[test]
    fn a_dialog_fades_the_screen_and_slides_in_and_the_same_going_out() {
        let mut u = ui();
        let behind = u.t.rect(u.behind).unwrap();
        let spot = (behind.x + 2, behind.y + 2);
        let (plain, _) = dark(&mut u, spot);
        u.t.open_dialog(u.dialog);
        relayout(&mut u);
        let (start, s0) = dark(&mut u, spot);
        assert_eq!(start, plain, "nothing is dimmed at the first frame");
        let d = u.t.rect(u.dialog).unwrap();
        let panel_top = |s: &VelloCpu| {
            (d.y..H as i32)
                .find(|&y| s.pixel(d.x + (d.w / 2) as i32, y) == Some(THEME.colors.surface_raised))
        };
        let mut levels = vec![start];
        let mut tops = vec![panel_top(&s0)];
        let step = THEME.motion.normal_ms / 5;
        for _ in 0..5 {
            assert!(
                u.t.advance(&THEME, step) || levels.len() == 5,
                "frames are wanted while it moves"
            );
            let (v, s) = dark(&mut u, spot);
            levels.push(v);
            tops.push(panel_top(&s));
        }
        assert!(
            levels.windows(2).all(|w| w[1] <= w[0]),
            "it only gets darker: {levels:?}"
        );
        assert!(
            levels[0] > levels[3] && levels[3] >= levels[5],
            "{levels:?}"
        );
        // The first frame has no dimming yet, so the page's own raised surfaces are in the way of the search.
        let tops: Vec<i32> = tops.into_iter().skip(1).flatten().collect();
        assert!(
            tops.windows(2).all(|w| w[1] <= w[0]),
            "the panel only rises: {tops:?}"
        );
        // Its outline is the first row, so the fill starts a pixel in.
        assert!(
            (d.y..=d.y + 2).contains(tops.last().unwrap()),
            "and ends in its place: {tops:?} for {d:?}"
        );
        assert!(tops[0] > d.y + 4, "it started lower: {tops:?}");
        assert!(!u.t.advance(&THEME, 50), "at rest, no more frames");
        let (full, _) = dark(&mut u, spot);
        assert_eq!(full, *levels.last().unwrap());
        // Out again: lighter step by step, and exactly the page at the end.
        u.t.close_dialog(u.dialog);
        let mut out = vec![full];
        for _ in 0..5 {
            u.t.advance(&THEME, step);
            out.push(dark(&mut u, spot).0);
        }
        assert!(
            out.windows(2).all(|w| w[1] >= w[0]),
            "it only gets lighter: {out:?}"
        );
        u.t.advance(&THEME, 50);
        assert_eq!(dark(&mut u, spot).0, plain, "back to the page");
        assert_eq!(
            u.t.rect(u.dialog).map(|r| (r.w, r.h)),
            Some((0, 0)),
            "and it takes no room again"
        );
        assert!(!u.t.needs_frame());
    }

    #[test]
    fn a_closing_dialog_no_longer_blocks_the_page_and_a_reopened_one_turns_round() {
        let mut u = ui();
        open(&mut u);
        u.t.close_dialog(u.dialog);
        u.t.advance(&THEME, THEME.motion.normal_ms / 2);
        relayout(&mut u);
        assert!(!u.t.is_dialog_open(u.dialog));
        let p = centre(&u, u.behind);
        tap(&mut u, p);
        assert_eq!(
            u.t.pop_action().map(|a| a.source),
            Some(u.behind),
            "the page answers while the dialog fades"
        );
        // Opening it again from half way goes in again, not from nothing.
        let behind = u.t.rect(u.behind).unwrap();
        let spot = (behind.x + 2, behind.y + 2);
        let (half, _) = dark(&mut u, spot);
        u.t.open_dialog(u.dialog);
        u.t.advance(&THEME, 1);
        let (again, _) = dark(&mut u, spot);
        assert!(again <= half + 3, "{again} after {half}");
        u.t.advance(&THEME, 1000);
        assert!(u.t.is_dialog_open(u.dialog));
    }

    #[test]
    fn a_motion_budget_shortens_or_removes_the_animation() {
        let behind = |u: &Ui| u.t.rect(u.behind).unwrap();
        let mut u = ui();
        let b = behind(&u);
        let spot = (b.x + 2, b.y + 2);
        let (plain, _) = dark(&mut u, spot);
        // Off: one frame of any length, even none, and it is all the way in.
        let off = THEME.with_motion(crate::MotionBudget::Off);
        u.t.open_dialog(u.dialog);
        assert!(!u.t.advance(&off, 0), "nothing to animate");
        let (v, _) = dark(&mut u, spot);
        assert!(v < plain, "dimmed at once");
        u.t.close_dialog(u.dialog);
        u.t.advance(&off, 0);
        relayout(&mut u);
        assert_eq!(dark(&mut u, spot).0, plain);
        // Reduced: half the time.
        let half = THEME.with_motion(crate::MotionBudget::Reduced);
        assert_eq!(half.motion.normal_ms, THEME.motion.normal_ms / 2);
        u.t.open_dialog(u.dialog);
        assert!(
            !u.t.advance(&half, half.motion.normal_ms),
            "in after half the usual time"
        );
        assert_eq!(THEME.with_motion(crate::MotionBudget::Full), THEME);
    }

    #[test]
    fn what_is_drawn_part_way_is_the_same_picture_whole_or_by_region() {
        let mut u = ui();
        u.t.open_dialog(u.dialog);
        u.t.advance(&THEME, THEME.motion.normal_ms / 3);
        let mut full = screen();
        paint(&mut u, &mut full);
        let b = u.t.rect(u.ok).unwrap();
        let region = Rect::new(b.x - 5, b.y - 5, b.w + 10, b.h + 30);
        let mut part = screen();
        paint(&mut u, &mut part);
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
        // Removing a dialog while it is moving frees the screen.
        let d = u.dialog;
        u.t.remove(d);
        u.t.advance(&THEME, 5000);
        paint(&mut u, &mut part);
        assert!(!u.t.needs_frame());
    }
}
