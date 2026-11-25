//! Virtual lists: hundreds or thousands of rows in a tree that has room for a few. The list is a scroll container
//! with a spacer above and below a small pool of rows; the spacers are as tall as the rows that are not there, so the
//! scrollbar and the offsets are those of the whole list, and as the list scrolls the pool is moved along it. The
//! app asks [`Tree::virtual_window`] before each frame and puts the text of the rows it names into the pool's rows.

use crate::tree::{BuildError, CrossAlign, Flags, Kind, Length, NodeId, Style, Tree};

/// Most virtual lists one tree keeps track of.
pub const MAX_LISTS: usize = 2;

/// What a virtual list is and where its window is.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Virt {
    list: NodeId,
    count: u32,
    row_h: u32,
    pool: u32,
    /// The index of the first row in the pool.
    first: u32,
}

/// The rows that are in the pool now: row `first + k` is the pool's row number `k`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    /// The index of the first row of the pool in the list.
    pub first: usize,
    /// How many rows of the pool have something to show (fewer than the pool at the end of a short list).
    pub rows: usize,
}

impl Virt {
    fn rows(&self) -> u32 {
        self.pool.min(self.count)
    }

    /// The first index of the pool for a list scrolled `offset` pixels.
    fn first_for(&self, offset: i32) -> u32 {
        let at = u32::try_from(offset.max(0)).unwrap_or(0) / self.row_h.max(1);
        at.min(self.count.saturating_sub(self.rows()))
    }
}

impl<const N: usize> Tree<N> {
    /// Adds a virtual list of `count` rows, each `row_h` pixels high, kept in a pool of `pool` row nodes (enough for
    /// the view and one more: `view_height / row_h + 2`). `style` gives the size, which should be fixed or filling
    /// (a list that grows to its content would grow to all of it); its gap is not used. Bind the rows with
    /// [`virtual_window`](Self::virtual_window) before every frame.
    ///
    /// # Errors
    /// [`BuildError`] when the nodes cannot be added there, or the tree already holds the most virtual lists.
    pub fn virtual_list(
        &mut self,
        parent: Option<NodeId>,
        style: Style,
        count: usize,
        row_h: u32,
        pool: usize,
    ) -> Result<NodeId, BuildError> {
        let slot = self
            .virts
            .iter()
            .position(Option::is_none)
            .ok_or(BuildError::Full)?;
        let list = self.scroll(parent, style.gap(0).align(CrossAlign::Stretch))?;
        let virt = Virt {
            list,
            count: u32::try_from(count).unwrap_or(u32::MAX),
            row_h: row_h.max(1),
            pool: u32::try_from(pool.max(1)).unwrap_or(u32::MAX),
            first: 0,
        };
        self.spacer(Some(list), Style::new().height(Length::Fixed(0)))?;
        for _ in 0..virt.rows() {
            self.list_item(Some(list), "", false)?;
        }
        self.spacer(Some(list), Style::new().height(Length::Fixed(0)))?;
        self.virts[slot] = Some(virt);
        self.apply_virtual(slot);
        Ok(list)
    }

    fn virt_slot(&self, list: NodeId) -> Option<usize> {
        self.virts
            .iter()
            .position(|v| v.is_some_and(|v| v.list == list))
            .filter(|_| self.slot(list).is_some())
    }

    /// The window of virtual list `list` for where it is scrolled now: moves the pool along the list (the spacers
    /// change size) and says which rows are in it. Call it before each frame and put the text of row
    /// `first + k` into the pool's row `k` ([`virtual_rows`](Self::virtual_rows)).
    pub fn virtual_window(&mut self, list: NodeId) -> Option<Window> {
        let s = self.virt_slot(list)?;
        let i = self.slot(list)?;
        let offset = match self.nodes[i].kind {
            Kind::Scroll { offset, .. } => offset,
            _ => 0,
        };
        let v = self.virts[s]?;
        let first = v.first_for(offset);
        if first != v.first {
            if let Some(v) = &mut self.virts[s] {
                v.first = first;
            }
            self.apply_virtual(s);
        }
        let v = self.virts[s]?;
        Some(Window {
            first: v.first as usize,
            rows: v.rows() as usize,
        })
    }

