//! Icons in buttons, list items and tabs, and the tab itself: an icon (a character of the icon font, found by name
//! with `FontSet::icon`) before the text, the pair sized and placed together.

use fusuma_render::{Color, Point, Rect, Renderer, Size, TextStyle};

use crate::paint::centred;
use crate::theme::{ButtonVariant, Theme};
use crate::tree::{BuildError, Flags, Kind, Length, NodeId, SmallText, Style, Tree};

/// An icon and the text after it, measured.
#[derive(Clone, Copy, Debug)]
pub(crate) struct IconText {
    pub icon: Option<(char, Size)>,
    pub text: Size,
    pub gap: u32,
}

impl IconText {
    /// Width and height of the icon, the gap and the text together.
    pub(crate) fn size(&self) -> Size {
        let (iw, ih) = self.icon.map_or((0, 0), |(_, s)| (s.w, s.h));
        let gap = if self.icon.is_some() && self.text.w > 0 {
            self.gap
        } else {
            0
        };
        Size::new(
            iw.saturating_add(gap).saturating_add(self.text.w),
            ih.max(self.text.h),
        )
    }
}

impl<const N: usize> Tree<N> {
    /// Adds a button with an icon before its text. An empty `text` makes an icon-only button.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn button_with_icon(
        &mut self,
        parent: Option<NodeId>,
        icon: char,
        text: &str,
        variant: ButtonVariant,
    ) -> Result<NodeId, BuildError> {
        let id = self.button(parent, text, variant)?;
        self.set_icon(id, Some(icon));
        Ok(id)
    }

    /// Adds a list item with an icon before its text.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn list_item_with_icon(
        &mut self,
        parent: Option<NodeId>,
        icon: char,
        text: &str,
        selected: bool,
    ) -> Result<NodeId, BuildError> {
        let id = self.list_item(parent, text, selected)?;
        self.set_icon(id, Some(icon));
        Ok(id)
    }

    /// Adds a tab bar: a row that gives each of its tabs an equal share. Add its tabs with [`tab`](Self::tab).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn tab_bar(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.row(parent, style.align(crate::tree::CrossAlign::Stretch))
    }

    /// Adds a tab to a tab bar (or any container): the tabs that share a parent are one group with at most one
    /// selected. Choosing one sends [`ActionKind::Selected`](crate::ActionKind::Selected); the app shows the matching
    /// content. `icon` and `text` may each be left out (an empty text).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn tab(
        &mut self,
        parent: Option<NodeId>,
        icon: Option<char>,
        text: &str,
        selected: bool,
    ) -> Result<NodeId, BuildError> {
        let id = self.add(
            parent,
            Kind::Tab {
                text: SmallText::new(text),
                selected: false,
                icon,
            },
            Style::new().width(Length::Fill(1)),
        )?;
        if selected {
            self.set_checked(id, true);
        }
        Ok(id)
    }

    /// The icon of a button, list item or tab.
    #[must_use]
    pub fn icon_of(&self, id: NodeId) -> Option<char> {
        match self.nodes[self.slot(id)?].kind {
            Kind::Button { icon, .. } | Kind::ListItem { icon, .. } | Kind::Tab { icon, .. } => {
                icon
            }
            _ => None,
        }
    }

    /// Sets or removes the icon of a button, list item or tab; the layout is redone.
    pub fn set_icon(&mut self, id: NodeId, to: Option<char>) {
        let Some(i) = self.slot(id) else { return };
        if let Kind::Button { icon, .. } | Kind::ListItem { icon, .. } | Kind::Tab { icon, .. } =
            &mut self.nodes[i].kind
            && *icon != to
        {
            *icon = to;
            self.request(i, Flags::LAYOUT | Flags::PAINT);
        }
    }

    /// Measures an icon and the text after it, as a widget draws them.
    pub(crate) fn measure_icon_text<M: fusuma_render::Measure>(
        r: &M,
        theme: &Theme,
        icon: Option<char>,
        text: &str,
    ) -> IconText {
        let body = TextStyle::new(theme.text.body, Color::BLACK).font(theme.fonts.body);
        let icon = icon.map(|c| {
            let mut buf = [0u8; 4];
            let style = TextStyle::new(theme.text.icon, Color::BLACK).font(theme.fonts.icons);
            (c, r.measure_text(c.encode_utf8(&mut buf), &style))
        });
        IconText {
            icon,
            text: r.measure_text(text, &body),
            gap: theme.spacing.sm,
        }
    }

    /// Draws an icon and the text after it on one line, starting at `x` and centred up and down in `area`, in
    /// `color`, cut to `area`.
    pub(crate) fn paint_icon_text<R: Renderer>(
        r: &mut R,
        theme: &Theme,
        area: Rect,
        x: i32,
        m: IconText,
        text: &str,
        color: Color,
    ) -> Result<(), R::Error> {
        Self::in_clip(r, area, |r| {
            let mut at = x;
            if let Some((c, size)) = m.icon {
                let mut buf = [0u8; 4];
                let style = TextStyle::new(theme.text.icon, color).font(theme.fonts.icons);
                let y = centred(area, size).y;
                r.draw_text(c.encode_utf8(&mut buf), Point::new(at, y), &style)?;
                at = at.saturating_add(i32::try_from(size.w.saturating_add(m.gap)).unwrap_or(0));
            }
            if m.text.w > 0 {
                let style = TextStyle::new(theme.text.body, color).font(theme.fonts.body);
                r.draw_text(text, Point::new(at, centred(area, m.text).y), &style)?;
            }
            Ok(())
        })
    }

    /// The look of a tab: its text colour, and the fill behind it if any.
    pub(crate) fn paint_tab<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let n = &self.nodes[i];
        let Kind::Tab {
            text,
            selected,
            icon,
        } = n.kind
        else {
            return Ok(());
        };
        let rect = n.rect;
        let look = theme.tab_in(n.class, self.widget_state(i), selected);
        let (fill, color) = (look.fill, look.text);
        if let Some(fill) = fill {
            r.fill_rect(rect, look.radius, fill)?;
        }
        let m = Self::measure_icon_text(r, theme, icon, text.as_str());
        let size = m.size();
        let x = centred(rect, size).x;
        Self::paint_icon_text(r, theme, rect, x, m, text.as_str(), color)?;
        if let Some(color) = look.indicator {
            // The indicator of the chosen tab: a bar along the bottom edge.
            let bar = theme.border_width.max(3);
            let h = bar.min(rect.h);
            let bottom = i64::from(rect.y) + i64::from(rect.h) - i64::from(h);
            let line = Rect::new(rect.x, bottom as i32, rect.w, h);
            r.fill_rect(line, h / 2, color)?;
        }
        self.ring(i, r, theme, look.radius)
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Color, Point, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{ActionKind, CrossAlign, Length, NodeId, Style, Tree};

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 240;
    const H: u32 = 120;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    fn lay(t: &mut Tree<16>) {
        t.update(&screen(), &THEME, Rect::new(0, 0, W, H));
    }

    fn centre(t: &Tree<16>, id: NodeId) -> Point {
        let r = t.rect(id).unwrap();
        Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
    }

    fn tap(t: &mut Tree<16>, id: NodeId) {
        let p = centre(t, id);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            t.pointer(e);
        }
    }

    fn paint(t: &mut Tree<16>) -> VelloCpu {
        let mut s = screen();
        lay(t);
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        s
    }

    /// How many pixels of `rect` differ from `bg`.
    fn ink(s: &VelloCpu, rect: Rect, bg: Color) -> usize {
        (rect.y..rect.bottom())
            .flat_map(|y| (rect.x..rect.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| s.pixel(x, y) != Some(bg))
            .count()
    }

    #[test]
    fn an_icon_widens_a_button_by_the_icon_and_a_gap_and_an_icon_alone_is_a_square_button() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new().gap(4)).unwrap();
        let plain = t
            .button(Some(root), "Save", ButtonVariant::Primary)
            .unwrap();
        let with = t
            .button_with_icon(Some(root), 'S', "Save", ButtonVariant::Primary)
            .unwrap();
        let only = t
            .button_with_icon(Some(root), 'S', "", ButtonVariant::Primary)
            .unwrap();
        lay(&mut t);
        let (a, b, c) = (
            t.rect(plain).unwrap(),
            t.rect(with).unwrap(),
            t.rect(only).unwrap(),
        );
        let icon = screen()
            .measure_text(
                "S",
                &fusuma_render::TextStyle::new(THEME.text.icon, Color::BLACK)
                    .font(THEME.fonts.icons),
            )
            .w;
        assert!(icon > 0);
        assert_eq!(b.w, a.w + icon + THEME.spacing.sm, "icon, gap, text");
        assert!(b.h >= a.h);
        assert_eq!(c.w, icon + THEME.spacing.md * 2, "icon and padding, no gap");
        assert_eq!(t.icon_of(with), Some('S'));
        assert_eq!(t.icon_of(plain), None);
        t.set_icon(with, None);
        lay(&mut t);
        assert_eq!(
            t.rect(with).unwrap().w,
            a.w,
            "removing the icon shrinks it back"
        );
    }

    #[test]
    fn the_icon_is_drawn_before_the_text_and_not_when_there_is_none() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new().padding(6)).unwrap();
        let li = t
            .list_item_with_icon(Some(root), 'I', "Row", false)
            .unwrap();
        let bare = t.list_item(Some(root), "Row", false).unwrap();
        let s = paint(&mut t);
        let (a, b) = (t.rect(li).unwrap(), t.rect(bare).unwrap());
        let bg = THEME.colors.background;
        let left = |r: Rect| Rect::new(r.x, r.y, THEME.spacing.md + THEME.text.icon, r.h);
        assert!(
            ink(&s, left(a), bg) > 10,
            "the icon sits where the text would start"
        );
        // The text of the row with the icon starts further right than the one without.
        let first_ink = |r: Rect| {
            (r.x..r.right()).find(|&x| (r.y..r.bottom()).any(|y| s.pixel(x, y) != Some(bg)))
        };
        let (ta, tb) = (first_ink(a).unwrap(), first_ink(b).unwrap());
        assert!(
            ta <= tb,
            "the icon starts at the same left edge as text would: {ta} {tb}"
        );
        let text_from = |r: Rect, skip: u32| {
            (r.x + skip as i32..r.right())
                .find(|&x| (r.y..r.bottom()).any(|y| s.pixel(x, y) != Some(bg)))
        };
        assert!(
            text_from(a, THEME.spacing.md + THEME.text.icon + THEME.spacing.sm - 1).unwrap()
                >= tb + (THEME.text.icon + THEME.spacing.sm) as i32 - 1,
            "the text is pushed right by the icon and the gap"
        );
    }

    #[test]
    fn a_tab_bar_gives_the_tabs_equal_shares_and_choosing_one_clears_the_rest() {
        let mut t = Tree::<16>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let bar = t.tab_bar(Some(root), Style::new()).unwrap();
        let tabs = [
            t.tab(Some(bar), Some('H'), "Home", true).unwrap(),
            t.tab(Some(bar), None, "Search", false).unwrap(),
            t.tab(Some(bar), Some('P'), "", false).unwrap(),
        ];
        lay(&mut t);
        let w = tabs.map(|id| t.rect(id).unwrap().w);
        assert!(
            w[0].abs_diff(w[1]) <= 1 && w[1].abs_diff(w[2]) <= 1,
            "{w:?}"
        );
        assert!(
            w.iter().sum::<u32>().abs_diff(W) <= 2,
            "fills the bar: {w:?}"
        );
        assert_eq!(tabs.map(|id| t.is_checked(id)), [true, false, false]);
        tap(&mut t, tabs[2]);
        assert_eq!(tabs.map(|id| t.is_checked(id)), [false, false, true]);
        let a = t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (tabs[2], ActionKind::Selected));
        tap(&mut t, tabs[2]);
        assert_eq!(t.pop_action(), None, "the chosen tab again is no change");
        // From code: no action.
        t.set_checked(tabs[1], true);
        assert!(t.is_checked(tabs[1]) && !t.is_checked(tabs[2]));
        assert_eq!(t.pop_action(), None);
        assert_eq!(t.text(tabs[1]), Some("Search"));
    }

    #[test]
    fn the_chosen_tab_shows_a_bar_along_its_bottom_and_a_disabled_tab_ignores_taps() {
        let mut t = Tree::<16>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let bar = t.tab_bar(Some(root), Style::new()).unwrap();
        let a = t.tab(Some(bar), None, "One", true).unwrap();
        let b = t.tab(Some(bar), None, "Two", false).unwrap();
        let s = paint(&mut t);
        let (ra, rb) = (t.rect(a).unwrap(), t.rect(b).unwrap());
        let probe = |r: Rect| s.pixel(r.x + (r.w / 2) as i32, r.bottom() - 1);
        assert_eq!(
            probe(ra),
            Some(THEME.colors.primary),
            "the bar of the chosen tab"
        );
        assert_eq!(
            probe(rb),
            Some(THEME.colors.background),
            "none under the other"
        );
        t.set_enabled(b, false);
        lay(&mut t);
        tap(&mut t, b);
        assert!(!t.is_checked(b) && t.pop_action().is_none());
        let _ = Length::Content;
    }

    #[test]
    fn a_stale_handle_and_the_wrong_kind_are_ignored() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new()).unwrap();
        let l = t
            .label(Some(root), "x", crate::theme::TextRole::Body)
            .unwrap();
        let b = t.button(Some(root), "b", ButtonVariant::Primary).unwrap();
        t.set_icon(l, Some('x'));
        assert_eq!(t.icon_of(l), None, "a label has no icon slot");
        t.remove(b);
        t.set_icon(b, Some('x'));
        assert_eq!(t.icon_of(b), None);
    }
}
