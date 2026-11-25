//! The rewrite passes: disabled state flowing down, then layout, then hover against the new layout.

use fusuma_render::{Rect, Renderer, Size};

use crate::theme::Theme;
use crate::tree::{Axis, CrossAlign, Flags, Kind, Length, Style, Tree};

fn main_of(axis: Axis, s: Size) -> u32 {
    match axis {
        Axis::Column => s.h,
        Axis::Row => s.w,
    }
}

fn cross_of(axis: Axis, s: Size) -> u32 {
    match axis {
        Axis::Column => s.w,
        Axis::Row => s.h,
    }
}

fn main_len(axis: Axis, st: &Style) -> Length {
    match axis {
        Axis::Column => st.height,
        Axis::Row => st.width,
    }
}

fn cross_len(axis: Axis, st: &Style) -> Length {
    match axis {
        Axis::Column => st.width,
        Axis::Row => st.height,
    }
}

fn size_of(axis: Axis, main: u32, cross: u32) -> Size {
    match axis {
        Axis::Column => Size::new(cross, main),
        Axis::Row => Size::new(main, cross),
    }
}

fn weight(w: u16) -> u32 {
    u32::from(w.max(1))
}

fn offset(base: i32, by: i64) -> i32 {
    (i64::from(base) + by).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl<const N: usize> Tree<N> {
    /// Runs the rewrite passes after events and mutations: disabled state is passed down, the
    /// layout is redone if something asked for it (or `bounds` changed), and the hovered widget is
    /// refreshed against the new layout. Does nothing when nothing changed.
    ///
    /// The whole tree is laid out when anything needs it; the flags already say where, so laying out
    /// only the changed subtrees can follow without changing this interface.
    pub fn update<R: Renderer>(&mut self, renderer: &R, theme: &Theme, bounds: Rect) {
        self.update_disabled();
        self.layout(renderer, theme, bounds);
    }

    /// Passes "disabled" down: a node is disabled if it or any ancestor is.
    fn update_disabled(&mut self) {
        if !self.disabled_dirty {
            return;
        }
        self.disabled_dirty = false;
        let Some(root) = self.root.map(usize::from) else {
            return;
        };
        let mut cur = Some(root);
        while let Some(i) = cur {
            let inherited = self.nodes[i]
                .parent
                .is_some_and(|p| self.nodes[usize::from(p)].disabled_eff);
            let now = self.nodes[i].disabled || inherited;
            if now != self.nodes[i].disabled_eff {
                self.nodes[i].disabled_eff = now;
                self.request(i, Flags::PAINT | Flags::ANIM);
                if now {
                    // A widget that can no longer be used lets go of the pointer.
                    self.nodes[i].active = false;
                    self.nodes[i].hovered = false;
                    let id = self.id_of(i);
                    if self.captured == Some(id) {
                        self.captured = None;
                    }
                    if self.hovered == Some(id) {
                        self.hovered = None;
                    }
                    // Nor can it keep the keyboard.
                    if self.focus == Some(id) {
                        self.focus = None;
                        self.focus_visible = false;
                    }
                }
            }
            cur = self.preorder_next(i, root);
        }
    }

    fn layout<R: Renderer>(&mut self, renderer: &R, theme: &Theme, bounds: Rect) {
        let Some(root) = self.root.map(usize::from) else {
            return;
        };
        if !self.nodes[root].sub.has(Flags::LAYOUT) && self.bounds == Some(bounds) {
            return;
        }
        self.bounds = Some(bounds);
        // Measure: every node after its children.
        let mut cur = Some(self.leftmost_leaf(root));
        while let Some(i) = cur {
            self.nodes[i].size = self.measure(i, renderer, theme);
            cur = if i == root { None } else { self.post_next(i) };
        }
        // Place: every node before its children.
        self.place_all(root, bounds, theme.spacing.lg);
        // A widget the keyboard moved to may be scrolled out of sight: scroll to it with the fresh
        // positions, then place again with the new offsets.
        if let Some(target) = self.pending_reveal.take().and_then(|id| self.slot(id)) {
            self.reveal(target);
            if self.nodes[root].sub.has(Flags::LAYOUT) {
                self.place_all(root, bounds, theme.spacing.lg);
            }
        }
        // The cursor of a text field a press landed on, and where the caret is.
        self.resolve_input(renderer, theme);
        self.clear_flag(Flags::LAYOUT);
        // From here the rows are where they will be drawn, so a change in the shifting window can be drawn over it.
        self.shift_laid = self.shift.is_some();
        // Things may have moved under the pointer.
        if let Some(p) = self.pointer {
            let h = self.hit(p);
            self.set_hover(h);
        }
    }

    /// Lays the tree out again at the same bounds, for a test that moves things between frames.
    #[cfg(test)]
    pub(crate) fn relayout<R: Renderer>(&mut self, renderer: &R, theme: &Theme) {
        if let Some(b) = self.bounds {
            self.update(renderer, theme, b);
        }
    }

    /// Puts every node where its parent says, from the root down.
    fn place_all(&mut self, root: usize, bounds: Rect, margin: u32) {
        self.set_rect(root, bounds);
        let mut cur = Some(root);
        while let Some(i) = cur {
            if let Kind::Dialog { open, popover, .. } = self.nodes[i].kind {
                // A dialog sits in the middle of the screen at its own size, kept `margin` from the edges; a closed
                // one takes no room at all. A menu hangs from its widget instead.
                let rect = if !open {
                    Rect::new(bounds.x, bounds.y, 0, 0)
                } else if let Some(p) = popover {
                    self.popover_rect(i, p, bounds, margin.min(4))
                } else {
                    let s = self.nodes[i].size;
                    let w = s.w.min(bounds.w.saturating_sub(margin.saturating_mul(2)));
                    let h = s.h.min(bounds.h.saturating_sub(margin.saturating_mul(2)));
                    let at = crate::paint::centred(bounds, fusuma_render::Size::new(w, h));
                    Rect::new(at.x, at.y, w, h)
                };
                self.set_rect(i, rect);
            }
            self.place_children(i);
            cur = self.preorder_next(i, root);
        }
    }

    /// The size node `i` wants, from its content and its style.
    fn measure<R: Renderer>(&self, i: usize, renderer: &R, theme: &Theme) -> Size {
        let n = &self.nodes[i];
        let pad = n.style.padding.saturating_mul(2);
        let content = match n.kind {
            Kind::Label { text, role } => {
                renderer.measure_text(text.as_str(), &theme.text_style(role))
            }
            Kind::Button { text, icon, .. } => {
                let t = Self::measure_icon_text(renderer, theme, icon, text.as_str()).size();
                Size::new(
                    t.w.saturating_add(theme.spacing.md.saturating_mul(2)),
                    t.h.saturating_add(theme.spacing.sm.saturating_mul(2))
                        .max(theme.control_height),
                )
            }
            Kind::Spacer => Size::new(0, 0),
            Kind::Checkbox { .. }
            | Kind::Radio { .. }
            | Kind::ListItem { .. }
            | Kind::Tab { .. }
            | Kind::Progress { .. }
            | Kind::Switch { .. }
            | Kind::Slider { .. }
            | Kind::TextInput { .. } => self.measure_widget(i, renderer, theme),
            Kind::Stack(_) | Kind::Scroll { .. } | Kind::Dialog { .. } => {
                let axis = match n.kind {
                    Kind::Stack(axis) => axis,
                    _ => Axis::Column,
                };
                let (mut main, mut cross, mut count) = (0u32, 0u32, 0u32);
                let mut c = n.first;
                while let Some(ci) = c {
                    let child = &self.nodes[usize::from(ci)];
                    // A dialog under this container is out of its flow: it takes no room here.
                    if matches!(child.kind, Kind::Dialog { .. }) {
                        c = child.next;
                        continue;
                    }
                    main = main.saturating_add(main_of(axis, child.size));
                    cross = cross.max(cross_of(axis, child.size));
                    count += 1;
                    c = child.next;
                }
                main = main.saturating_add(n.style.gap.saturating_mul(count.saturating_sub(1)));
                size_of(axis, main, cross)
            }
        };
        let w = match n.style.width {
            Length::Fixed(w) => w,
            _ => content.w.saturating_add(pad),
        };
        let h = match n.style.height {
            Length::Fixed(h) => h,
            _ => content.h.saturating_add(pad),
        };
        Size::new(w, h)
    }

    /// Puts the children of a container inside it.
    fn place_children(&mut self, i: usize) {
        let (axis, scrolls) = match self.nodes[i].kind {
            Kind::Stack(axis) => (axis, false),
            Kind::Scroll { .. } => (Axis::Column, true),
            Kind::Dialog { .. } => (Axis::Column, false),
            _ => return,
        };
        let st = self.nodes[i].style;
        let inner = self.nodes[i].rect.inset(st.padding);
        // What a child's length means here: in a scroll container the height is the natural one, because
        // there is no fixed room to share out.
        let main_of_child = |style: &Style| match main_len(axis, style) {
            Length::Fill(_) if scrolls => Length::Content,
            other => other,
        };
        let (main_total, cross_total) = match axis {
            Axis::Column => (inner.h, inner.w),
            Axis::Row => (inner.w, inner.h),
        };
        // How much the fixed and content-sized children take, and how many shares there are.
        let (mut count, mut taken, mut weights) = (0u32, 0u32, 0u32);
        let mut c = self.nodes[i].first;
        while let Some(ci) = c {
            let child = &self.nodes[usize::from(ci)];
            if matches!(child.kind, Kind::Dialog { .. }) {
                c = child.next;
                continue;
            }
            count += 1;
            match main_of_child(&child.style) {
                Length::Fill(w) => weights = weights.saturating_add(weight(w)),
                _ => taken = taken.saturating_add(main_of(axis, child.size)),
            }
            c = child.next;
        }
        let gaps = st.gap.saturating_mul(count.saturating_sub(1));
        // A scroll container: how tall the content is, and where the window onto it starts.
        let mut start: i64 = 0;
        if scrolls {
            let content = taken.saturating_add(gaps);
            let max = content.saturating_sub(inner.h);
            if let Kind::Scroll {
                offset,
                content: c,
                over,
                ..
            } = &mut self.nodes[i].kind
            {
                *offset = (*offset).clamp(0, i32::try_from(max).unwrap_or(i32::MAX));
                *c = content;
                let reach = i32::try_from(inner.h).unwrap_or(i32::MAX);
                *over = (*over).clamp(-reach, reach);
                start = -(i64::from(*offset) + i64::from(*over));
            }
        }
        let mut room = main_total.saturating_sub(gaps).saturating_sub(taken);
        let mut weights_left = weights;
        let mut pos: i64 = start;
        let mut c = self.nodes[i].first;
        while let Some(ci) = c {
            let ci = usize::from(ci);
            let (child_style, child_size, next) = {
                let child = &self.nodes[ci];
                (child.style, child.size, child.next)
            };
            if matches!(self.nodes[ci].kind, Kind::Dialog { .. }) {
                c = next;
                continue;
            }
            let main_px = match main_of_child(&child_style) {
                Length::Fill(w) => {
                    let w = weight(w);
                    let share = if weights_left == 0 {
                        0
                    } else {
                        (u64::from(room) * u64::from(w) / u64::from(weights_left)) as u32
                    };
                    room = room.saturating_sub(share);
                    weights_left = weights_left.saturating_sub(w);
                    share
                }
                _ => main_of(axis, child_size),
            };
            // A label's text is drawn from its left edge, so stretching it across a column changes nothing that is
            // seen: it keeps its text width, which is also all that needs redrawing when the text changes.
            let text_in_column =
                axis == Axis::Column && matches!(self.nodes[ci].kind, Kind::Label { .. });
            let cross_px = match (cross_len(axis, &child_style), st.align) {
                (Length::Fill(_), _) => cross_total,
                (Length::Content, CrossAlign::Stretch) if !text_in_column => cross_total,
                (Length::Fixed(f), _) => f,
                (Length::Content, _) => cross_of(axis, child_size).min(cross_total),
            };
            let cross_off = match st.align {
                CrossAlign::Start | CrossAlign::Stretch => 0,
                CrossAlign::Center => (i64::from(cross_total) - i64::from(cross_px)) / 2,
                CrossAlign::End => i64::from(cross_total) - i64::from(cross_px),
            };
            let rect = match axis {
                Axis::Column => Rect::new(
                    offset(inner.x, cross_off),
                    offset(inner.y, pos),
                    cross_px,
                    main_px,
                ),
                Axis::Row => Rect::new(
                    offset(inner.x, pos),
                    offset(inner.y, cross_off),
                    main_px,
                    cross_px,
                ),
            };
            self.set_rect(ci, rect);
            pos += i64::from(main_px) + i64::from(st.gap);
            c = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ButtonVariant, TextRole};
    use fusuma_render_vello_cpu::VelloCpu;

    type Screen = VelloCpu;

    fn screen() -> Screen {
        fusuma_render_vello_cpu::new(8, 8).unwrap()
    }

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn rect<const N: usize>(t: &Tree<N>, id: crate::tree::NodeId) -> Rect {
        t.rect(id).unwrap()
    }

    fn fixed(w: u32, h: u32) -> Style {
        Style::new()
            .width(Length::Fixed(w))
            .height(Length::Fixed(h))
    }

    #[test]
    fn a_column_stacks_fixed_children_with_gap_and_padding() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new().padding(5).gap(3)).unwrap();
        let a = t.spacer(Some(root), fixed(20, 10)).unwrap();
        let b = t.spacer(Some(root), fixed(30, 12)).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        assert_eq!(rect(&t, root), Rect::new(0, 0, 100, 100));
        assert_eq!(rect(&t, a), Rect::new(5, 5, 20, 10));
        assert_eq!(rect(&t, b), Rect::new(5, 18, 30, 12));
    }

    #[test]
    fn a_row_shares_the_room_in_proportion_to_the_weights() {
        let mut t = Tree::<8>::new();
        let root = t.row(None, Style::new().gap(10)).unwrap();
        let fixed_child = t.spacer(Some(root), fixed(40, 5)).unwrap();
        let one = t
            .spacer(Some(root), Style::new().width(Length::Fill(1)))
            .unwrap();
        let two = t
            .spacer(Some(root), Style::new().width(Length::Fill(2)))
            .unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 250, 20));
        // 250 - 40 fixed - 2 gaps of 10 = 190 to share 1:2 -> 63 and 127 (the last takes the rest).
        assert_eq!(rect(&t, fixed_child).w, 40);
        let (a, b) = (rect(&t, one), rect(&t, two));
        assert_eq!((a.w, b.w), (63, 127));
        assert_eq!(a.x, 50);
        assert_eq!(b.x, a.x + 63 + 10);
        assert_eq!(a.w + b.w, 190, "nothing is lost to rounding");
    }

    #[test]
    fn weights_of_zero_count_as_one_and_no_room_gives_no_share() {
        let mut t = Tree::<8>::new();
        let root = t.row(None, Style::new()).unwrap();
        let a = t
            .spacer(Some(root), Style::new().width(Length::Fill(0)))
            .unwrap();
        let b = t
            .spacer(Some(root), Style::new().width(Length::Fill(1)))
            .unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 101, 10));
        assert_eq!((rect(&t, a).w, rect(&t, b).w), (50, 51));
        let mut t = Tree::<8>::new();
        let root = t.row(None, Style::new()).unwrap();
        t.spacer(Some(root), fixed(200, 5)).unwrap();
        let fill = t
            .spacer(Some(root), Style::new().width(Length::Fill(1)))
            .unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 10));
        assert_eq!(
            rect(&t, fill).w,
            0,
            "an overfull row leaves nothing to share"
        );
    }

    #[test]
    fn cross_alignment_places_and_stretches_children() {
        let build = |align| {
            let mut t = Tree::<8>::new();
            let root = t.column(None, Style::new().align(align)).unwrap();
            let small = t.spacer(Some(root), fixed(20, 10)).unwrap();
            let content = t.label(Some(root), "ab", TextRole::Body).unwrap();
            let fill = t
                .spacer(
                    Some(root),
                    Style::new().width(Length::Fill(1)).height(Length::Fixed(4)),
                )
                .unwrap();
            t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
            (rect(&t, small), rect(&t, content), rect(&t, fill))
        };
        let (s, c, f) = build(CrossAlign::Start);
        assert_eq!((s.x, c.x, f.w), (0, 0, 100));
        let (s, c, _) = build(CrossAlign::Center);
        assert_eq!((s.x, c.x), (40, (100 - c.w as i32) / 2));
        let (s, c, _) = build(CrossAlign::End);
        assert_eq!((s.x, c.x), (80, 100 - c.w as i32));
        let (s, c, _) = build(CrossAlign::Stretch);
        assert_eq!(s.w, 20, "a fixed size stays");
        assert!(
            c.w < 100,
            "a label keeps its text width: stretching it would change nothing that is seen"
        );
    }

    #[test]
    fn content_is_measured_through_the_renderer() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let title = t.label(Some(root), "Hello", TextRole::Title).unwrap();
        let body = t.label(Some(root), "Hello", TextRole::Body).unwrap();
        let ok = t.button(Some(root), "OK", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 300, 300));
        // The built-in fonts: 10x20 for 20 px text and 9x15 for 15 px text.
        assert_eq!((rect(&t, title).w, rect(&t, title).h), (50, 20));
        assert_eq!((rect(&t, body).w, rect(&t, body).h), (45, 15));
        assert_eq!(rect(&t, body).y, 20);
        // A button is the label plus padding, and at least a touch target high.
        assert_eq!(
            (rect(&t, ok).w, rect(&t, ok).h),
            (18 + 24, THEME.control_height)
        );
        assert_eq!(rect(&t, ok).y, 35);
    }

    #[test]
    fn a_container_wants_the_size_of_its_children() {
        let mut t = Tree::<8>::new();
        let root = t.row(None, Style::new().align(CrossAlign::Start)).unwrap();
        let col = t
            .column(Some(root), Style::new().padding(4).gap(2))
            .unwrap();
        t.spacer(Some(col), fixed(10, 6)).unwrap();
        t.spacer(Some(col), fixed(14, 8)).unwrap();
        t.spacer(Some(root), fixed(5, 5)).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 200, 200));
        // The column wants max(10, 14) + 8 by 6 + 2 + 8 + 8.
        assert_eq!((rect(&t, col).w, rect(&t, col).h), (22, 24));
    }

    #[test]
    fn nothing_is_laid_out_again_until_something_asks() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let child = t.spacer(Some(root), fixed(10, 10)).unwrap();
        let bounds = Rect::new(0, 0, 50, 50);
        t.update(&screen(), &THEME, bounds);
        // Move the child by hand: a second update with no request must not put it back.
        let i = t.slot(child).unwrap();
        t.nodes[i].rect.x = 99;
        t.update(&screen(), &THEME, bounds);
        assert_eq!(rect(&t, child).x, 99);
        t.set_style(child, fixed(11, 10));
        t.update(&screen(), &THEME, bounds);
        assert_eq!(rect(&t, child), Rect::new(0, 0, 11, 10));
        // New bounds are a reason too.
        t.update(&screen(), &THEME, Rect::new(5, 5, 50, 50));
        assert_eq!(rect(&t, child).x, 5);
        // And a change of text.
        let label = t.label(Some(root), "a", TextRole::Body).unwrap();
        t.update(&screen(), &THEME, Rect::new(5, 5, 50, 50));
        let w = rect(&t, label).w;
        t.set_text(label, "abcd");
        t.update(&screen(), &THEME, Rect::new(5, 5, 50, 50));
        assert!(rect(&t, label).w > w);
    }

    #[test]
    fn disabled_flows_down_and_back_up() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let group = t.row(Some(root), Style::new()).unwrap();
        let b = t.button(Some(group), "x", ButtonVariant::Primary).unwrap();
        let other = t.button(Some(root), "y", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        t.set_enabled(group, false);
        assert!(!t.is_disabled(b), "not until the update pass runs");
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        assert!(t.is_disabled(group) && t.is_disabled(b) && !t.is_disabled(other));
        t.set_enabled(group, true);
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        assert!(!t.is_disabled(group) && !t.is_disabled(b));
        // A node added under a disabled parent is disabled at once.
        t.set_enabled(root, false);
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        let late = t.label(Some(group), "late", TextRole::Body).unwrap();
        assert!(t.is_disabled(late));
    }

    #[test]
    fn extreme_sizes_and_empty_trees_do_not_panic() {
        let mut empty = Tree::<2>::new();
        empty.update(&screen(), &THEME, Rect::new(0, 0, 10, 10));
        let mut t = Tree::<8>::new();
        let root = t
            .row(None, Style::new().padding(u32::MAX).gap(u32::MAX))
            .unwrap();
        t.spacer(Some(root), fixed(u32::MAX, u32::MAX)).unwrap();
        t.spacer(Some(root), Style::new().width(Length::Fill(u16::MAX)))
            .unwrap();
        t.spacer(
            Some(root),
            Style::new()
                .width(Length::Fill(u16::MAX))
                .height(Length::Fill(1)),
        )
        .unwrap();
        t.label(Some(root), &"é".repeat(50), TextRole::Title)
            .unwrap();
        for bounds in [
            Rect::new(0, 0, 0, 0),
            Rect::new(i32::MIN, i32::MAX, u32::MAX, u32::MAX),
            Rect::new(i32::MAX, i32::MAX, 1, 1),
        ] {
            t.update(&screen(), &THEME, bounds);
        }
    }

    /// A small deterministic generator, so the property test needs no dependency.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, n: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 33) as u32) % n
        }
    }

    #[test]
    fn random_trees_share_exactly_the_room_that_is_left_and_follow_each_other() {
        let mut rng = Lcg(7);
        for round in 0..400 {
            let mut t = Tree::<40>::new();
            let mut containers = vec![
                t.column(None, Style::new().padding(rng.next(6)).gap(rng.next(5)))
                    .unwrap(),
            ];
            for _ in 0..rng.next(30) {
                let parent = containers[rng.next(containers.len() as u32) as usize];
                let length = |rng: &mut Lcg| match rng.next(3) {
                    0 => Length::Fill(rng.next(4) as u16),
                    1 => Length::Fixed(rng.next(60)),
                    _ => Length::Content,
                };
                let style = Style::new()
                    .width(length(&mut rng))
                    .height(length(&mut rng))
                    .padding(rng.next(4))
                    .gap(rng.next(4))
                    .align(
                        [
                            CrossAlign::Start,
                            CrossAlign::Center,
                            CrossAlign::End,
                            CrossAlign::Stretch,
                        ][rng.next(4) as usize],
                    );
                let id = match rng.next(4) {
                    0 => t.row(Some(parent), style),
                    1 => t.column(Some(parent), style),
                    2 => t.label(Some(parent), "ab", TextRole::Body),
                    _ => t.spacer(Some(parent), style),
                };
                if let Ok(id) = id
                    && matches!(t.nodes[t.slot(id).unwrap()].kind, Kind::Stack(_))
                {
                    containers.push(id);
                }
            }
            let bounds = Rect::new(
                rng.next(20) as i32,
                rng.next(20) as i32,
                rng.next(700),
                rng.next(700),
            );
            t.update(&screen(), &THEME, bounds);
            for &c in &containers {
                let i = t.slot(c).unwrap();
                let Kind::Stack(axis) = t.nodes[i].kind else {
                    continue;
                };
                let (st, rect) = (t.nodes[i].style, t.nodes[i].rect);
                let inner = rect.inset(st.padding);
                let (main_total, cross_total) = match axis {
                    Axis::Column => (inner.h, inner.w),
                    Axis::Row => (inner.w, inner.h),
                };
                let kids: Vec<usize> = t.children(c).map(|k| t.slot(k).unwrap()).collect();
                let main = |k: usize| match axis {
                    Axis::Column => (t.nodes[k].rect.h, t.nodes[k].rect.y),
                    Axis::Row => (t.nodes[k].rect.w, t.nodes[k].rect.x),
                };
                let is_fill =
                    |k: usize| matches!(main_len(axis, &t.nodes[k].style), Length::Fill(_));
                let gaps = st.gap.saturating_mul(kids.len().saturating_sub(1) as u32);
                let taken: u32 = kids
                    .iter()
                    .filter(|&&k| !is_fill(k))
                    .map(|&k| main(k).0)
                    .sum();
                let room = main_total.saturating_sub(gaps).saturating_sub(taken);
                let shared: u32 = kids
                    .iter()
                    .filter(|&&k| is_fill(k))
                    .map(|&k| main(k).0)
                    .sum();
                let any_fill = kids.iter().any(|&k| is_fill(k));
                assert_eq!(
                    shared,
                    if any_fill { room } else { 0 },
                    "round {round}: fill children share exactly the room that is left"
                );
                let mut at = i64::from(match axis {
                    Axis::Column => inner.y,
                    Axis::Row => inner.x,
                });
                for &k in &kids {
                    let (len, pos) = main(k);
                    assert_eq!(
                        i64::from(pos),
                        at,
                        "round {round}: children follow each other along the main axis"
                    );
                    at += i64::from(len) + i64::from(st.gap);
                    let cross = match axis {
                        Axis::Column => t.nodes[k].rect.w,
                        Axis::Row => t.nodes[k].rect.h,
                    };
                    if matches!(cross_len(axis, &t.nodes[k].style), Length::Fill(_)) {
                        assert_eq!(
                            cross, cross_total,
                            "round {round}: a fill child spans the cross axis"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_button_in_a_stretching_column_still_stretches() {
        let mut t = Tree::<4>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let b = t.button(Some(root), "OK", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        assert_eq!(
            rect(&t, b).w,
            100,
            "it draws a background, so the width is seen"
        );
    }
}