    /// The pool's row nodes of virtual list `list`, in order: row `k` shows the list's row `first + k`.
    pub fn virtual_rows(&self, list: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        let rows = self
            .virt_slot(list)
            .and_then(|s| self.virts[s])
            .map_or(0, |v| v.rows() as usize);
        self.children(list).skip(1).take(rows)
    }

    /// The index in the list of pool row `row`, if it is one of the rows of virtual list `list`.
    #[must_use]
    pub fn virtual_index(&self, list: NodeId, row: NodeId) -> Option<usize> {
        let v = self.virts[self.virt_slot(list)?]?;
        let at = self
            .children(list)
            .skip(1)
            .take(v.rows() as usize)
            .position(|c| c == row)?;
        Some(v.first as usize + at)
    }

    /// Changes how many rows virtual list `list` has.
    pub fn set_virtual_count(&mut self, list: NodeId, count: usize) {
        let Some(s) = self.virt_slot(list) else {
            return;
        };
        let count = u32::try_from(count).unwrap_or(u32::MAX);
        if let Some(v) = &mut self.virts[s] {
            if v.count == count {
                return;
            }
            v.count = count;
            v.first = v.first.min(count.saturating_sub(v.rows()));
        }
        self.apply_virtual(s);
    }

    /// Sizes the spacers and the rows of virtual list `s` for its window.
    fn apply_virtual(&mut self, s: usize) {
        let Some(v) = self.virts[s] else { return };
        let Some(list) = self.slot(v.list) else {
            return;
        };
        let rows = u64::from(v.rows());
        let above = u64::from(v.first) * u64::from(v.row_h);
        let below = (u64::from(v.count) - u64::from(v.first).min(u64::from(v.count)))
            .saturating_sub(rows)
            * u64::from(v.row_h);
        let px = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
        let mut child = self.nodes[list].first.map(usize::from);
        let mut at = 0u64;
        while let Some(c) = child {
            let next = self.nodes[c].next.map(usize::from);
            let h = if at == 0 {
                px(above)
            } else if at <= rows {
                v.row_h
            } else {
                px(below)
            };
            let id = self.id_of(c);
            let style = Style::new()
                .height(Length::Fixed(h))
                .width(self.nodes[c].style.width);
            self.set_style(id, style);
            child = next;
            at += 1;
        }
        self.request(list, Flags::LAYOUT | Flags::PAINT);
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Color, Point, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::event::Pointer;
    use crate::theme::Theme;
    use crate::tree::ActionKind;

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 200;
    const H: u32 = 200;
    const BOUNDS: Rect = Rect::new(0, 0, W, H);
    const COUNT: usize = 300;
    const VIEW: u32 = 120;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    fn row_h() -> u32 {
        THEME.control_height
    }

    fn name(i: usize) -> String {
        format!("Row {i}")
    }

    fn style() -> Style {
        Style::new()
            .height(Length::Fixed(VIEW))
            .width(Length::Fixed(160))
    }

    fn pool() -> usize {
        (VIEW / row_h()) as usize + 2
    }

    /// The list, with its rows bound to the names.
    struct Ui {
        t: Tree<24>,
        list: NodeId,
        selected: Option<usize>,
    }

    impl Ui {
        fn new() -> Self {
            let mut t = Tree::<24>::new();
            let root = t.column(None, Style::new().padding(10)).unwrap();
            let list = t
                .virtual_list(Some(root), style(), COUNT, row_h(), pool())
                .unwrap();
            let mut u = Self {
                t,
                list,
                selected: None,
            };
            u.bind();
            u.t.update(&screen(), &THEME, BOUNDS);
            u
        }

        /// What an app does before each frame.
        fn bind(&mut self) {
            let w = self.t.virtual_window(self.list).unwrap();
            let rows: Vec<NodeId> = self.t.virtual_rows(self.list).collect();
            assert_eq!(rows.len(), w.rows);
            for (k, row) in rows.into_iter().enumerate() {
                self.t.set_text(row, &name(w.first + k));
                self.t.set_checked(row, self.selected == Some(w.first + k));
            }
        }

        fn frame(&mut self) {
            self.bind();
            self.t.update(&screen(), &THEME, BOUNDS);
        }
    }

    fn pixels_of(s: &VelloCpu, r: Rect) -> Vec<Color> {
        (r.y..r.bottom())
            .flat_map(|y| (r.x..r.right()).map(move |x| (x, y)))
            .map(|(x, y)| s.pixel(x, y).unwrap())
            .collect()
    }

    /// The same list as real rows, scrolled to the same place.
    fn real_picture(offset: i32, selected: Option<usize>) -> (Vec<Color>, Rect) {
        let mut t = Tree::<{ COUNT + 8 }>::new();
        let root = t.column(None, Style::new().padding(10)).unwrap();
        let list = t
            .scroll(Some(root), style().gap(0).align(CrossAlign::Stretch))
            .unwrap();
        for i in 0..COUNT {
            t.list_item(Some(list), &name(i), selected == Some(i))
                .unwrap();
        }
        let mut s = screen();
        t.update(&s, &THEME, BOUNDS);
        t.scroll_to(list, offset);
        t.update(&s, &THEME, BOUNDS);
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        let view = t.rect(list).unwrap();
        (pixels_of(&s, view), view)
    }

    fn virtual_picture(u: &mut Ui) -> (Vec<Color>, Rect) {
        u.frame();
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        let view = u.t.rect(u.list).unwrap();
        (pixels_of(&s, view), view)
    }

    #[test]
    fn a_few_nodes_stand_for_the_whole_list() {
        let u = Ui::new();
        assert!(u.t.len() < 12, "{} nodes for {COUNT} rows", u.t.len());
        assert_eq!(
            u.t.scroll_max(u.list),
            Some((COUNT as u32 * row_h() - VIEW) as i32)
        );
        let rows: Vec<_> = u.t.virtual_rows(u.list).collect();
        assert_eq!(rows.len(), pool());
        assert_eq!(u.t.text(rows[0]), Some("Row 0"));
        assert_eq!(u.t.virtual_index(u.list, rows[3]), Some(3));
    }

    #[test]
    fn at_any_offset_the_picture_is_the_picture_of_the_real_list() {
        let mut u = Ui::new();
        let max = u.t.scroll_max(u.list).unwrap();
        let mut seed = 1u64;
        let mut offsets = vec![
            0,
            1,
            row_h() as i32 - 1,
            row_h() as i32,
            max,
            max - 1,
            5 * row_h() as i32 + 7,
        ];
        for _ in 0..25 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            offsets.push(((seed >> 33) as i32).rem_euclid(max + 1));
        }
        for off in offsets {
            u.t.scroll_to(u.list, off);
            let (got, view) = virtual_picture(&mut u);
            assert_eq!(u.t.scroll_offset(u.list), Some(off));
            let (want, rview) = real_picture(off, None);
            assert_eq!(view, rview);
            assert!(got == want, "different at offset {off}");
        }
    }

