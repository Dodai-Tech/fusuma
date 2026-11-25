//! Menus: a list of actions in a panel that hangs from the widget that opened it, with no dimming layer. A press on
//! one of its rows or buttons sends [`ActionKind::Clicked`](crate::ActionKind::Clicked) from that row and closes the
//! menu; a press outside, or Cancel (Escape), closes it and sends
//! [`ActionKind::Dismissed`](crate::ActionKind::Dismissed) from the menu. While open it is modal like a dialog: the
//! focus stays in it and the widgets behind it do not answer.

use fusuma_render::{Rect, Size};

use crate::tree::{BuildError, CrossAlign, Kind, NodeId, Popover, Style, Tree};

/// Space between the widget and its menu.
const OFFSET: u32 = 2;

impl<const N: usize> Tree<N> {
    /// Adds a menu under `parent` (usually the root), closed. Its children are the rows (see
    /// [`list_item`](Self::list_item) and [`list_item_with_icon`](Self::list_item_with_icon), or buttons); `style`
    /// gives the padding and gap. Rows stretch to the width of the menu. Show it with
    /// [`open_menu`](Self::open_menu).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn menu(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Dialog {
                open: false,
                prev: None,
                popover: Some(Popover { anchor: None }),
                shown: 0,
            },
            style.align(CrossAlign::Stretch),
        )
    }

    /// Opens `menu` below `anchor` (above it when there is no room below), at least as wide as `anchor` and kept
    /// on the screen. The focus moves to its first row.
    pub fn open_menu(&mut self, menu: NodeId, anchor: NodeId) {
        let Some(i) = self.slot(menu) else { return };
        let anchor = self.slot(anchor).map(|a| self.id_of(a));
        if let Kind::Dialog {
            popover: Some(p), ..
        } = &mut self.nodes[i].kind
        {
            p.anchor = anchor;
        } else {
            return;
        }
        self.open_dialog(menu);
    }

    /// Closes `menu` (the same as [`close_dialog`](Self::close_dialog)).
    pub fn close_menu(&mut self, menu: NodeId) {
        self.close_dialog(menu);
    }

    /// Whether `menu` is open.
    #[must_use]
    pub fn is_menu_open(&self, menu: NodeId) -> bool {
        self.slot(menu).is_some_and(|i| {
            matches!(
                self.nodes[i].kind,
                Kind::Dialog {
                    open: true,
                    popover: Some(_),
                    ..
                }
            )
        })
    }

    /// The open menu that node `i` is in, if any.
    pub(crate) fn open_menu_of(&self, i: usize) -> Option<usize> {
        let mut cur = self.nodes[i].parent.map(usize::from);
        while let Some(c) = cur {
            if let Kind::Dialog {
                open: true,
                popover: Some(_),
                ..
            } = self.nodes[c].kind
            {
                return Some(c);
            }
            cur = self.nodes[c].parent.map(usize::from);
        }
        None
    }

    /// Where menu `i` goes: below its anchor, flipped above when it does not fit, never outside `bounds` less
    /// `margin`.
    pub(crate) fn popover_rect(&self, i: usize, p: Popover, bounds: Rect, margin: u32) -> Rect {
        let inner = Rect::new(
            bounds.x.saturating_add(margin as i32),
            bounds.y.saturating_add(margin as i32),
            bounds.w.saturating_sub(margin.saturating_mul(2)),
            bounds.h.saturating_sub(margin.saturating_mul(2)),
        );
        let anchor = p
            .anchor
            .and_then(|a| self.slot(a))
            .map_or(Rect::new(inner.x, inner.y, 0, 0), |a| self.nodes[a].rect);
        let want: Size = self.nodes[i].size;
        let w = want.w.max(anchor.w).min(inner.w);
        let h = want.h.min(inner.h);
        let clamp = |v: i64, lo: i64, hi: i64| v.clamp(lo, hi.max(lo));
        let x = clamp(
            i64::from(anchor.x),
            i64::from(inner.x),
            i64::from(inner.right()) - i64::from(w),
        );
        let below = i64::from(anchor.bottom()) + i64::from(OFFSET);
        let above = i64::from(anchor.y) - i64::from(OFFSET) - i64::from(h);
        let fits_below = below + i64::from(h) <= i64::from(inner.bottom());
        let y = if fits_below || above < i64::from(inner.y) {
            below
        } else {
            above
        };
        let y = clamp(
            y,
            i64::from(inner.y),
            i64::from(inner.bottom()) - i64::from(h),
        );
        Rect::new(x as i32, y as i32, w, h)
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Point, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use crate::event::Pointer;
    use crate::focus::Key;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{ActionKind, CrossAlign, Length, NodeId, Style, Tree};

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 200;
    const H: u32 = 240;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    struct Ui {
        t: Tree<24>,
        top: NodeId,
        bottom: NodeId,
        behind: NodeId,
        menu: NodeId,
        rows: [NodeId; 3],
    }

    /// A page with a button at the top and one at the bottom, another button and a closed menu of three rows.
    fn ui() -> Ui {
        let mut t = Tree::<24>::new();
        let root = t
            .column(
                None,
                Style::new().gap(6).padding(8).align(CrossAlign::Start),
            )
            .unwrap();
        let top = t
            .button(Some(root), "Menu", ButtonVariant::Secondary)
            .unwrap();
        let behind = t
            .button(Some(root), "Behind", ButtonVariant::Primary)
            .unwrap();
        t.spacer(Some(root), Style::new().height(Length::Fixed(120)))
            .unwrap();
        let bottom = t
            .button(Some(root), "Low", ButtonVariant::Secondary)
            .unwrap();
        let menu = t.menu(Some(root), Style::new().padding(4)).unwrap();
        let rows = [
            t.list_item_with_icon(Some(menu), 'a', "Copy", false)
                .unwrap(),
            t.list_item(Some(menu), "Paste", false).unwrap(),
            t.list_item(Some(menu), "Delete", false).unwrap(),
        ];
        t.update(&screen(), &THEME, Rect::new(0, 0, W, H));
        Ui {
            t,
            top,
            bottom,
            behind,
            menu,
            rows,
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

    #[test]
    fn a_closed_menu_takes_no_room_and_an_open_one_hangs_below_its_button() {
        let mut u = ui();
        assert_eq!(u.t.rect(u.menu).map(|r| (r.w, r.h)), Some((0, 0)));
        assert!(!u.t.is_menu_open(u.menu));
        u.t.open_menu(u.menu, u.top);
        relayout(&mut u);
        assert!(u.t.is_menu_open(u.menu));
        let (m, a) = (u.t.rect(u.menu).unwrap(), u.t.rect(u.top).unwrap());
        assert_eq!(m.x, a.x, "left edges line up");
        assert_eq!(m.y, a.bottom() + 2, "just below");
        assert!(m.w >= a.w);
        // The rows stretch to the menu.
        for r in u.rows {
            assert_eq!(u.t.rect(r).unwrap().w + 8, m.w);
        }
        u.t.close_menu(u.menu);
        relayout(&mut u);
        assert_eq!(u.t.rect(u.menu).map(|r| (r.w, r.h)), Some((0, 0)));
    }

    #[test]
    fn near_the_bottom_it_opens_upwards_and_it_stays_on_the_screen() {
        let mut u = ui();
        u.t.open_menu(u.menu, u.bottom);
        relayout(&mut u);
        let (m, a) = (u.t.rect(u.menu).unwrap(), u.t.rect(u.bottom).unwrap());
        assert!(m.bottom() <= a.y, "above: {m:?} over {a:?}");
        assert!(m.y >= 0 && m.right() <= W as i32);
        // A wide menu on a button at the right edge is moved left, not cut.
        let mut u = ui();
        u.t.set_style(u.top, Style::new().width(Length::Fixed(10)));
        u.t.set_text(
            u.rows[2],
            "A row that is a lot wider than the button that opens it",
        );
        u.t.open_menu(u.menu, u.top);
        relayout(&mut u);
        let m = u.t.rect(u.menu).unwrap();
        assert!(m.x >= 0 && m.right() <= W as i32, "{m:?}");
    }

    #[test]
    fn a_row_is_an_action_that_closes_the_menu_every_time() {
        let mut u = ui();
        for round in 0..3 {
            u.t.open_menu(u.menu, u.top);
            relayout(&mut u);
            let p = centre(&u, u.rows[1]);
            tap(&mut u, p);
            let a = u.t.pop_action().unwrap();
            assert_eq!(
                (a.source, a.kind),
                (u.rows[1], ActionKind::Clicked),
                "round {round}"
            );
            assert!(!u.t.is_menu_open(u.menu), "round {round}");
            assert_eq!(u.t.pop_action(), None);
            assert!(!u.t.is_checked(u.rows[1]), "a menu row is not a choice");
            relayout(&mut u);
        }
    }

    #[test]
    fn a_press_outside_or_escape_dismisses_it_and_reaches_nothing_behind() {
        let mut u = ui();
        u.t.open_menu(u.menu, u.top);
        relayout(&mut u);
        // Away from the menu, which hangs over the button under it.
        let p = centre(&u, u.bottom);
        tap(&mut u, p);
        let a = u.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (u.menu, ActionKind::Dismissed));
        assert!(!u.t.is_menu_open(u.menu), "it closed itself");
        assert_eq!(u.t.pop_action(), None, "the button outside was not pressed");
        relayout(&mut u);
        u.t.open_menu(u.menu, u.top);
        relayout(&mut u);
        u.t.key(Key::Cancel);
        assert_eq!(
            u.t.pop_action().map(|a| a.kind),
            Some(ActionKind::Dismissed)
        );
        assert!(!u.t.is_menu_open(u.menu));
        // With the menu closed the button works again.
        relayout(&mut u);
        let p = centre(&u, u.behind);
        tap(&mut u, p);
        assert_eq!(u.t.pop_action().map(|a| a.source), Some(u.behind));
    }

    #[test]
    fn the_keyboard_walks_the_rows_and_enter_chooses() {
        let mut u = ui();
        u.t.open_menu(u.menu, u.top);
        relayout(&mut u);
        assert_eq!(u.t.focused(), Some(u.rows[0]), "the focus moved in");
        u.t.key(Key::Next);
        assert_eq!(u.t.focused(), Some(u.rows[1]));
        u.t.key(Key::Activate);
        assert_eq!(u.t.pop_action().map(|a| a.source), Some(u.rows[1]));
        assert!(!u.t.is_menu_open(u.menu));
    }

    #[test]
    fn it_has_no_dimming_layer_and_opening_damages_only_the_menu() {
        let mut u = ui();
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        assert_eq!(u.t.take_damage(), None);
        u.t.open_menu(u.menu, u.top);
        relayout(&mut u);
        let m = u.t.rect(u.menu).unwrap();
        let damage = u.t.take_damage().unwrap();
        assert!(
            damage.w * damage.h <= (m.w + 8) * (m.h + 8) * 2,
            "{damage:?} for {m:?}"
        );
        u.t.invalidate();
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        assert_eq!(
            s.pixel(W as i32 - 2, H as i32 - 2),
            Some(THEME.colors.background),
            "the page behind is not dimmed"
        );
        let inside = s.pixel(m.x + 3, m.y + 3).unwrap();
        assert_ne!(inside, THEME.colors.background, "the panel is drawn");
    }

    #[test]
    fn odd_calls_are_ignored() {
        let mut u = ui();
        u.t.open_menu(u.top, u.behind);
        assert!(!u.t.is_menu_open(u.top), "a button is not a menu");
        let gone = u.behind;
        u.t.remove(gone);
        u.t.open_menu(u.menu, gone);
        relayout(&mut u);
        let m = u.t.rect(u.menu).unwrap();
        assert!(
            m.w > 0 && m.x >= 0 && m.right() <= W as i32,
            "an anchor that is gone: {m:?}"
        );
        u.t.open_menu(u.menu, u.top);
        u.t.open_menu(u.menu, u.top);
        u.t.close_menu(u.menu);
        u.t.close_menu(u.menu);
        assert!(!u.t.is_menu_open(u.menu));
    }
}
