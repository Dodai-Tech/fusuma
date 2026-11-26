//! The rewrite passes: disabled state flowing down, then layout, then hover against the new layout.

use fusuma_render::{Rect, Size};

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

/// Where a walk over the lines of a flow is.
#[derive(Clone, Copy, Debug, Default)]
struct FlowCursor {
    started: bool,
    left: u32,
    next: Option<u16>,
    after: Option<u16>,
    x: u32,
    y: u32,
    line_h: u32,
    widest: u32,
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
    pub fn update<M: fusuma_render::Measure>(&mut self, renderer: &M, theme: &Theme, bounds: Rect) {
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

    fn layout<M: fusuma_render::Measure>(&mut self, renderer: &M, theme: &Theme, bounds: Rect) {
        let Some(root) = self.root.map(usize::from) else {
            return;
        };
        let full = self.nodes[root].sub.has(Flags::LAYOUT) || self.bounds != Some(bounds);
        if !full && !self.nodes[root].sub.has(Flags::PLACE) {
            return;
        }
        let moved = self.bounds != Some(bounds);
        self.bounds = Some(bounds);
        let margin = theme.spacing.lg;
        if full {
            // Measure: every node after its children, the ones whose subtree changed (a node that did not keeps its
            // size). A new size of the screen, a theme or a font changes them all.
            let all = core::mem::take(&mut self.measure_all) || moved;
            if all {
                // A new screen: a flow wraps at what the screen gives it again, not at the width it had (which could only ever
                // make it narrower).
                for n in &mut self.nodes {
                    if let Kind::Flow { wrap_w, .. } = &mut n.kind {
                        *wrap_w = 0;
                    }
                }
            }
            let mut cur = Some(self.leftmost_leaf(root));
            while let Some(i) = cur {
                if all || self.nodes[i].sub.has(Flags::LAYOUT) {
                    self.nodes[i].size = self.measure(i, renderer, theme);
                }
                cur = if i == root { None } else { self.post_next(i) };
            }
            // Place: every node before its children.
            self.place_all(root, bounds, margin);
            // A paragraph wraps to the width its parent gave it, which is only known now: if that is not the width it
            // was measured at, measure again with it (a paragraph's height depends on its width). Twice is enough for
            // text nested in text-sized containers; more would only chase a layout that does not settle.
            for _ in 0..2 {
                if !self.settle_paragraphs() {
                    break;
                }
                let mut cur = Some(self.leftmost_leaf(root));
                while let Some(i) = cur {
                    self.nodes[i].size = self.measure(i, renderer, theme);
                    cur = if i == root { None } else { self.post_next(i) };
                }
                self.place_all(root, bounds, margin);
            }
        } else {
            // Only where children sit changed (a scroll moved, a node slid): sizes are the same, so place the
            // subtrees that asked and leave the rest of the tree alone.
            self.place_dirty(bounds, margin);
        }
        // A widget the keyboard moved to may be scrolled out of sight: scroll to it with the fresh
        // positions, then place again with the new offsets.
        if let Some(target) = self.pending_reveal.take().and_then(|id| self.slot(id)) {
            self.reveal(target);
            if self.nodes[root].sub.has(Flags::LAYOUT) {
                self.place_all(root, bounds, margin);
            } else if self.nodes[root].sub.has(Flags::PLACE) {
                self.place_dirty(bounds, margin);
            }
        }
        // The cursor of a text field a press landed on, and where the caret is.
        self.resolve_input(renderer, theme);
        self.clear_flag(Flags::LAYOUT);
        self.clear_flag(Flags::PLACE);
        // From here the rows are where they will be drawn, so a change in the shifting window can be drawn over it.
        self.shift_laid = self.shift.is_some();
        // Things may have moved under the pointer.
        if let Some(p) = self.pointer {
            let h = self.hit(p);
            self.set_hover(h);
        }
    }

    /// Places the subtrees of the nodes that asked for it ([`Flags::PLACE`]).
    fn place_dirty(&mut self, bounds: Rect, margin: u32) {
        for i in 0..N {
            if self.nodes[i].alive && self.nodes[i].own.has(Flags::PLACE) {
                self.place_from(i, i, bounds, margin);
            }
        }
    }

    /// Remembers for every paragraph and flow the width it was placed at; true when one changed.
    fn settle_paragraphs(&mut self) -> bool {
        let mut changed = false;
        for n in &mut self.nodes {
            if let Kind::Paragraph { wrap_w, .. } | Kind::Flow { wrap_w, .. } = &mut n.kind
                && n.alive
            {
                let width = n.rect.w.saturating_sub(n.style.padding.saturating_mul(2));
                if *wrap_w != width {
                    *wrap_w = width;
                    changed = true;
                }
            }
        }
        changed
    }

    /// Lays the tree out again at the same bounds, for a test that moves things between frames.
    #[cfg(test)]
    pub(crate) fn relayout<M: fusuma_render::Measure>(&mut self, renderer: &M, theme: &Theme) {
        if let Some(b) = self.bounds {
            self.update(renderer, theme, b);
        }
    }

    /// Puts every node where its parent says, from the root down.
    fn place_all(&mut self, root: usize, bounds: Rect, margin: u32) {
        self.set_rect(root, bounds);
        self.place_from(root, root, bounds, margin);
    }

    /// Puts node `start` and everything under it where its parent says (`top` is the node the walk stays inside).
    fn place_from(&mut self, start: usize, top: usize, bounds: Rect, margin: u32) {
        let mut cur = Some(start);
        while let Some(i) = cur {
            // A hidden node is given no place and nothing in it is placed: it keeps what it had for when it is shown.
            if self.nodes[i].hidden {
                cur = self.after_subtree(i, top);
                continue;
            }
            if let Kind::Dialog {
                open,
                popover,
                sheet,
                ..
            } = self.nodes[i].kind
            {
                // A dialog sits in the middle of the screen at its own size, kept `margin` from the edges; a closed
                // one takes no room at all (one that is closing still has its place until it is gone). A menu hangs
                // from its widget instead.
                let rect = if !(open || self.dialog_visible(i)) {
                    Rect::new(bounds.x, bounds.y, 0, 0)
                } else if let Some(p) = popover {
                    self.popover_rect(i, p, bounds, margin.min(4))
                } else if sheet {
                    let s = self.nodes[i].size;
                    let h = s.h.min(bounds.h / 4 * 3);
                    Rect::new(bounds.x, bounds.bottom() - h as i32, bounds.w, h)
                } else {
                    let s = self.nodes[i].size;
                    let w = s.w.min(bounds.w.saturating_sub(margin.saturating_mul(2)));
                    let h = s.h.min(bounds.h.saturating_sub(margin.saturating_mul(2)));
                    let at = crate::paint::centred(bounds, fusuma_render::Size::new(w, h));
                    Rect::new(at.x, at.y, w, h)
                };
                self.set_rect(i, rect);
            }
            if matches!(self.nodes[i].kind, Kind::Toast { .. }) {
                let rect = self.toast_rect(i, bounds, margin);
                self.set_rect(i, rect);
            }
            self.place_children(i);
            cur = self.preorder_next(i, top);
        }
    }

    /// The size node `i` wants, from its content and its style.
    fn measure<M: fusuma_render::Measure>(&self, i: usize, renderer: &M, theme: &Theme) -> Size {
        let n = &self.nodes[i];
        let pad = n.style.padding.saturating_mul(2);
        let content = match n.kind {
            Kind::Label { text, role } => {
                renderer.measure_text(text.as_str(), &theme.text_style(role))
            }
            Kind::Button { text, icon, .. } => {
                let t = Self::measure_icon_text(renderer, theme, icon, text.as_str()).size();
                // In cells a button has brackets and a space inside each.
                let extra = if theme.cells { 2 } else { 0 };
                Size::new(
                    t.w.saturating_add(theme.spacing.md.saturating_mul(2))
                        .saturating_add(extra),
                    t.h.saturating_add(theme.spacing.sm.saturating_mul(2))
                        .max(theme.control_height),
                )
            }
            Kind::Paragraph { text, role, wrap_w } => {
                // Wrapped to the width it was given, its own if that is fixed, else the last one it got, else the
                // screen's.
                let wrap = match n.style.width {
                    Length::Fixed(w) => w.saturating_sub(pad),
                    _ if wrap_w > 0 => wrap_w,
                    _ => self.bounds.map_or(u32::MAX, |b| b.w).saturating_sub(pad),
                };
                Self::measure_paragraph(renderer, theme, text.as_str(), role, wrap)
            }
            Kind::Flow { wrap_w, cols } => {
                // Wrapped to its own width if that is fixed, else the one it was last placed at, else the screen's.
                let avail = match n.style.width {
                    Length::Fixed(w) => w.saturating_sub(pad),
                    _ if wrap_w > 0 => wrap_w,
                    _ => self.bounds.map_or(u32::MAX, |b| b.w).saturating_sub(pad),
                };
                self.flow_size(
                    i,
                    avail,
                    cols,
                    n.style.width == Length::Content && wrap_w == 0,
                )
            }
            Kind::Spacer => Size::new(0, 0),
            Kind::Checkbox { .. }
            | Kind::Radio { .. }
            | Kind::ListItem { .. }
            | Kind::Tab { .. }
            | Kind::Progress { .. }
            | Kind::Gauge { .. }
            | Kind::Spinner { .. }
            | Kind::Switch { .. }
            | Kind::Slider { .. }
            | Kind::Splitter { .. }
            | Kind::Toast { .. }
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
                    if child.skipped() {
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

    /// The next non-skipped child at or after `c`.
    fn live(&self, mut c: Option<u16>) -> Option<u16> {
        while let Some(ci) = c {
            if !self.nodes[usize::from(ci)].skipped() {
                break;
            }
            c = self.nodes[usize::from(ci)].next;
        }
        c
    }

    /// How wide a grid's columns are at `avail`: its width shared out, or, when it has no width of its own yet (`natural`), as
    /// wide as the widest cell.
    fn grid_cell(&self, i: usize, avail: u32, cols: u8, natural: bool) -> u32 {
        let gaps = self.nodes[i]
            .style
            .gap
            .saturating_mul(u32::from(cols).saturating_sub(1));
        let shared = avail.saturating_sub(gaps) / u32::from(cols.max(1));
        if !natural {
            return shared;
        }
        let mut widest = 0;
        let mut c = self.live(self.nodes[i].first);
        while let Some(ci) = c {
            widest = widest.max(self.nodes[usize::from(ci)].size.w);
            c = self.live(self.nodes[usize::from(ci)].next);
        }
        widest.min(shared)
    }

    /// The next child of flow `i` and where it goes, breaking lines at `avail` (a child wider than that has a line of its
    /// own) or, in a grid, after `cols` cells of width `cell`: `(child, x, y, line height, width)`. `None` at the end. Nothing
    /// is allocated, so a layout walks the children once.
    fn flow_next(
        &self,
        i: usize,
        avail: u32,
        grid: (u8, u32),
        cur: &mut FlowCursor,
    ) -> Option<(usize, u32, u32, u32, u32)> {
        let gap = self.nodes[i].style.gap;
        let (cols, cell) = grid;
        if cur.left == 0 {
            // Start a line: how many children fit it, and how tall it is.
            let at = if cur.started {
                cur.after
            } else {
                self.live(self.nodes[i].first)
            };
            let first = at?;
            let (mut width, mut height, mut count) = (0u32, 0u32, 0u32);
            let mut d = Some(first);
            while let Some(di) = d {
                let s = self.nodes[usize::from(di)].size;
                if cols > 0 {
                    if count == u32::from(cols) {
                        break;
                    }
                    width = width
                        .saturating_add(if count > 0 { gap } else { 0 })
                        .saturating_add(cell);
                } else {
                    let add = if count > 0 {
                        gap.saturating_add(s.w)
                    } else {
                        s.w
                    };
                    if count > 0 && width.saturating_add(add) > avail {
                        break;
                    }
                    width = width.saturating_add(add);
                }
                height = height.max(s.h);
                count += 1;
                d = self.live(self.nodes[usize::from(di)].next);
            }
            if cur.started {
                cur.y = cur.y.saturating_add(cur.line_h).saturating_add(gap);
            }
            cur.started = true;
            cur.left = count;
            cur.line_h = height;
            cur.x = 0;
            cur.next = Some(first);
            cur.after = d;
            cur.widest = cur.widest.max(width);
        }
        let ci = cur.next?;
        let w = if cols > 0 {
            cell
        } else {
            self.nodes[usize::from(ci)].size.w
        };
        let out = (usize::from(ci), cur.x, cur.y, cur.line_h, w);
        cur.x = cur.x.saturating_add(w).saturating_add(gap);
        cur.next = self.live(self.nodes[usize::from(ci)].next);
        cur.left -= 1;
        Some(out)
    }

    /// The size of flow `i` wrapped at `avail`: the widest line and the height of all lines.
    fn flow_size(&self, i: usize, avail: u32, cols: u8, natural: bool) -> Size {
        let cell = if cols > 0 {
            self.grid_cell(i, avail, cols, natural)
        } else {
            0
        };
        let mut cur = FlowCursor::default();
        while self.flow_next(i, avail, (cols, cell), &mut cur).is_some() {}
        let h = if cur.started {
            cur.y.saturating_add(cur.line_h)
        } else {
            0
        };
        Size::new(cur.widest, h)
    }

    /// Puts the children of a flow inside it, line by line.
    fn place_flow(&mut self, i: usize) {
        let st = self.nodes[i].style;
        let inner = self.nodes[i].rect.inset(st.padding);
        let mut c = self.nodes[i].first;
        while let Some(ci) = c {
            let ci = usize::from(ci);
            if self.nodes[ci].hidden {
                // No room, and no place: it keeps the state of what it holds for when it comes back.
                let at = self.nodes[i].rect;
                self.set_rect(ci, Rect::new(at.x, at.y, 0, 0));
            }
            c = self.nodes[ci].next;
        }
        let (cols, cell) = match self.nodes[i].kind {
            Kind::Flow { cols, .. } if cols > 0 => (cols, self.grid_cell(i, inner.w, cols, false)),
            _ => (0, 0),
        };
        let mut cur = FlowCursor::default();
        while let Some((ci, x, y, line_h, w)) = self.flow_next(i, inner.w, (cols, cell), &mut cur) {
            let s = self.nodes[ci].size;
            let h = match (self.nodes[ci].style.height, st.align) {
                (Length::Fixed(f), _) => f,
                (_, CrossAlign::Stretch) => line_h,
                _ => s.h,
            };
            let dy = match st.align {
                CrossAlign::Start | CrossAlign::Stretch => 0,
                CrossAlign::Center => line_h.saturating_sub(h) / 2,
                CrossAlign::End => line_h.saturating_sub(h),
            };
            let (sx, sy) = self.nodes[ci].shift;
            let rect = Rect::new(
                offset(inner.x, i64::from(x)),
                offset(inner.y, i64::from(y) + i64::from(dy)),
                w,
                h,
            );
            self.set_rect(ci, rect.translate(i32::from(sx), i32::from(sy)));
        }
    }

    /// Puts the children of a container inside it.
    fn place_children(&mut self, i: usize) {
        if matches!(self.nodes[i].kind, Kind::Flow { .. }) {
            self.place_flow(i);
            return;
        }
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
            if child.skipped() {
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
            if self.nodes[ci].skipped() {
                if self.nodes[ci].hidden {
                    // No room, and no place: it keeps the state of what it holds for when it comes back.
                    let at = self.nodes[i].rect;
                    self.set_rect(ci, Rect::new(at.x, at.y, 0, 0));
                }
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
            let (sx, sy) = self.nodes[ci].shift;
            self.set_rect(ci, rect.translate(i32::from(sx), i32::from(sy)));
            pos += i64::from(main_px) + i64::from(st.gap);
            c = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ButtonVariant, Surface, TextRole};
    use fusuma_render::Renderer;
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

    /// A flow of `n` boxes of `w` x `h` (heights `h + i % 3` when `ragged`) under a root of `room` wide.
    fn flow_of(
        n: usize,
        w: u32,
        h: u32,
        gap: u32,
        room: u32,
        align: CrossAlign,
        ragged: bool,
    ) -> (
        Tree<64>,
        std::vec::Vec<crate::tree::NodeId>,
        crate::tree::NodeId,
    ) {
        let mut t = Tree::<64>::new();
        let root = t.column(None, Style::new()).unwrap();
        let f = t
            .flow(
                Some(root),
                Style::new().width(Length::Fill(1)).gap(gap).align(align),
            )
            .unwrap();
        let kids = (0..n)
            .map(|i| {
                let extra = if ragged { (i % 3) as u32 } else { 0 };
                t.spacer(Some(f), fixed(w, h + extra)).unwrap()
            })
            .collect();
        t.update(&screen(), &THEME, Rect::new(0, 0, room, 400));
        (t, kids, f)
    }

    #[test]
    fn a_flow_wraps_at_its_width_with_gaps_between_children_and_lines() {
        // Three 30 wide boxes with a gap of 5 fit 100 (30+5+30+5+30 = 100): a line of three, then the rest.
        let (t, k, f) = flow_of(7, 30, 10, 5, 100, CrossAlign::Start, false);
        let at = |i: usize| rect(&t, k[i]);
        assert_eq!((at(0).x, at(0).y), (0, 0));
        assert_eq!((at(1).x, at(1).y), (35, 0));
        assert_eq!((at(2).x, at(2).y), (70, 0), "the third fits exactly");
        assert_eq!(
            (at(3).x, at(3).y),
            (0, 15),
            "the fourth starts the next line, a gap below"
        );
        assert_eq!((at(5).x, at(5).y), (70, 15));
        assert_eq!((at(6).x, at(6).y), (0, 30));
        assert_eq!(rect(&t, f).h, 10 * 3 + 5 * 2, "three lines and two gaps");
        // One pixel narrower: only two fit a line.
        let (t, k, _) = flow_of(7, 30, 10, 5, 99, CrossAlign::Start, false);
        assert_eq!((rect(&t, k[2]).x, rect(&t, k[2]).y), (0, 15));
        // Wide enough for everything: one line.
        let (t, k, f) = flow_of(7, 30, 10, 5, 1000, CrossAlign::Start, false);
        assert!(k.iter().all(|c| rect(&t, *c).y == 0));
        assert_eq!(rect(&t, f).h, 10);
    }

    fn grid_of(
        cols: u8,
        n: usize,
        gap: u32,
        width: Length,
        room: u32,
    ) -> (
        Tree<64>,
        std::vec::Vec<crate::tree::NodeId>,
        crate::tree::NodeId,
    ) {
        let mut t = Tree::<64>::new();
        let root = t.column(None, Style::new()).unwrap();
        let g = t
            .grid(Some(root), cols, Style::new().width(width).gap(gap))
            .unwrap();
        let kids = (0..n)
            .map(|i| {
                t.spacer(
                    Some(g),
                    fixed(10 + (i as u32 % 2) * 5, 8 + (i as u32 % 3) * 4),
                )
                .unwrap()
            })
            .collect();
        t.update(&screen(), &THEME, Rect::new(0, 0, room, 300));
        (t, kids, g)
    }

    #[test]
    fn a_grid_shares_its_width_in_equal_columns_and_each_row_is_as_tall_as_its_tallest_cell() {
        // 100 wide, 3 columns, gap 5: (100 - 10) / 3 = 30 each.
        let (t, k, g) = grid_of(3, 7, 5, Length::Fill(1), 100);
        let at = |i: usize| rect(&t, k[i]);
        for i in 0..7 {
            assert_eq!(at(i).w, 30, "cell {i} is a column wide");
            assert_eq!(at(i).x, (i as i32 % 3) * 35, "cell {i}");
        }
        // Row heights: cells 0..2 are 8, 12, 16 tall -> 16; cells 3..5 are 8, 12, 16 -> 16; cell 6 is 8.
        assert_eq!((at(0).y, at(3).y, at(6).y), (0, 21, 42));
        assert_eq!(rect(&t, g).h, 16 + 5 + 16 + 5 + 8);
        assert_eq!(rect(&t, g).w, 100);
        // A fixed width does the same; one column is a column of full-width cells.
        let (t, k, _) = grid_of(2, 4, 0, Length::Fixed(80), 300);
        assert!(k.iter().all(|c| rect(&t, *c).w == 40));
        let (t, k, _) = grid_of(1, 3, 2, Length::Fill(1), 60);
        assert!(
            k.iter()
                .all(|c| rect(&t, *c).w == 60 && rect(&t, *c).x == 0)
        );
        // Fewer cells than columns: the row is not stretched to fill, the cells keep their column width.
        let (t, k, _) = grid_of(4, 2, 0, Length::Fill(1), 100);
        assert_eq!((rect(&t, k[0]).w, rect(&t, k[1]).x), (25, 25));
    }

    #[test]
    fn a_grid_with_no_width_of_its_own_has_columns_as_wide_as_its_widest_cell_and_follows_the_screen()
     {
        let (t, k, g) = grid_of(3, 6, 4, Length::Content, 300);
        // Widest cell is 15: three columns and two gaps.
        assert_eq!(rect(&t, g).w, 15 * 3 + 4 * 2);
        assert!(k.iter().all(|c| rect(&t, *c).w == 15));
        // A screen narrower than that squeezes the columns.
        let (t, _, g) = grid_of(3, 6, 4, Length::Content, 30);
        assert!(rect(&t, g).w <= 30, "{}", rect(&t, g).w);
    }

    #[test]
    fn a_flow_aligns_children_of_different_heights_on_each_line() {
        // Heights 10, 11, 12, 10, 11, 12: lines of three, 12 tall.
        for (align, dy1) in [
            (CrossAlign::Start, 0),
            (CrossAlign::Center, 0),
            (CrossAlign::End, 1),
            (CrossAlign::Stretch, 0),
        ] {
            let (t, k, _) = flow_of(6, 30, 10, 0, 90, align, true);
            let r1 = rect(&t, k[1]);
            assert_eq!(r1.y, dy1, "{align:?}: the 11 high one in a 12 high line");
            let r0 = rect(&t, k[0]);
            let want0 = match align {
                CrossAlign::Start => 0,
                CrossAlign::Center => 1,
                CrossAlign::End => 2,
                CrossAlign::Stretch => 0,
            };
            assert_eq!(r0.y, want0, "{align:?}");
            if align == CrossAlign::Stretch {
                assert_eq!((r0.h, r1.h), (10, 11), "a fixed height stays as it is");
            }
            assert_eq!(
                rect(&t, k[3]).y >= 12,
                true,
                "the second line is below the first"
            );
        }
    }

    #[test]
    fn a_flow_stretches_children_without_a_height_to_the_line() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new()).unwrap();
        let f = t
            .flow(
                Some(root),
                Style::new()
                    .width(Length::Fill(1))
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        let tall = t
            .spacer(
                Some(f),
                Style::new()
                    .width(Length::Fixed(20))
                    .height(Length::Fixed(60)),
            )
            .unwrap();
        let b = t.button(Some(f), "Ok", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 200, 100));
        assert_eq!(rect(&t, b).h, 60, "a button takes the height of the line");
        assert_eq!(rect(&t, tall).h, 60);
    }

    #[test]
    fn a_flow_with_a_child_wider_than_it_gives_that_child_a_line_and_never_loops() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new()).unwrap();
        let f = t
            .flow(Some(root), Style::new().width(Length::Fill(1)).gap(2))
            .unwrap();
        let a = t.spacer(Some(f), fixed(20, 5)).unwrap();
        let big = t.spacer(Some(f), fixed(500, 7)).unwrap();
        let b = t.spacer(Some(f), fixed(20, 5)).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        assert_eq!(rect(&t, a).y, 0);
        assert_eq!(rect(&t, big).y, 7, "on its own line");
        assert_eq!(rect(&t, big).w, 500);
        assert_eq!(
            rect(&t, b).y,
            7 + 7 + 2 - 5 + 5 - 5 + 5,
            "and the next after it"
        );
        // An empty flow and one of only hidden children take no height.
        let mut e = Tree::<4>::new();
        let r = e.column(None, Style::new()).unwrap();
        let f = e.flow(Some(r), Style::new()).unwrap();
        let h = e.spacer(Some(f), fixed(10, 10)).unwrap();
        e.set_hidden(h, true);
        e.update(&screen(), &THEME, Rect::new(0, 0, 50, 50));
        assert_eq!((rect(&e, f).w, rect(&e, f).h), (0, 0));
    }

    #[test]
    fn a_flow_of_content_width_is_as_wide_as_its_widest_line_and_follows_the_room_it_has() {
        // No width given: it wraps at the room the screen has, and is as wide as its widest line.
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new()).unwrap();
        let f = t.flow(Some(root), Style::new().gap(4)).unwrap();
        for _ in 0..5 {
            t.spacer(Some(f), fixed(40, 10)).unwrap();
        }
        t.update(&screen(), &THEME, Rect::new(0, 0, 130, 100));
        // 40+4+40+4+40 = 128 fits 130: lines of three and two.
        assert_eq!((rect(&t, f).w, rect(&t, f).h), (128, 24));
        // The screen narrows: it wraps again at the new width.
        t.update(&screen(), &THEME, Rect::new(0, 0, 100, 100));
        assert_eq!((rect(&t, f).w, rect(&t, f).h), (84, 10 * 3 + 4 * 2));
        t.update(&screen(), &THEME, Rect::new(0, 0, 130, 100));
        assert_eq!(
            (rect(&t, f).w, rect(&t, f).h),
            (128, 24),
            "and widens again"
        );
    }

    #[test]
    fn a_flow_keeps_its_place_in_a_scroll_and_with_padding_and_a_background() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new()).unwrap();
        let f = t
            .flow(
                Some(root),
                Style::new()
                    .width(Length::Fixed(100))
                    .padding(10)
                    .gap(5)
                    .background(Surface::Raised),
            )
            .unwrap();
        let a = t.spacer(Some(f), fixed(40, 10)).unwrap();
        let b = t.spacer(Some(f), fixed(40, 10)).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 200, 200));
        // Inside 100 - 20 = 80: 40 + 5 + 40 = 85 does not fit, so two lines.
        assert_eq!((rect(&t, a).x, rect(&t, a).y), (10, 10));
        assert_eq!((rect(&t, b).x, rect(&t, b).y), (10, 25));
        assert_eq!((rect(&t, f).w, rect(&t, f).h), (100, 10 + 10 + 5 + 10 + 10));
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

    /// A renderer that counts how often it was asked to measure text, so a test can see how much was laid out.
    struct Counting {
        inner: VelloCpu,
        measured: core::cell::Cell<u32>,
    }

    impl Counting {
        fn new() -> Self {
            Self {
                inner: fusuma_render_vello_cpu::new(8, 8).unwrap(),
                measured: core::cell::Cell::new(0),
            }
        }

        /// How many measurements since the last call.
        fn take(&self) -> u32 {
            self.measured.replace(0)
        }
    }

    impl Renderer for Counting {
        type Error = <VelloCpu as Renderer>::Error;
        fn size(&self) -> Size {
            self.inner.size()
        }
        fn capabilities(&self) -> fusuma_render::Capabilities {
            self.inner.capabilities()
        }
        fn save(&mut self) -> Result<(), Self::Error> {
            self.inner.save()
        }
        fn restore(&mut self) -> Result<(), Self::Error> {
            self.inner.restore()
        }
        fn clip(&mut self, r: Rect) -> Result<(), Self::Error> {
            self.inner.clip(r)
        }
        fn translate(&mut self, dx: i32, dy: i32) -> Result<(), Self::Error> {
            self.inner.translate(dx, dy)
        }
        fn clear(&mut self, c: fusuma_render::Color) -> Result<(), Self::Error> {
            self.inner.clear(c)
        }
        fn fill_rect(
            &mut self,
            r: Rect,
            rad: u32,
            c: fusuma_render::Color,
        ) -> Result<(), Self::Error> {
            self.inner.fill_rect(r, rad, c)
        }
        fn stroke_rect(
            &mut self,
            r: Rect,
            rad: u32,
            w: u32,
            c: fusuma_render::Color,
        ) -> Result<(), Self::Error> {
            self.inner.stroke_rect(r, rad, w, c)
        }
        fn fill_circle(
            &mut self,
            p: fusuma_render::Point,
            r: u32,
            c: fusuma_render::Color,
        ) -> Result<(), Self::Error> {
            self.inner.fill_circle(p, r, c)
        }
        fn fill_path(
            &mut self,
            p: &[fusuma_render::PathCmd],
            c: fusuma_render::Color,
        ) -> Result<(), Self::Error> {
            self.inner.fill_path(p, c)
        }
        fn stroke_path(
            &mut self,
            p: &[fusuma_render::PathCmd],
            w: u32,
            c: fusuma_render::Color,
        ) -> Result<(), Self::Error> {
            self.inner.stroke_path(p, w, c)
        }
        fn draw_text(
            &mut self,
            t: &str,
            p: fusuma_render::Point,
            s: &fusuma_render::TextStyle,
        ) -> Result<(), Self::Error> {
            self.inner.draw_text(t, p, s)
        }
        fn measure_text(&self, t: &str, s: &fusuma_render::TextStyle) -> Size {
            self.measured.set(self.measured.get() + 1);
            self.inner.measure_text(t, s)
        }
        fn draw_image(
            &mut self,
            i: &fusuma_render::Image<'_>,
            p: fusuma_render::Point,
        ) -> Result<(), Self::Error> {
            self.inner.draw_image(i, p)
        }
    }

    fn page(r: &Counting) -> (Tree<64>, Vec<crate::tree::NodeId>, crate::tree::NodeId) {
        let mut t = Tree::<64>::new();
        let root = t.column(None, Style::new().gap(4)).unwrap();
        let mut labels = Vec::new();
        for g in 0..4 {
            let group = t.column(Some(root), Style::new().gap(2)).unwrap();
            for k in 0..4 {
                labels.push(
                    t.label(Some(group), &format!("label {g} {k}"), TextRole::Body)
                        .unwrap(),
                );
            }
        }
        let list = t
            .scroll(Some(root), Style::new().height(Length::Fixed(60)))
            .unwrap();
        for k in 0..10 {
            t.list_item(Some(list), &format!("row {k}"), false).unwrap();
        }
        t.update(r, &THEME, Rect::new(0, 0, 300, 400));
        (t, labels, list)
    }

    #[test]
    fn a_change_measures_only_what_it_touches_and_a_scroll_measures_nothing() {
        let r = Counting::new();
        let (mut t, labels, list) = page(&r);
        assert!(r.take() >= 26, "the first layout measures everything");
        t.update(&r, &THEME, Rect::new(0, 0, 300, 400));
        assert_eq!(r.take(), 0, "nothing changed, nothing measured");
        // One label's text: that label, not the other fifteen and the ten rows.
        t.set_text(labels[5], "a changed label");
        t.update(&r, &THEME, Rect::new(0, 0, 300, 400));
        assert_eq!(r.take(), 1, "one label");
        // A scroll: the rows move, no size is asked for.
        t.scroll_to(list, 20);
        t.update(&r, &THEME, Rect::new(0, 0, 300, 400));
        assert_eq!(r.take(), 0, "a scroll only places");
        {
            let at = t.rect(list).unwrap();
            t.pointer(crate::event::Pointer::Wheel(
                fusuma_render::Point::new(at.x + 5, at.y + 5),
                9,
            ));
        }
        t.update(&r, &THEME, Rect::new(0, 0, 300, 400));
        assert_eq!(t.scroll_offset(list), Some(29));
        // A new size of the screen, or invalidate, measures all.
        t.update(&r, &THEME, Rect::new(0, 0, 280, 400));
        assert!(r.take() >= 26);
        t.invalidate();
        t.update(&r, &THEME, Rect::new(0, 0, 280, 400));
        assert!(r.take() >= 26, "a theme change measures all");
    }

    /// Mixed changes, each followed by a layout of only what changed, give the same places as a layout from scratch.
    #[test]
    fn laying_out_only_what_changed_gives_the_places_a_full_layout_gives() {
        let r = Counting::new();
        let (mut t, labels, list) = page(&r);
        let mut seed = 5u64;
        let mut next = |n: u32| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((seed >> 33) as u32) % n
        };
        let rows: Vec<_> = t.children(list).collect();
        for step in 0..80 {
            match next(6) {
                0 => t.set_text(
                    labels[next(16) as usize],
                    ["a", "much longer text than before", "mid length"][next(3) as usize],
                ),
                1 => t.set_text(
                    rows[next(10) as usize],
                    ["row", "a row with a longer name"][next(2) as usize],
                ),
                2 => t.scroll_to(list, next(200) as i32),
                3 => {
                    let g = t
                        .children(t.root_id().unwrap())
                        .nth(next(4) as usize)
                        .unwrap();
                    t.set_style(g, Style::new().gap(next(9)).padding(next(6)));
                }
                4 => {
                    let g = t
                        .children(t.root_id().unwrap())
                        .nth(next(4) as usize)
                        .unwrap();
                    let _ = t.label(Some(g), "added", TextRole::Caption);
                }
                _ => t.set_style(list, Style::new().height(Length::Fixed(30 + next(120)))),
            }
            t.update(&r, &THEME, Rect::new(0, 0, 300, 400));
            let mut fresh = t.clone();
            fresh.invalidate();
            fresh.update(&r, &THEME, Rect::new(0, 0, 300, 400));
            for i in 0..64 {
                assert_eq!(
                    t.nodes[i].rect, fresh.nodes[i].rect,
                    "step {step}, node {i}"
                );
                assert_eq!(
                    t.nodes[i].size, fresh.nodes[i].size,
                    "step {step}, node {i} size"
                );
            }
        }
    }
}