    #[test]
    fn the_window_follows_the_scroll_and_never_leaves_the_list() {
        let mut u = Ui::new();
        let max = u.t.scroll_max(u.list).unwrap();
        for off in [0, 100, 5000, max] {
            u.t.scroll_to(u.list, off);
            let w = u.t.virtual_window(u.list).unwrap();
            assert_eq!(
                w.first as i32,
                (off / row_h() as i32).min((COUNT - pool()) as i32),
                "offset {off}"
            );
            assert!(w.first + w.rows <= COUNT);
        }
        // A fling and a drag keep every frame right: each visible row shows its own text where it belongs.
        let at = u.t.rect(u.list).unwrap();
        let x = at.x + 20;
        u.t.pointer(Pointer::Down(Point::new(x, at.y + 100)));
        for k in 1..=6 {
            u.t.pointer(Pointer::Move(Point::new(x, at.y + 100 - k * 25)));
            u.t.advance(&THEME, 16);
            u.frame();
        }
        u.t.pointer(Pointer::Up(Point::new(x, at.y + 100 - 150)));
        for _ in 0..120 {
            let moving = u.t.advance(&THEME, 16);
            u.frame();
            let off = u.t.scroll_offset(u.list).unwrap() + u.t.scroll_over(u.list).unwrap();
            let view = u.t.rect(u.list).unwrap();
            for row in u.t.virtual_rows(u.list) {
                let idx = u.t.virtual_index(u.list, row).unwrap();
                let r = u.t.rect(row).unwrap();
                assert_eq!(
                    r.y,
                    view.y + (idx as u32 * row_h()) as i32 - off,
                    "row {idx} at offset {off}"
                );
                assert_eq!(u.t.text(row), Some(name(idx).as_str()));
            }
            if !moving && !u.t.needs_frame() {
                break;
            }
        }
        assert!(
            u.t.scroll_offset(u.list).unwrap() > 1000,
            "the fling went a long way"
        );
    }

    #[test]
    fn a_tap_names_its_index_and_the_choice_follows_the_row_through_scrolling() {
        let mut u = Ui::new();
        u.t.scroll_to(u.list, 40 * row_h() as i32 + 9);
        u.frame();
        let row = u.t.virtual_rows(u.list).nth(2).unwrap();
        let r = u.t.rect(row).unwrap();
        let p = Point::new(r.x + 10, r.y + 5);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            u.t.pointer(e);
        }
        let a = u.t.pop_action().unwrap();
        assert_eq!(a.kind, ActionKind::Selected);
        let idx = u.t.virtual_index(u.list, a.source).unwrap();
        assert_eq!(idx, 42);
        u.selected = Some(idx);
        // Away and back: the chosen row is chosen, and no other.
        u.t.scroll_to(u.list, 0);
        u.frame();
        assert!(u.t.virtual_rows(u.list).all(|r| !u.t.is_checked(r)));
        u.t.scroll_to(u.list, 40 * row_h() as i32 + 9);
        u.frame();
        let chosen: Vec<_> =
            u.t.virtual_rows(u.list)
                .filter(|&r| u.t.is_checked(r))
                .collect();
        assert_eq!(chosen.len(), 1);
        assert_eq!(u.t.virtual_index(u.list, chosen[0]), Some(42));
        let (got, _) = virtual_picture(&mut u);
        let (want, _) = real_picture(40 * row_h() as i32 + 9, Some(42));
        assert!(got == want, "the choice is drawn as in the real list");
    }

    #[test]
    fn the_count_can_change_a_short_list_has_fewer_rows_and_odd_calls_are_harmless() {
        let mut u = Ui::new();
        u.t.scroll_to(u.list, 100 * row_h() as i32);
        u.frame();
        u.t.set_virtual_count(u.list, 2);
        u.frame();
        let w = u.t.virtual_window(u.list).unwrap();
        assert_eq!((w.first, w.rows), (0, 2));
        assert_eq!(
            u.t.scroll_max(u.list),
            Some(0),
            "two rows fit: nothing to scroll"
        );
        let shown = u.t.virtual_rows(u.list).count();
        assert_eq!(shown, 2);
        u.t.set_virtual_count(u.list, 0);
        u.frame();
        assert_eq!(u.t.virtual_rows(u.list).count(), 0);
        u.t.set_virtual_count(u.list, 1000);
        u.frame();
        assert_eq!(u.t.scroll_max(u.list), Some((1000 * row_h() - VIEW) as i32));
        // Not a virtual list, a stale handle.
        let root = u.t.root().unwrap();
        assert_eq!(u.t.virtual_window(root), None);
        assert_eq!(u.t.virtual_rows(root).count(), 0);
        assert_eq!(u.t.virtual_index(root, root), None);
        u.t.set_virtual_count(root, 5);
        let gone = u.list;
        u.t.remove(gone);
        assert_eq!(u.t.virtual_window(gone), None);
        // Only so many virtual lists in one tree.
        let mut t = Tree::<40>::new();
        let r = t.column(None, Style::new()).unwrap();
        for _ in 0..MAX_LISTS {
            t.virtual_list(Some(r), style(), 10, 20, 3).unwrap();
        }
        assert_eq!(
            t.virtual_list(Some(r), style(), 10, 20, 3),
            Err(BuildError::Full)
        );
    }
}
