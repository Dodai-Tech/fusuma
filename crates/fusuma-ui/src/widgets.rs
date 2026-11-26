//! Checkbox, radio button, list item and progress bar: what pressing them does, how big they
//! are and how they look.

use fusuma_render::{Color, PathCmd, Point, Rect, Renderer, Size, TextStyle};

use crate::paint::centred;
use crate::theme::Theme;
use crate::tree::{Action, ActionKind, Flags, Kind, NodeId, Tree};

/// The colours of a checkbox or a radio button right now.
struct Mark {
    fill: Color,
    border: Color,
    glyph: Color,
    text: Color,
}

/// `base` moved right or down by `d` pixels, held at the edge of the coordinate range instead of
/// overflowing: a node can be placed anywhere.
fn px(base: i32, d: u32) -> i32 {
    pxs(base, i64::from(d))
}

/// `base` moved by `d` pixels (either way), held within the coordinate range.
fn pxs(base: i32, d: i64) -> i32 {
    (i64::from(base) + d).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

impl<const N: usize> Tree<N> {
    /// What pressing node `i` does, from the pointer or the keyboard: the one place the widgets'
    /// behaviour lives.
    pub(crate) fn activate(&mut self, i: usize) {
        if self.nodes[i].disabled_eff {
            return;
        }
        let source = self.id_of(i);
        // A row or a button of a menu is an action, and the menu closes once it was chosen.
        if matches!(
            self.nodes[i].kind,
            Kind::Button { .. } | Kind::ListItem { .. } | Kind::Tab { .. }
        ) && let Some(menu) = self.open_menu_of(i)
        {
            self.push_action(Action {
                source,
                kind: ActionKind::Clicked,
            });
            let menu = self.id_of(menu);
            self.close_dialog(menu);
            return;
        }
        let kind = match self.nodes[i].kind {
            Kind::Button { .. } => Some(ActionKind::Clicked),
            Kind::Checkbox { checked: on, .. } | Kind::Switch { on, .. } => {
                self.set_flag(i, !on);
                Some(ActionKind::Toggled(!on))
            }
            // Enter in a text field submits it.
            Kind::TextInput { .. } => Some(ActionKind::Clicked),
            Kind::Radio { checked, .. } if !checked => {
                self.choose(i);
                Some(ActionKind::Selected)
            }
            Kind::ListItem { selected, .. } | Kind::Tab { selected, .. } if !selected => {
                self.choose(i);
                Some(ActionKind::Selected)
            }
            _ => None,
        };
        if let Some(kind) = kind {
            self.push_action(Action { source, kind });
        }
    }

    /// Sets the on/off state of node `i`; redraws it if that changed.
    fn set_flag(&mut self, i: usize, on: bool) {
        if let Some(f) = self.nodes[i].kind.flag_mut()
            && *f != on
        {
            *f = on;
            self.request(i, Flags::PAINT);
        }
    }

    /// Chooses node `i` and clears the others of its group: the siblings of the same kind.
    fn choose(&mut self, i: usize) {
        let same = core::mem::discriminant(&self.nodes[i].kind);
        let mut sibling = self.nodes[i]
            .parent
            .and_then(|p| self.nodes[usize::from(p)].first)
            .map(usize::from);
        while let Some(s) = sibling {
            if s != i && core::mem::discriminant(&self.nodes[s].kind) == same {
                self.set_flag(s, false);
            }
            sibling = self.nodes[s].next.map(usize::from);
        }
        self.set_flag(i, true);
    }

    /// Whether a checkbox is checked, or a radio button or list item chosen.
    #[must_use]
    pub fn is_checked(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(|i| match self.nodes[i].kind {
            Kind::Checkbox { checked, .. } | Kind::Radio { checked, .. } => checked,
            Kind::Switch { on, .. } => on,
            Kind::ListItem { selected, .. } | Kind::Tab { selected, .. } => selected,
            _ => false,
        })
    }

    /// Checks or unchecks a checkbox, or chooses a radio button or list item (which clears the
    /// others of its group; unchecking one leaves the group without a choice). It sends no action:
    /// actions report what the user did.
    pub fn set_checked(&mut self, id: NodeId, on: bool) {
        let Some(i) = self.slot(id) else { return };
        if matches!(
            self.nodes[i].kind,
            Kind::Radio { .. } | Kind::ListItem { .. } | Kind::Tab { .. }
        ) && on
        {
            self.choose(i);
        } else {
            self.set_flag(i, on);
        }
    }

    /// A slider or a splitter: the widgets a press on drags.
    pub(crate) const fn is_slider(&self, i: usize) -> bool {
        matches!(
            self.nodes[i].kind,
            Kind::Slider { .. } | Kind::Splitter { .. }
        )
    }

    /// Whether splitter `i` divides a column (the panes are one above the other).
    pub(crate) fn splitter_is_vertical(&self, i: usize) -> bool {
        self.nodes[i].parent.is_some_and(|p| {
            matches!(
                self.nodes[usize::from(p)].kind,
                Kind::Stack(crate::tree::Axis::Column)
            )
        })
    }

    /// Moves slider `i` to `to` thousandths; sends [`ActionKind::Changed`] if that is a change.
    pub(crate) fn move_slider(&mut self, i: usize, to: u16) {
        let to = to.min(1000);
        let (Kind::Slider { permille: old } | Kind::Splitter { permille: old }) =
            self.nodes[i].kind
        else {
            return;
        };
        if old == to {
            return;
        }
        if let Kind::Slider { permille } | Kind::Splitter { permille } = &mut self.nodes[i].kind {
            *permille = to;
        }
        let source = self.id_of(i);
        if matches!(self.nodes[i].kind, Kind::Splitter { .. }) {
            // The bar and what it divides move with the app's answer: draw the bar, and the layout is the app's.
            self.request(i, Flags::PAINT);
            self.push_action(Action {
                source,
                kind: ActionKind::Changed(to),
            });
            return;
        }
        // Only the stretch between the old and the new thumb changes: the thumb, and the filled part of the track.
        // Marking the whole slider changed would redraw a screen-wide strip for a thumb that moved a few pixels.
        let r = self.nodes[i].rect;
        let thumb = r.h.min(r.w);
        let span = i64::from(r.w.saturating_sub(thumb));
        let at = |p: u16| i64::from(r.x) + i64::from(thumb / 2) + span * i64::from(p) / 1000;
        let (a, b) = (at(old).min(at(to)), at(old).max(at(to)));
        let pad = i64::from(thumb / 2) + 2;
        let x0 = (a - pad).max(i64::from(r.x));
        let x1 = (b + pad).min(i64::from(r.right()));
        let part = Rect::new(x0 as i32, r.y, (x1 - x0).max(0) as u32, r.h);
        self.request_part(i, Flags::PAINT, part);
        self.push_action(Action {
            source,
            kind: ActionKind::Changed(to),
        });
    }

    /// Moves slider or splitter `i` to where the pointer is. A splitter goes to where the pointer is along its parent.
    pub(crate) fn slider_to(&mut self, i: usize, p: Point) {
        if !matches!(self.nodes[i].kind, Kind::Splitter { .. }) {
            self.slider_to_x(i, p.x);
            return;
        }
        let Some(parent) = self.nodes[i].parent else {
            return;
        };
        let inner = self.nodes[usize::from(parent)]
            .rect
            .inset(self.nodes[usize::from(parent)].style.padding);
        let (pos, from, len) = if self.splitter_is_vertical(i) {
            (p.y, inner.y, inner.h)
        } else {
            (p.x, inner.x, inner.w)
        };
        if len == 0 {
            return;
        }
        let along = i64::from(pos) - i64::from(from);
        let permille =
            (along.clamp(0, i64::from(len)) * 1000 + i64::from(len) / 2) / i64::from(len);
        self.move_slider(i, permille as u16);
    }

    /// Moves slider `i` to where the pointer is, `x` in tree coordinates. The thumb is as wide as the
    /// slider is high, so its centre runs from half a thumb in from each end.
    pub(crate) fn slider_to_x(&mut self, i: usize, x: i32) {
        let r = self.nodes[i].rect;
        let thumb = r.h.min(r.w);
        let span = i64::from(r.w.saturating_sub(thumb));
        if span == 0 {
            return;
        }
        let along = i64::from(x) - i64::from(r.x) - i64::from(thumb / 2);
        let permille = (along.clamp(0, span) * 1000 + span / 2) / span;
        self.move_slider(i, permille as u16);
    }

    /// How far a slider is along, in thousandths.
    #[must_use]
    pub fn slider_value(&self, id: NodeId) -> Option<u16> {
        match self.nodes[self.slot(id)?].kind {
            Kind::Slider { permille } | Kind::Splitter { permille } => Some(permille),
            _ => None,
        }
    }

    /// Sets a slider (0 to 1000; more is 1000). It sends no action: actions report what the user did.
    pub fn set_slider(&mut self, id: NodeId, permille: u16) {
        let Some(i) = self.slot(id) else { return };
        let permille = permille.min(1000);
        if let Kind::Slider { permille: p } | Kind::Splitter { permille: p } =
            &mut self.nodes[i].kind
            && *p != permille
        {
            *p = permille;
            self.request(i, Flags::PAINT);
        }
    }

    pub(crate) fn scroll_offset_at(&self, i: usize) -> i32 {
        match self.nodes[i].kind {
            Kind::Scroll { offset, .. } => offset,
            _ => 0,
        }
    }

    /// The furthest container `i` can scroll, in pixels, from the last layout.
    pub(crate) fn scroll_max_at(&self, i: usize) -> i32 {
        match self.nodes[i].kind {
            Kind::Scroll { content, .. } => {
                let view = self.nodes[i].rect.inset(self.nodes[i].style.padding).h;
                i32::try_from(content.saturating_sub(view)).unwrap_or(i32::MAX)
            }
            _ => 0,
        }
    }

    /// Scrolls container `i` to `to` pixels (cut to what the content allows); the layout is redone.
    pub(crate) fn set_scroll(&mut self, i: usize, to: i32) {
        // Code, the wheel and the keyboard take the scroll over from a fling or a bounce.
        self.cancel_motion(i);
        let to = to.clamp(0, self.scroll_max_at(i));
        if let Kind::Scroll { offset, over, .. } = &mut self.nodes[i].kind
            && (*offset != to || *over != 0)
        {
            let before = *offset + *over;
            *offset = to;
            *over = 0;
            self.note_scroll(i, before);
        }
    }

    /// A press inside a scroll container that has moved far enough is a scroll: the widget that was
    /// pressed lets go without a click, and the container follows the pointer.
    pub(crate) fn drag_scroll(&mut self, p: Point) {
        let Some(mut d) = self.drag else { return };
        let Some(s) = self.slot(d.scroll) else {
            self.drag = None;
            return;
        };
        // A press on a slider is the slider's: it drags sideways, and a finger drifting up or down while it does must
        // not turn the gesture into a scroll of the page it sits on.
        if self
            .captured
            .and_then(|id| self.slot(id))
            .is_some_and(|c| self.is_slider(c))
        {
            self.drag = None;
            return;
        }
        if !d.moving
            && (i64::from(p.y) - i64::from(d.start_y)).abs() >= i64::from(crate::event::SCROLL_SLOP)
        {
            d.moving = true;
            // Take the press away from the widget under the finger.
            if let Some(held) = self.captured.take().and_then(|id| self.slot(id)) {
                self.set_active(held, false);
            }
        }
        if d.moving {
            let raw = i64::from(d.offset0) - (i64::from(p.y) - i64::from(d.start_y));
            d.raw = raw.clamp(-30_000, 30_000) as i32;
            // Past an end the content resists, like a rubber band, instead of stopping dead.
            let (offset, over) = self.rubber(s, d.raw);
            self.set_motion_offset(s, offset, over);
        }
        self.drag = Some(d);
    }

    /// Scrolls every container above node `i` just enough to show it.
    pub(crate) fn reveal(&mut self, i: usize) {
        let mut cur = self.nodes[i].parent.map(usize::from);
        while let Some(s) = cur {
            if matches!(self.nodes[s].kind, Kind::Scroll { .. }) {
                let view = self.nodes[s].rect.inset(self.nodes[s].style.padding);
                let r = self.nodes[i].rect;
                let off = self.scroll_offset_at(s);
                // 64-bit: the rectangles can be anywhere in the coordinate range.
                let to = if r.y < view.y {
                    i64::from(off) - (i64::from(view.y) - i64::from(r.y))
                } else if r.bottom() > view.bottom() {
                    i64::from(off) + (i64::from(r.bottom()) - i64::from(view.bottom()))
                } else {
                    i64::from(off)
                };
                self.set_scroll(s, to.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
            }
            cur = self.nodes[s].parent.map(usize::from);
        }
    }

    /// How far container `id` is scrolled, in pixels.
    #[must_use]
    pub fn scroll_offset(&self, id: NodeId) -> Option<i32> {
        let i = self.slot(id)?;
        matches!(self.nodes[i].kind, Kind::Scroll { .. }).then(|| self.scroll_offset_at(i))
    }

    /// How far container `id` can scroll in all, in pixels (0 when its content fits), as of the last
    /// layout.
    #[must_use]
    pub fn scroll_max(&self, id: NodeId) -> Option<i32> {
        let i = self.slot(id)?;
        matches!(self.nodes[i].kind, Kind::Scroll { .. }).then(|| self.scroll_max_at(i))
    }

    /// Scrolls container `id` to `offset` pixels, cut to what the content allows. Sends no action.
    pub fn scroll_to(&mut self, id: NodeId, offset: i32) {
        if let Some(i) = self.slot(id) {
            self.set_scroll(i, offset);
        }
    }

    /// How full a progress bar is, in thousandths.
    #[must_use]
    pub fn progress_value(&self, id: NodeId) -> Option<u16> {
        match self.nodes[self.slot(id)?].kind {
            Kind::Progress { permille } => Some(permille),
            _ => None,
        }
    }

    /// Where a spinner is in its turn, in tenths of a degree.
    #[must_use]
    pub fn spinner_phase(&self, id: NodeId) -> Option<u16> {
        match self.nodes[self.slot(id)?].kind {
            Kind::Spinner { phase } => Some(phase),
            _ => None,
        }
    }

    /// How full a gauge is, in thousandths.
    #[must_use]
    pub fn gauge_value(&self, id: NodeId) -> Option<u16> {
        match self.nodes[self.slot(id)?].kind {
            Kind::Gauge { permille, .. } => Some(permille),
            _ => None,
        }
    }

    /// Sets how full a gauge is, in thousandths (0 to 1000; more is 1000).
    pub fn set_gauge(&mut self, id: NodeId, permille: u16) {
        let Some(i) = self.slot(id) else { return };
        let permille = permille.min(1000);
        if let Kind::Gauge { permille: p, .. } = &mut self.nodes[i].kind
            && *p != permille
        {
            *p = permille;
            self.request(i, Flags::PAINT);
        }
    }

    /// Sets how full a progress bar is, in thousandths (0 to 1000; more is 1000).
    pub fn set_progress(&mut self, id: NodeId, permille: u16) {
        let Some(i) = self.slot(id) else { return };
        let permille = permille.min(1000);
        if let Kind::Progress { permille: p } = &mut self.nodes[i].kind
            && *p != permille
        {
            *p = permille;
            self.request(i, Flags::PAINT);
        }
    }

    /// The size the widget wants, from its content and the theme.
    pub(crate) fn measure_widget<M: fusuma_render::Measure>(
        &self,
        i: usize,
        r: &M,
        theme: &Theme,
    ) -> Size {
        let body = TextStyle::new(theme.text.body, Color::BLACK).font(theme.fonts.body);
        let text = |t: &str| r.measure_text(t, &body);
        match self.nodes[i].kind {
            Kind::Checkbox { text: t, .. }
            | Kind::Radio { text: t, .. }
            | Kind::Switch { text: t, .. } => {
                let t = text(t.as_str());
                let box_w = if matches!(self.nodes[i].kind, Kind::Switch { .. }) {
                    theme.indicator.saturating_mul(2)
                } else {
                    theme.indicator
                };
                let w = box_w.saturating_add(theme.spacing.md).saturating_add(t.w);
                // A box is as high as it is wide, except in cells, where it is a word of the line.
                let box_h = if theme.cells { 0 } else { theme.indicator };
                Size::new(w, t.h.max(box_h).max(theme.control_height))
            }
            Kind::Tab { text: t, icon, .. } => {
                let t = Self::measure_icon_text(r, theme, icon, t.as_str()).size();
                // In cells a tab has a space inside its brackets.
                let extra = if theme.cells { 2 } else { 0 };
                Size::new(
                    t.w.saturating_add(theme.spacing.md.saturating_mul(2))
                        .saturating_add(extra),
                    t.h.max(theme.control_height),
                )
            }
            Kind::ListItem { text: t, icon, .. } => {
                let t = Self::measure_icon_text(r, theme, icon, t.as_str()).size();
                // In cells a row has a marker in front of its text.
                let extra = if theme.cells { 2 } else { 0 };
                Size::new(
                    t.w.saturating_add(theme.spacing.md.saturating_mul(2))
                        .saturating_add(extra),
                    t.h.max(theme.control_height),
                )
            }
            Kind::Toast { text: t, .. } => {
                let t = text(t.as_str());
                Size::new(
                    t.w.saturating_add(theme.spacing.md.saturating_mul(2)),
                    t.h.saturating_add(theme.spacing.sm.saturating_mul(2))
                        .max(theme.control_height),
                )
            }
            Kind::Progress { .. } => Size::new(0, theme.track_height),
            Kind::Spinner { .. } if theme.cells => Size::new(1, 1),
            Kind::Spinner { .. } => {
                let side = theme.control_height;
                Size::new(side, side)
            }
            Kind::Gauge { .. } if theme.cells => Size::new(theme.indicator.saturating_mul(4), 1),
            Kind::Gauge { .. } => {
                let side = theme.control_height.saturating_mul(2);
                Size::new(side, side)
            }
            Kind::TextInput { .. } if theme.cells => Size::new(24, 1),
            Kind::TextInput { .. } => {
                Size::new(theme.control_height.saturating_mul(6), theme.control_height)
            }
            Kind::Chart { .. } if theme.cells => Size::new(24, 4),
            Kind::Chart { .. } => Size::new(
                theme.control_height.saturating_mul(6),
                theme.control_height.saturating_mul(3),
            ),
            Kind::Image { w, h, .. } if theme.cells => {
                // In cells a picture is as many cells as fit about 8 pixels each across and 16 down.
                Size::new(u32::from(w).div_ceil(8), u32::from(h).div_ceil(16))
            }
            Kind::Image { w, h, .. } => Size::new(u32::from(w), u32::from(h)),
            Kind::Splitter { .. } => {
                let bar = if theme.cells {
                    1
                } else {
                    theme.spacing.sm.max(4)
                };
                if self.splitter_is_vertical(i) {
                    Size::new(0, bar)
                } else {
                    Size::new(bar, 0)
                }
            }
            Kind::Slider { .. } if theme.cells => Size::new(theme.indicator.saturating_mul(6), 1),
            Kind::Slider { .. } => Size::new(
                theme.indicator.saturating_mul(6),
                theme.indicator.max(theme.track_height),
            ),
            _ => Size::new(0, 0),
        }
    }

    /// The state of node `i` as a style asks for it: disabled, pressed, hovered, or at rest.
    pub(crate) fn widget_state(&self, i: usize) -> crate::theme::ButtonState {
        use crate::theme::ButtonState;
        let n = &self.nodes[i];
        if n.disabled_eff {
            ButtonState::Disabled
        } else if n.active {
            ButtonState::Pressed
        } else if n.hovered {
            ButtonState::Hovered
        } else {
            ButtonState::Idle
        }
    }

    fn focus_ring_shown(&self, i: usize) -> bool {
        self.focus_visible && self.focus == Some(self.id_of(i))
    }

    fn mark_look(&self, i: usize, theme: &Theme, checked: bool) -> Mark {
        let c = &theme.colors;
        let n = &self.nodes[i];
        if n.disabled_eff {
            Mark {
                fill: if checked { c.border } else { c.surface },
                border: c.border,
                glyph: c.surface,
                text: c.text_disabled,
            }
        } else if checked {
            let fill = if n.active {
                c.primary_active
            } else if n.hovered {
                c.primary_hover
            } else {
                c.primary
            };
            Mark {
                fill,
                border: fill,
                glyph: c.on_primary,
                text: c.text,
            }
        } else {
            Mark {
                fill: if n.active {
                    c.surface_raised
                } else {
                    c.surface
                },
                border: if n.hovered || n.active {
                    c.primary
                } else {
                    c.border_strong
                },
                glyph: c.on_primary,
                text: c.text,
            }
        }
    }

    pub(crate) fn paint_widget<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let n = &self.nodes[i];
        let rect = n.rect;
        let c = &theme.colors;
        match n.kind {
            Kind::Checkbox { text, checked } | Kind::Radio { text, checked } => {
                let round = matches!(n.kind, Kind::Radio { .. });
                let look = self.mark_look(i, theme, checked);
                let ind = theme.indicator;
                let top = px(rect.y, rect.h.saturating_sub(ind) / 2);
                let bx = Rect::new(rect.x, top, ind, ind);
                let bw = theme.border_width;
                if round {
                    let centre = Point::new(px(bx.x, ind / 2), px(bx.y, ind / 2));
                    r.fill_circle(centre, ind / 2, look.border)?;
                    r.fill_circle(centre, (ind / 2).saturating_sub(bw), look.fill)?;
                    if checked {
                        r.fill_circle(centre, (ind / 5).max(2), look.glyph)?;
                    }
                } else {
                    r.fill_rect(bx, theme.radii.sm, look.fill)?;
                    r.stroke_rect(bx, theme.radii.sm, bw, look.border)?;
                    if checked {
                        let at = |fx: i32, fy: i32| {
                            Point::new(
                                pxs(bx.x, i64::from(ind) * i64::from(fx) / 100),
                                pxs(bx.y, i64::from(ind) * i64::from(fy) / 100),
                            )
                        };
                        let tick = [
                            PathCmd::MoveTo(at(24, 52)),
                            PathCmd::LineTo(at(43, 71)),
                            PathCmd::LineTo(at(77, 30)),
                        ];
                        r.stroke_path(&tick, 2, look.glyph)?;
                    }
                }
                let style = TextStyle::new(theme.text.body, look.text).font(theme.fonts.body);
                let size = r.measure_text(text.as_str(), &style);
                let at = Point::new(
                    px(rect.x, ind.saturating_add(theme.spacing.md)),
                    centred(rect, size).y,
                );
                Self::in_clip(r, rect, |r| r.draw_text(text.as_str(), at, &style))?;
                self.ring(i, r, theme, theme.radii.sm)
            }
            Kind::ListItem {
                text,
                selected,
                icon,
            } => {
                let state = self.widget_state(i);
                let look = theme.item_in(n.class, state, selected);
                if let Some(fill) = look.fill {
                    r.fill_rect(rect, look.radius, fill)?;
                }
                let label = look.text;
                let m = Self::measure_icon_text(r, theme, icon, text.as_str());
                let x = px(rect.x, theme.spacing.md);
                Self::paint_icon_text(r, theme, rect, x, m, text.as_str(), label)?;
                self.ring(i, r, theme, look.radius)
            }
            Kind::Switch { text, on } => {
                let look = self.mark_look(i, theme, on);
                let h = theme.indicator;
                let w = h.saturating_mul(2);
                let top = px(rect.y, rect.h.saturating_sub(h) / 2);
                let track = Rect::new(rect.x, top, w, h);
                let off_fill = if n.disabled_eff {
                    c.surface
                } else {
                    c.surface_raised
                };
                r.fill_rect(track, h / 2, if on { look.fill } else { off_fill })?;
                if !on {
                    r.stroke_rect(track, h / 2, theme.border_width, look.border)?;
                }
                let pad = (h / 6).max(2);
                let d = h.saturating_sub(pad.saturating_mul(2));
                let knob_x = if on {
                    px(track.x, w.saturating_sub(pad).saturating_sub(d))
                } else {
                    px(track.x, pad)
                };
                let knob = Point::new(px(knob_x, d / 2), px(top, h / 2));
                let knob_color = if on { look.glyph } else { look.border };
                r.fill_circle(knob, d / 2, knob_color)?;
                let style = TextStyle::new(theme.text.body, look.text).font(theme.fonts.body);
                let size = r.measure_text(text.as_str(), &style);
                let at = Point::new(
                    px(rect.x, w.saturating_add(theme.spacing.md)),
                    centred(rect, size).y,
                );
                Self::in_clip(r, rect, |r| r.draw_text(text.as_str(), at, &style))?;
                self.ring(i, r, theme, theme.radii.sm)
            }
            Kind::TextInput { .. } => self.paint_text_input(i, r, theme),
            Kind::Chart { data, len, style } => {
                let c = &theme.colors;
                let cnt = usize::from(len);
                // A little room round the plot so a dot or the edge of a bar is not cut off.
                let plot = rect.inset(theme.border_width.max(2));
                if plot.w < 2 || plot.h < 2 {
                    return Ok(());
                }
                let base = plot.bottom();
                // The baseline and two faint lines between it and the top.
                r.fill_rect(Rect::new(plot.x, base - 1, plot.w, 1), 0, c.border)?;
                for k in 1..3 {
                    let y = base - (plot.h as i32 * k) / 3;
                    r.fill_rect(Rect::new(plot.x, y, plot.w, 1), 0, c.surface_raised)?;
                }
                if cnt == 0 {
                    return Ok(());
                }
                let color = if n.disabled_eff { c.border } else { c.primary };
                // Where sample `k` is: spread across the width (a single sample is in the middle).
                let at = |k: usize| {
                    let x = if cnt == 1 {
                        plot.x + plot.w as i32 / 2
                    } else {
                        plot.x + (plot.w as i64 - 1) as i32 * k as i32 / (cnt as i32 - 1)
                    };
                    let v = i32::from(data[k]);
                    let y = base - 1 - (plot.h as i32 - 1) * v / 255;
                    Point::new(x, y)
                };
                match style {
                    crate::tree::ChartStyle::Bars => {
                        let slot = plot.w / cnt as u32;
                        let bar = slot.saturating_sub(slot / 4).max(1);
                        for k in 0..cnt {
                            let h = ((plot.h as i64 * i64::from(data[k])) / 255) as u32;
                            if h > 0 {
                                let x = plot.x + (slot * k as u32 + (slot - bar) / 2) as i32;
                                r.fill_rect(
                                    Rect::new(x, base - h as i32, bar, h),
                                    bar.min(4) / 2,
                                    color,
                                )?;
                            }
                        }
                    }
                    crate::tree::ChartStyle::Line | crate::tree::ChartStyle::Area => {
                        if style == crate::tree::ChartStyle::Area && cnt > 1 {
                            // The area as one column of pixels' width per step, so it works on every renderer and
                            // stays inside the integer renderer's path limit.
                            for k in 0..cnt - 1 {
                                let (a, b) = (at(k), at(k + 1));
                                let top = a.y.min(b.y);
                                let wide = (b.x - a.x).max(1) as u32;
                                r.fill_rect(
                                    Rect::new(a.x, top, wide, (base - 1 - top).max(0) as u32),
                                    0,
                                    crate::theme::mix(c.surface, color, 250),
                                )?;
                            }
                        }
                        for k in 0..cnt.saturating_sub(1) {
                            r.stroke_path(
                                &[PathCmd::MoveTo(at(k)), PathCmd::LineTo(at(k + 1))],
                                2,
                                color,
                            )?;
                        }
                        for k in 0..cnt {
                            r.fill_circle(at(k), 2, color)?;
                        }
                    }
                }
                Ok(())
            }
            Kind::Image { rgba, w, h } => {
                let Some(img) = fusuma_render::Image::new(u32::from(w), u32::from(h), rgba) else {
                    return Ok(());
                };
                let at = Point::new(
                    rect.x + (rect.w as i32 - i32::from(w)) / 2,
                    rect.y + (rect.h as i32 - i32::from(h)) / 2,
                );
                Self::in_clip(r, rect, |r| r.draw_image(&img, at))
            }
            Kind::Splitter { .. } => {
                // A thin bar; a short grip in the middle that lights while it is held or has the focus.
                let c = &theme.colors;
                let vertical = self.splitter_is_vertical(i);
                let accent = if n.disabled_eff {
                    c.border
                } else if n.active {
                    c.primary_active
                } else if n.hovered {
                    c.primary_hover
                } else {
                    c.border_strong
                };
                let line = Rect::new(
                    if vertical {
                        rect.x
                    } else {
                        px(rect.x, rect.w / 2)
                    },
                    if vertical {
                        px(rect.y, rect.h / 2)
                    } else {
                        rect.y
                    },
                    if vertical {
                        rect.w
                    } else {
                        1.max(theme.border_width)
                    },
                    if vertical {
                        1.max(theme.border_width)
                    } else {
                        rect.h
                    },
                );
                r.fill_rect(line, 0, c.border)?;
                let grip = if vertical {
                    let w = rect.w.min(theme.control_height);
                    Rect::new(
                        px(rect.x, (rect.w - w) / 2),
                        px(rect.y, rect.h / 4),
                        w,
                        rect.h.saturating_sub(rect.h / 2).max(2),
                    )
                } else {
                    let h = rect.h.min(theme.control_height);
                    Rect::new(
                        px(rect.x, rect.w / 4),
                        px(rect.y, (rect.h - h) / 2),
                        rect.w.saturating_sub(rect.w / 2).max(2),
                        h,
                    )
                };
                r.fill_rect(grip, grip.w.min(grip.h) / 2, accent)?;
                self.ring(i, r, theme, theme.radii.sm)
            }
            Kind::Slider { permille } => {
                let thumb = rect.h.min(rect.w);
                let span = rect.w.saturating_sub(thumb);
                let th = theme.track_height.min(rect.h);
                let track = Rect::new(
                    px(rect.x, thumb / 2),
                    px(rect.y, rect.h.saturating_sub(th) / 2),
                    span,
                    th,
                );
                // How far along the track the thumb is, in pixels.
                let along = (u64::from(span) * u64::from(permille) / 1000) as u32;
                let cx = px(track.x, along);
                r.fill_rect(track, th / 2, c.surface_raised)?;
                let filled = along;
                let accent = if n.disabled_eff {
                    c.border
                } else if n.active {
                    c.primary_active
                } else if n.hovered {
                    c.primary_hover
                } else {
                    c.primary
                };
                if filled > 0 {
                    r.fill_rect(Rect::new(track.x, track.y, filled, th), th / 2, accent)?;
                }
                let centre = Point::new(cx, px(rect.y, rect.h / 2));
                r.fill_circle(centre, thumb / 2, c.border_strong)?;
                r.fill_circle(
                    centre,
                    (thumb / 2).saturating_sub(theme.border_width),
                    accent,
                )?;
                self.ring(i, r, theme, thumb / 2)
            }
            Kind::Progress { permille } => {
                let h = rect.h.min(theme.track_height);
                let track = Rect::new(rect.x, px(rect.y, rect.h.saturating_sub(h) / 2), rect.w, h);
                r.fill_rect(track, h / 2, c.surface_raised)?;
                let w = (u64::from(track.w) * u64::from(permille) / 1000) as u32;
                if w > 0 {
                    let color = if n.disabled_eff { c.border } else { c.primary };
                    r.fill_rect(Rect::new(track.x, track.y, w, h), (h / 2).min(w / 2), color)?;
                }
                Ok(())
            }
            Kind::Spinner { phase } => {
                let side = rect.w.min(rect.h);
                let stroke = (side / 8).clamp(2, theme.track_height.max(2) * 2);
                let sq = Rect::new(
                    rect.x + (rect.w.saturating_sub(side) / 2) as i32,
                    rect.y + (rect.h.saturating_sub(side) / 2) as i32,
                    side,
                    side,
                );
                // The track, then a quarter of the ring that goes round.
                crate::gauge::stroke_ring(r, sq, stroke, c.surface_raised)?;
                let color = if n.disabled_eff { c.border } else { c.primary };
                crate::gauge::stroke_arc_from(r, sq, stroke, i32::from(phase), 900, color)
            }
            Kind::Gauge { permille, text } => {
                let side = rect.w.min(rect.h);
                let stroke = (side / 10).clamp(2, theme.track_height.max(2) * 2);
                let sq = Rect::new(
                    rect.x + (rect.w.saturating_sub(side) / 2) as i32,
                    rect.y + (rect.h.saturating_sub(side) / 2) as i32,
                    side,
                    side,
                );
                crate::gauge::stroke_arc(r, sq, stroke, crate::gauge::SWEEP, c.surface_raised)?;
                let color = if n.disabled_eff { c.border } else { c.primary };
                let sweep = (i32::from(permille) * crate::gauge::SWEEP) / 1000;
                crate::gauge::stroke_arc(r, sq, stroke, sweep, color)?;
                if !text.as_str().is_empty() {
                    // The number sits inside the ring: body size, a title on a big gauge.
                    let role = if side > theme.control_height.saturating_mul(4) {
                        crate::theme::TextRole::Title
                    } else {
                        crate::theme::TextRole::Body
                    };
                    let style = theme.text_style(role);
                    let size = r.measure_text(text.as_str(), &style);
                    let at = Point::new(
                        sq.x + (side as i32 - size.w as i32) / 2,
                        sq.y + (side as i32 - size.h as i32) / 2,
                    );
                    r.draw_text(text.as_str(), at, &style)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(crate) fn ring<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
        radius: u32,
    ) -> Result<(), R::Error> {
        if self.focus_ring_shown(i) {
            r.stroke_rect(self.nodes[i].rect, radius, 2, theme.colors.text)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Pointer;
    use crate::focus::Key;
    use crate::theme::TextRole;
    use crate::tree::{CrossAlign, Style};
    use fusuma_render::Probe;
    use fusuma_render_vello_cpu::VelloCpu;

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(240, 320).unwrap()
    }

    struct Form {
        t: Tree<16>,
        check: NodeId,
        radios: [NodeId; 3],
        items: [NodeId; 2],
        bar: NodeId,
    }

    fn form() -> Form {
        let mut t = Tree::<16>::new();
        let root = t
            .column(
                None,
                Style::new().gap(4).padding(8).align(CrossAlign::Stretch),
            )
            .unwrap();
        let check = t.checkbox(Some(root), "Verify", false).unwrap();
        let group = t
            .column(Some(root), Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let radios = [
            t.radio(Some(group), "A", true).unwrap(),
            t.radio(Some(group), "B", false).unwrap(),
            t.radio(Some(group), "C", false).unwrap(),
        ];
        let list = t
            .column(Some(root), Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let items = [
            t.list_item(Some(list), "one", false).unwrap(),
            t.list_item(Some(list), "two", false).unwrap(),
        ];
        let bar = t.progress_bar(Some(root), 250).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 240, 320));
        Form {
            t,
            check,
            radios,
            items,
            bar,
        }
    }

    fn centre<const N: usize>(t: &Tree<N>, id: NodeId) -> Point {
        let r = t.rect(id).unwrap();
        Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
    }

    fn tap<const N: usize>(t: &mut Tree<N>, id: NodeId) {
        let p = centre(t, id);
        t.pointer(Pointer::Down(p));
        t.pointer(Pointer::Up(p));
        t.pointer(Pointer::Leave);
    }

    #[test]
    fn a_tap_toggles_a_checkbox_and_reports_the_new_state() {
        let mut f = form();
        assert!(!f.t.is_checked(f.check));
        tap(&mut f.t, f.check);
        assert!(f.t.is_checked(f.check));
        let a = f.t.pop_action().unwrap();
        assert_eq!((a.source, a.kind), (f.check, ActionKind::Toggled(true)));
        tap(&mut f.t, f.check);
        assert!(!f.t.is_checked(f.check));
        assert_eq!(f.t.pop_action().unwrap().kind, ActionKind::Toggled(false));
    }

    #[test]
    fn choosing_a_radio_button_clears_the_rest_of_its_group_only() {
        let mut f = form();
        assert!(f.t.is_checked(f.radios[0]), "built chosen");
        tap(&mut f.t, f.radios[2]);
        assert_eq!(f.radios.map(|r| f.t.is_checked(r)), [false, false, true]);
        assert_eq!(f.t.pop_action().unwrap().kind, ActionKind::Selected);
        assert!(
            !f.t.is_checked(f.check),
            "another kind in another group is untouched"
        );
        tap(&mut f.t, f.radios[2]);
        assert_eq!(
            f.t.pop_action(),
            None,
            "choosing the chosen one again is no change"
        );
    }

    #[test]
    fn two_groups_do_not_clear_each_other() {
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new()).unwrap();
        let g1 = t.column(Some(root), Style::new()).unwrap();
        let g2 = t.column(Some(root), Style::new()).unwrap();
        let a = t.radio(Some(g1), "a", true).unwrap();
        let b = t.radio(Some(g2), "b", true).unwrap();
        assert!(t.is_checked(a) && t.is_checked(b));
        let c = t.radio(Some(g1), "c", true).unwrap();
        assert!(!t.is_checked(a) && t.is_checked(b) && t.is_checked(c));
    }

    #[test]
    fn a_list_has_one_selected_row() {
        let mut f = form();
        tap(&mut f.t, f.items[1]);
        assert_eq!(f.items.map(|i| f.t.is_checked(i)), [false, true]);
        tap(&mut f.t, f.items[0]);
        assert_eq!(f.items.map(|i| f.t.is_checked(i)), [true, false]);
        assert_eq!(f.t.pop_action().unwrap().source, f.items[1]);
        assert_eq!(f.t.pop_action().unwrap().source, f.items[0]);
    }

    #[test]
    fn setting_state_from_code_sends_no_action() {
        let mut f = form();
        f.t.set_checked(f.check, true);
        f.t.set_checked(f.radios[1], true);
        f.t.set_checked(f.items[0], true);
        assert!(
            f.t.is_checked(f.check) && f.t.is_checked(f.radios[1]) && !f.t.is_checked(f.radios[0])
        );
        assert_eq!(f.t.pop_action(), None);
        f.t.set_checked(f.radios[1], false);
        assert_eq!(
            f.radios.map(|r| f.t.is_checked(r)),
            [false; 3],
            "a group may have no choice"
        );
    }

    #[test]
    fn disabled_widgets_ignore_taps_and_keys() {
        let mut f = form();
        f.t.set_enabled(f.check, false);
        f.t.update(&screen(), &THEME, Rect::new(0, 0, 240, 320));
        tap(&mut f.t, f.check);
        assert!(!f.t.is_checked(f.check));
        assert!(!f.t.focus(f.check), "cannot be focused either");
        assert_eq!(f.t.pop_action(), None);
    }

    #[test]
    fn progress_is_clamped_and_only_progress_has_a_value() {
        let mut f = form();
        assert_eq!(f.t.progress_value(f.bar), Some(250));
        f.t.set_progress(f.bar, 5000);
        assert_eq!(f.t.progress_value(f.bar), Some(1000));
        f.t.set_progress(f.check, 10);
        assert_eq!(f.t.progress_value(f.check), None);
        assert!(!f.t.is_checked(f.bar));
    }

    #[test]
    fn text_of_the_new_widgets_can_be_read_and_changed() {
        let mut f = form();
        assert_eq!(f.t.text(f.check), Some("Verify"));
        f.t.set_text(f.radios[0], "Alpha");
        assert_eq!(f.t.text(f.radios[0]), Some("Alpha"));
        assert_eq!(f.t.text(f.bar), None);
    }

    fn frame(t: &mut Tree<16>, s: &mut VelloCpu) {
        t.update(s, &THEME, Rect::new(0, 0, 240, 320));
        s.clear(THEME.colors.background).unwrap();
        t.paint(s, &THEME).unwrap();
    }

    #[test]
    fn a_checked_box_is_filled_and_an_unchecked_one_is_not() {
        let mut f = form();
        let mut s = screen();
        let r = f.t.rect(f.check).unwrap();
        let inside = (r.x + 3, r.y + (r.h / 2) as i32 - 6);
        frame(&mut f.t, &mut s);
        assert_eq!(s.pixel(inside.0, inside.1).unwrap(), THEME.colors.surface);
        tap(&mut f.t, f.check);
        frame(&mut f.t, &mut s);
        assert_eq!(s.pixel(inside.0, inside.1).unwrap(), THEME.colors.primary);
    }

    #[test]
    fn a_radio_button_shows_its_dot_only_when_chosen() {
        let mut f = form();
        let mut s = screen();
        frame(&mut f.t, &mut s);
        let dot = |t: &Tree<16>, s: &VelloCpu, id| {
            let c = centre_of_indicator(t, id);
            s.pixel(c.x, c.y).unwrap()
        };
        assert_eq!(dot(&f.t, &s, f.radios[0]), THEME.colors.on_primary);
        assert_eq!(dot(&f.t, &s, f.radios[1]), THEME.colors.surface);
    }

    fn centre_of_indicator<const N: usize>(t: &Tree<N>, id: NodeId) -> Point {
        let r = t.rect(id).unwrap();
        Point::new(r.x + (THEME.indicator / 2) as i32, r.y + (r.h / 2) as i32)
    }

    #[test]
    fn the_progress_fill_follows_the_value() {
        let mut f = form();
        let mut s = screen();
        let r = f.t.rect(f.bar).unwrap();
        let y = r.y + (r.h / 2) as i32;
        let at = |frac: u32| r.x + (r.w * frac / 100) as i32;
        frame(&mut f.t, &mut s);
        assert_eq!(s.pixel(at(10), y).unwrap(), THEME.colors.primary);
        assert_eq!(s.pixel(at(60), y).unwrap(), THEME.colors.surface_raised);
        f.t.set_progress(f.bar, 1000);
        frame(&mut f.t, &mut s);
        assert_eq!(s.pixel(at(95), y).unwrap(), THEME.colors.primary);
        f.t.set_progress(f.bar, 0);
        frame(&mut f.t, &mut s);
        assert_eq!(s.pixel(at(5), y).unwrap(), THEME.colors.surface_raised);
    }

    #[test]
    fn a_selected_list_row_is_filled_and_the_focus_ring_follows_the_keys_not_the_touch() {
        let mut f = form();
        let mut s = screen();
        let r = f.t.rect(f.items[0]).unwrap();
        let edge = (r.x + (r.w / 2) as i32, r.y);
        tap(&mut f.t, f.items[0]);
        frame(&mut f.t, &mut s);
        assert_eq!(s.pixel(r.x + 3, r.y + 20).unwrap(), THEME.colors.primary);
        assert_ne!(
            s.pixel(edge.0, edge.1).unwrap(),
            THEME.colors.text,
            "a tap shows no ring"
        );
        f.t.key(Key::Activate);
        f.t.key(Key::Down);
        f.t.key(Key::Up);
        frame(&mut f.t, &mut s);
        assert_eq!(
            s.pixel(edge.0, edge.1).unwrap(),
            THEME.colors.text,
            "a key shows it"
        );
    }

    #[test]
    fn labels_are_untouched_by_the_new_passes() {
        let mut t = Tree::<4>::new();
        let root = t.column(None, Style::new()).unwrap();
        let l = t.label(Some(root), "x", TextRole::Body).unwrap();
        assert!(!t.focus(l));
        t.key(Key::Next);
        assert_eq!(t.focused(), None);
    }

    struct Controls {
        t: Tree<8>,
        sw: NodeId,
        sl: NodeId,
    }

    fn controls() -> Controls {
        let mut t = Tree::<8>::new();
        let root = t
            .column(
                None,
                Style::new().gap(8).padding(8).align(CrossAlign::Stretch),
            )
            .unwrap();
        let sw = t.switch(Some(root), "Wi-Fi", false).unwrap();
        let sl = t.slider(Some(root), 500).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 240, 320));
        Controls { t, sw, sl }
    }

    #[test]
    fn a_switch_toggles_like_a_checkbox_and_reports_it() {
        let mut c = controls();
        assert!(!c.t.is_checked(c.sw));
        tap(&mut c.t, c.sw);
        assert!(c.t.is_checked(c.sw));
        assert_eq!(c.t.pop_action().unwrap().kind, ActionKind::Toggled(true));
        c.t.key(Key::Activate);
        assert!(
            !c.t.is_checked(c.sw),
            "a tap focused it, Enter switches it back"
        );
        assert_eq!(c.t.pop_action().unwrap().kind, ActionKind::Toggled(false));
        c.t.set_checked(c.sw, true);
        assert!(c.t.is_checked(c.sw));
        assert_eq!(c.t.pop_action(), None, "code does not send actions");
        assert_eq!(c.t.text(c.sw), Some("Wi-Fi"));
    }

    #[test]
    fn a_switch_shows_its_state_in_the_track_colour() {
        let mut c = controls();
        let mut s = screen();
        let r = c.t.rect(c.sw).unwrap();
        // The left end of the track: the knob is at the other end when it is on.
        let on_track = (r.x + 2, r.y + (r.h / 2) as i32);
        let paint = |t: &mut Tree<8>, s: &mut VelloCpu| {
            t.update(s, &THEME, Rect::new(0, 0, 240, 320));
            s.clear(THEME.colors.background).unwrap();
            t.paint(s, &THEME).unwrap();
        };
        paint(&mut c.t, &mut s);
        assert_eq!(
            s.pixel(on_track.0, on_track.1).unwrap(),
            THEME.colors.surface_raised
        );
        tap(&mut c.t, c.sw);
        paint(&mut c.t, &mut s);
        let on = s.pixel(on_track.0, on_track.1).unwrap();
        assert!(
            on == THEME.colors.primary || on == THEME.colors.primary_hover,
            "the on track is the accent"
        );
        assert_ne!(on, THEME.colors.surface_raised);
    }

    fn x_at(t: &Tree<8>, id: NodeId, permille: i32) -> i32 {
        let r = t.rect(id).unwrap();
        let thumb = r.h.min(r.w) as i32;
        r.x + thumb / 2 + ((r.w as i32 - thumb) * permille) / 1000
    }

    #[test]
    fn pressing_a_slider_jumps_to_the_pointer_and_dragging_follows() {
        let mut c = controls();
        let y = {
            let r = c.t.rect(c.sl).unwrap();
            r.y + (r.h / 2) as i32
        };
        c.t.pointer(Pointer::Down(Point::new(x_at(&c.t, c.sl, 200), y)));
        // The pointer is in whole pixels, so the value is the nearest one the slider's width allows.
        let near = |v: Option<u16>, want: u16| v.is_some_and(|v| v.abs_diff(want) <= 6);
        assert!(
            near(c.t.slider_value(c.sl), 200),
            "{:?}",
            c.t.slider_value(c.sl)
        );
        assert!(
            matches!(c.t.pop_action().unwrap().kind, ActionKind::Changed(v) if near(Some(v), 200))
        );
        c.t.pointer(Pointer::Move(Point::new(x_at(&c.t, c.sl, 800), y)));
        assert!(near(c.t.slider_value(c.sl), 800));
        c.t.pointer(Pointer::Up(Point::new(x_at(&c.t, c.sl, 800), y)));
        assert!(
            matches!(c.t.pop_action().unwrap().kind, ActionKind::Changed(v) if near(Some(v), 800))
        );
        assert_eq!(c.t.pop_action(), None, "letting go sends nothing more");
        assert!(near(c.t.slider_value(c.sl), 800));
    }

    #[test]
    fn a_dragged_slider_clamps_when_the_pointer_leaves_it() {
        let mut c = controls();
        let r = c.t.rect(c.sl).unwrap();
        let y = r.y + (r.h / 2) as i32;
        c.t.pointer(Pointer::Down(Point::new(x_at(&c.t, c.sl, 500), y)));
        c.t.pointer(Pointer::Move(Point::new(r.x + r.w as i32 + 500, y + 300)));
        assert_eq!(c.t.slider_value(c.sl), Some(1000));
        c.t.pointer(Pointer::Move(Point::new(-400, y)));
        assert_eq!(c.t.slider_value(c.sl), Some(0));
        c.t.pointer(Pointer::Up(Point::new(-400, y)));
        c.t.pointer(Pointer::Move(Point::new(r.x + 50, y)));
        assert_eq!(
            c.t.slider_value(c.sl),
            Some(0),
            "after the release, moving does not drag"
        );
    }

    #[test]
    fn the_arrow_keys_step_a_focused_slider_and_clamp() {
        let mut c = controls();
        c.t.key(Key::Left);
        assert_eq!(
            c.t.slider_value(c.sl),
            Some(500),
            "nothing focused, nothing moves"
        );
        assert!(c.t.focus(c.sl));
        c.t.key(Key::Right);
        assert_eq!(c.t.slider_value(c.sl), Some(550));
        assert_eq!(c.t.pop_action().unwrap().kind, ActionKind::Changed(550));
        c.t.key(Key::Left);
        c.t.key(Key::Left);
        assert_eq!(c.t.slider_value(c.sl), Some(450));
        c.t.set_slider(c.sl, 990);
        c.t.key(Key::Right);
        assert_eq!(c.t.slider_value(c.sl), Some(1000), "clamped");
        let _ = (c.t.pop_action(), c.t.pop_action(), c.t.pop_action());
        c.t.key(Key::Right);
        assert_eq!(c.t.pop_action(), None, "no change, no action");
        c.t.set_slider(c.sl, 0);
        c.t.key(Key::Left);
        assert_eq!(c.t.slider_value(c.sl), Some(0));
        assert!(c.t.focus(c.sw));
        c.t.key(Key::Right);
        assert_eq!(
            c.t.slider_value(c.sl),
            Some(0),
            "only a slider takes the arrows"
        );
    }

    #[test]
    fn code_sets_a_slider_without_an_action_and_only_sliders_have_a_value() {
        let mut c = controls();
        c.t.set_slider(c.sl, 5000);
        assert_eq!(c.t.slider_value(c.sl), Some(1000));
        assert_eq!(c.t.pop_action(), None);
        c.t.set_slider(c.sw, 10);
        assert_eq!(c.t.slider_value(c.sw), None);
    }

    #[test]
    fn a_disabled_slider_ignores_the_pointer_and_the_keys() {
        let mut c = controls();
        c.t.set_enabled(c.sl, false);
        c.t.update(&screen(), &THEME, Rect::new(0, 0, 240, 320));
        let r = c.t.rect(c.sl).unwrap();
        let y = r.y + (r.h / 2) as i32;
        c.t.pointer(Pointer::Down(Point::new(x_at(&c.t, c.sl, 900), y)));
        c.t.pointer(Pointer::Up(Point::new(x_at(&c.t, c.sl, 900), y)));
        assert_eq!(c.t.slider_value(c.sl), Some(500));
        assert!(!c.t.focus(c.sl));
    }

    #[test]
    fn the_slider_thumb_is_drawn_at_its_value() {
        let mut c = controls();
        let mut s = screen();
        let r = c.t.rect(c.sl).unwrap();
        let y = r.y + (r.h / 2) as i32;
        c.t.update(&s, &THEME, Rect::new(0, 0, 240, 320));
        s.clear(THEME.colors.background).unwrap();
        c.t.paint(&mut s, &THEME).unwrap();
        let filled = s.pixel(x_at(&c.t, c.sl, 250), y).unwrap();
        let empty = s.pixel(x_at(&c.t, c.sl, 750), y).unwrap();
        assert_eq!(filled, THEME.colors.primary);
        assert_eq!(empty, THEME.colors.surface_raised);
        assert_eq!(
            s.pixel(x_at(&c.t, c.sl, 500), y).unwrap(),
            THEME.colors.primary,
            "the thumb"
        );
    }

    #[test]
    fn moving_a_slider_marks_changed_only_the_stretch_the_thumb_crossed() {
        let mut c = controls();
        let r = c.t.rect(c.sl).unwrap();
        let y = r.y + (r.h / 2) as i32;
        c.t.pointer(Pointer::Down(Point::new(x_at(&c.t, c.sl, 100), y)));
        let _ = c.t.take_damage(); // the press itself recolours the whole slider
        c.t.pointer(Pointer::Move(Point::new(x_at(&c.t, c.sl, 200), y)));
        let d = c.t.take_damage().expect("the thumb moved");
        let (a, b) = (x_at(&c.t, c.sl, 100), x_at(&c.t, c.sl, 200));
        assert!(
            d.x <= a && d.right() >= b,
            "covers the old and the new thumb: {d:?}"
        );
        assert!(
            d.w < r.w / 2 && d.h <= r.h,
            "but not the whole slider: {d:?} of {r:?}"
        );
        // The picture is still right: a partial redraw equals a full one.
        let (mut part, mut full) = (screen(), screen());
        for s in [&mut part, &mut full] {
            c.t.update(s, &THEME, Rect::new(0, 0, 240, 320));
            s.clear(THEME.colors.background).unwrap();
        }
        c.t.paint(&mut full, &THEME).unwrap();
        c.t.invalidate();
        let region = c.t.take_damage().unwrap();
        c.t.paint_region(&mut part, &THEME, region).unwrap();
        for x in (0..240).step_by(3) {
            for yy in (0..320).step_by(3) {
                assert_eq!(part.pixel(x, yy), full.pixel(x, yy), "({x}, {yy})");
            }
        }
    }

    #[test]
    fn a_label_in_a_stretching_column_keeps_its_text_width_and_a_fill_width_label_stretches() {
        let mut t = Tree::<4>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let text = t.label(Some(root), "Hi", TextRole::Body).unwrap();
        let wide = t.label(Some(root), "Hi", TextRole::Body).unwrap();
        t.set_style(wide, Style::new().width(crate::tree::Length::Fill(1)));
        t.update(&screen(), &THEME, Rect::new(0, 0, 200, 100));
        assert!(t.rect(text).unwrap().w < 50, "{:?}", t.rect(text));
        assert_eq!(t.rect(wide).unwrap().w, 200);
    }

    fn split(vertical: bool, permille: u16) -> (Tree<16>, [NodeId; 3]) {
        let mut t = Tree::<16>::new();
        let root = if vertical {
            t.column(None, Style::new().padding(10))
        } else {
            t.row(None, Style::new().padding(10))
        }
        .unwrap();
        let weight = |p: u16| {
            let l = crate::tree::Length::Fill(p);
            if vertical {
                Style::new().height(l).width(crate::tree::Length::Fill(1))
            } else {
                Style::new().width(l).height(crate::tree::Length::Fill(1))
            }
        };
        let a = t.row(Some(root), weight(permille)).unwrap();
        let bar = t.splitter(Some(root), permille).unwrap();
        let b = t.row(Some(root), weight(1000 - permille)).unwrap();
        t.update(&screen(), &THEME, Rect::new(0, 0, 220, 300));
        (t, [a, bar, b])
    }

    /// What an app does with the splitter's value: the panes' weights follow it.
    fn follow(t: &mut Tree<16>, [a, bar, b]: [NodeId; 3], vertical: bool) {
        let v = t.slider_value(bar).unwrap();
        let w = |p: u16| {
            let l = crate::tree::Length::Fill(p);
            if vertical {
                Style::new().height(l).width(crate::tree::Length::Fill(1))
            } else {
                Style::new().width(l).height(crate::tree::Length::Fill(1))
            }
        };
        t.set_style(a, w(v));
        t.set_style(b, w(1000 - v));
        t.update(&screen(), &THEME, Rect::new(0, 0, 220, 300));
    }

    fn drag_to(t: &mut Tree<16>, from: Point, to: Point) {
        t.pointer(Pointer::Move(from));
        t.pointer(Pointer::Down(from));
        for k in 1..=4 {
            let p = Point::new(
                from.x + (to.x - from.x) * k / 4,
                from.y + (to.y - from.y) * k / 4,
            );
            t.pointer(Pointer::Move(p));
        }
        t.pointer(Pointer::Up(to));
        t.pointer(Pointer::Leave);
    }

    fn changes(t: &mut Tree<16>, id: NodeId) -> std::vec::Vec<u16> {
        std::iter::from_fn(|| t.pop_action())
            .filter(|a| a.source == id)
            .filter_map(|a| match a.kind {
                crate::tree::ActionKind::Changed(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_splitter_between_two_panes_reports_where_it_is_dragged_along_its_parent() {
        let (mut t, ids) = split(false, 300);
        let [a, bar, b] = ids;
        let (ra, rb, rbar) = (t.rect(a).unwrap(), t.rect(b).unwrap(), t.rect(bar).unwrap());
        assert!(ra.w < rb.w && ra.right() <= rbar.x && rbar.right() <= rb.x);
        assert_eq!(rbar.h, 280, "a bar in a row is as tall as the row");
        assert_eq!(t.slider_value(bar), Some(300));
        let from = Point::new(rbar.x + 1, 150);
        drag_to(&mut t, from, Point::new(110, 150));
        let got = changes(&mut t, bar);
        assert_eq!(got.last(), Some(&500), "{got:?}");
        assert!(
            got.windows(2).all(|w| w[0] != w[1]),
            "each change once: {got:?}"
        );
        assert_eq!(t.slider_value(bar), Some(500));
        follow(&mut t, ids, false);
        assert!(
            t.rect(a).unwrap().w > 90 && t.rect(a).unwrap().w < 110,
            "the panes followed"
        );
        let at = t.rect(bar).unwrap().x + 1;
        drag_to(&mut t, Point::new(at, 150), Point::new(900, 150));
        assert_eq!(changes(&mut t, bar).last(), Some(&1000));
        follow(&mut t, ids, false);
        let at = t.rect(bar).unwrap().x + 1;
        drag_to(&mut t, Point::new(at, 150), Point::new(-50, 150));
        assert_eq!(t.slider_value(bar), Some(0));
        let _ = changes(&mut t, bar);
        assert_eq!(
            t.pop_action(),
            None,
            "nothing but the bar reported anything"
        );
    }

    #[test]
    fn a_splitter_in_a_column_goes_by_height_and_arrow_keys_move_it_in_steps() {
        let (mut t, [_, bar, _]) = split(true, 500);
        let r = t.rect(bar).unwrap();
        assert_eq!(r.w, 200, "a bar in a column is as wide as the column");
        let from = Point::new(150, r.y + 1);
        drag_to(&mut t, from, Point::new(150, 103));
        let v = t.slider_value(bar).unwrap();
        assert!((330..=336).contains(&v), "{v}");
        let _ = changes(&mut t, bar);
        t.focus(bar);
        t.key(Key::Right);
        assert_eq!(
            t.slider_value(bar),
            Some((v + crate::focus::SLIDER_STEP).min(1000))
        );
        t.key(Key::Left);
        t.key(Key::Left);
        assert_eq!(
            t.slider_value(bar),
            Some(v.saturating_sub(crate::focus::SLIDER_STEP))
        );
        assert!(!changes(&mut t, bar).is_empty());
        let s = t.semantics().find(|s| s.id == bar).unwrap();
        assert_eq!(s.role, crate::semantic::Role::Slider);
        assert_eq!(s.value, t.slider_value(bar).map(i32::from));
        t.set_enabled(bar, false);
        t.update(&screen(), &THEME, Rect::new(0, 0, 220, 300));
        let before = t.slider_value(bar);
        drag_to(&mut t, Point::new(150, r.y + 1), Point::new(150, 200));
        assert_eq!(t.slider_value(bar), before);
    }

    static RED_BLUE: [u8; 4 * 4 * 4] = {
        let mut p = [0u8; 64];
        let mut i = 0;
        while i < 16 {
            // The left two columns red, the right two blue, all opaque; the top-left pixel transparent.
            let (x, y) = (i % 4, i / 4);
            p[i * 4] = if x < 2 { 255 } else { 0 };
            p[i * 4 + 2] = if x < 2 { 0 } else { 255 };
            p[i * 4 + 3] = if x == 0 && y == 0 { 0 } else { 255 };
            i += 1;
        }
        p
    };

    #[test]
    fn an_image_is_its_own_size_drawn_centred_in_its_box_and_cut_to_it() {
        use fusuma_render::{Color, Renderer};
        let mut t = Tree::<8>::new();
        let root = t.column(None, Style::new().padding(10)).unwrap();
        let img = t.image(Some(root), 4, 4, &RED_BLUE).unwrap();
        let mut r = screen();
        let bounds = Rect::new(0, 0, 100, 100);
        t.update(&r, &THEME, bounds);
        assert_eq!(
            t.rect(img).map(|r| (r.w, r.h)),
            Some((4, 4)),
            "as big as the picture"
        );
        r.clear(Color::WHITE).unwrap();
        t.paint(&mut r, &THEME).unwrap();
        let at = t.rect(img).unwrap();
        assert_eq!(r.pixel(at.x + 1, at.y + 1), Some(Color::rgb(255, 0, 0)));
        assert_eq!(r.pixel(at.x + 3, at.y + 2), Some(Color::rgb(0, 0, 255)));
        assert_eq!(
            r.pixel(at.x, at.y),
            Some(Color::WHITE),
            "a transparent pixel shows what is behind"
        );
        assert_eq!(
            r.pixel(at.x - 1, at.y + 1),
            Some(Color::WHITE),
            "nothing outside it"
        );
        // In a bigger box it sits in the middle; in a smaller one it is cut.
        t.set_style(
            img,
            Style::new()
                .width(crate::tree::Length::Fixed(20))
                .height(crate::tree::Length::Fixed(20)),
        );
        t.update(&r, &THEME, bounds);
        let b = t.rect(img).unwrap();
        r.clear(Color::WHITE).unwrap();
        t.paint(&mut r, &THEME).unwrap();
        assert_eq!(
            r.pixel(b.x + 10 - 2 + 1, b.y + 10 - 2 + 1),
            Some(Color::rgb(255, 0, 0)),
            "in the middle of a 20 box"
        );
        assert_eq!(r.pixel(b.x + 1, b.y + 1), Some(Color::WHITE));
        t.set_style(
            img,
            Style::new()
                .width(crate::tree::Length::Fixed(2))
                .height(crate::tree::Length::Fixed(2)),
        );
        t.update(&r, &THEME, bounds);
        let b = t.rect(img).unwrap();
        r.clear(Color::WHITE).unwrap();
        t.paint(&mut r, &THEME).unwrap();
        assert_eq!(
            r.pixel(b.x + 3, b.y),
            Some(Color::WHITE),
            "cut to a 2 box: the blue half is outside it"
        );
        assert_eq!(
            t.semantics()
                .filter(|s| s.role == crate::semantic::Role::Image)
                .count(),
            1
        );
    }

    #[test]
    fn an_image_with_the_wrong_number_of_bytes_or_too_big_is_refused() {
        let mut t = Tree::<4>::new();
        assert_eq!(
            t.image(None, 4, 4, &RED_BLUE[..60]),
            Err(crate::tree::BuildError::BadImage)
        );
        assert_eq!(
            t.image(None, 5, 4, &RED_BLUE),
            Err(crate::tree::BuildError::BadImage)
        );
        static TALL: [u8; 4] = [0; 4];
        assert_eq!(
            t.image(None, 70_000, 1, &TALL),
            Err(crate::tree::BuildError::BadImage)
        );
        assert!(t.image(None, 1, 1, &TALL).is_ok());
        assert_eq!(t.len(), 1, "a refused image leaves no node behind");
    }

    fn chart_pic(style: crate::tree::ChartStyle, samples: &[u8]) -> (VelloCpu, Rect) {
        use fusuma_render::Renderer;
        let mut t = Tree::<4>::new();
        let root = t.column(None, Style::new().padding(10)).unwrap();
        let ch = t.chart(Some(root), style, samples).unwrap();
        t.set_style(
            ch,
            Style::new()
                .width(crate::tree::Length::Fixed(100))
                .height(crate::tree::Length::Fixed(60)),
        );
        let mut r = screen();
        t.update(&r, &THEME, Rect::new(0, 0, 200, 200));
        r.clear(THEME.colors.background).unwrap();
        t.paint(&mut r, &THEME).unwrap();
        let rect = t.rect(ch).unwrap();
        (r, rect)
    }

    fn lit(r: &VelloCpu, g: Rect, x: i32) -> std::vec::Vec<i32> {
        (g.y..g.bottom())
            .filter(|&y| r.pixel(x, y) == Some(THEME.colors.primary))
            .collect()
    }

    #[test]
    fn bars_are_as_tall_as_their_sample_and_leave_gaps_between() {
        use crate::tree::ChartStyle;
        // Four bars at 255, 128, 64 and 0 of the height: the plot is 60 - 4 = 56 high.
        let (r, g) = chart_pic(ChartStyle::Bars, &[255, 128, 64, 0]);
        let slot = (100 - 4) / 4;
        let mid = |k: i32| g.x + 2 + slot * k + slot / 2;
        let heights: std::vec::Vec<usize> = (0..4).map(|k| lit(&r, g, mid(k)).len()).collect();
        assert!((54..=56).contains(&heights[0]), "{heights:?}");
        assert!((26..=29).contains(&heights[1]), "{heights:?}");
        assert!((13..=15).contains(&heights[2]), "{heights:?}");
        assert_eq!(heights[3], 0, "{heights:?}");
        // The bars are on the baseline, and a gap between two bars is empty.
        let ys = lit(&r, g, mid(0));
        assert!(*ys.last().unwrap() >= g.bottom() - 4, "{ys:?}");
        assert!(lit(&r, g, g.x + 2 + slot).is_empty() || lit(&r, g, g.x + 2 + slot - 1).is_empty());
    }

    #[test]
    fn a_line_goes_through_its_samples_from_edge_to_edge_and_an_area_fills_under_it() {
        use crate::tree::ChartStyle;
        let (r, g) = chart_pic(ChartStyle::Line, &[0, 255, 0]);
        // The three samples: bottom-left, top-middle, bottom-right; the line is lit at each (a dot), and the picture has no fill.
        let left = lit(&r, g, g.x + 3);
        let top = lit(&r, g, g.x + 2 + 48);
        let right = lit(&r, g, g.x + 2 + 95);
        assert!(!left.is_empty() && !top.is_empty() && !right.is_empty());
        assert!(
            top[0] < left[0] - 40,
            "the middle sample is high: {top:?} {left:?}"
        );
        // Between them it rises: a column a quarter along is lit higher than the left end.
        let q = lit(&r, g, g.x + 2 + 24);
        assert!(q.iter().any(|y| *y < left[0] - 10 && *y > top[0]), "{q:?}");
        // Under the line there is nothing in a line chart; in an area chart the column under the peak is filled to the baseline.
        let under = lit(&r, g, g.x + 2 + 48).len();
        assert!(under < 12, "just the line: {under}");
        let (ra, ga) = chart_pic(ChartStyle::Area, &[0, 255, 0]);
        let ai = ra.pixel(ga.x + 2 + 48, ga.bottom() - 8).unwrap();
        assert_ne!(ai, THEME.colors.background, "filled under the peak");
        assert_ne!(ai, THEME.colors.primary, "in a tint, not the line's colour");
    }

    #[test]
    fn a_chart_keeps_its_first_samples_and_repaints_only_when_they_change() {
        use crate::tree::{CHART_MAX, ChartStyle};
        let mut t = Tree::<4>::new();
        let many: std::vec::Vec<u8> = (0..100).map(|i| i as u8).collect();
        let c = t.chart(None, ChartStyle::Line, &many).unwrap();
        assert_eq!(t.chart_samples(c).map(<[u8]>::len), Some(CHART_MAX));
        assert_eq!(t.chart_samples(c).unwrap()[31], 31);
        let mut r = screen();
        let b = Rect::new(0, 0, 200, 100);
        t.update(&r, &THEME, b);
        t.paint_frame(&mut r, &THEME, b).unwrap();
        assert!(!t.needs_frame());
        t.set_chart(c, ChartStyle::Line, &many);
        assert!(!t.needs_frame(), "the same data is a no-op");
        t.set_chart(c, ChartStyle::Bars, &many);
        assert!(t.needs_frame(), "another style is a change");
        t.update(&r, &THEME, b);
        t.paint_frame(&mut r, &THEME, b).unwrap();
        t.set_chart(c, ChartStyle::Bars, &[1, 2, 3]);
        assert_eq!(t.chart_samples(c), Some(&[1u8, 2, 3][..]));
        assert!(t.needs_frame());
        // No samples, one sample and a tiny box do not fail.
        t.set_chart(c, ChartStyle::Line, &[]);
        t.update(&r, &THEME, b);
        t.paint(&mut r, &THEME).unwrap();
        t.set_chart(c, ChartStyle::Area, &[200]);
        t.update(&r, &THEME, b);
        t.paint(&mut r, &THEME).unwrap();
        t.set_style(
            c,
            Style::new()
                .width(crate::tree::Length::Fixed(3))
                .height(crate::tree::Length::Fixed(3)),
        );
        t.update(&r, &THEME, b);
        t.paint(&mut r, &THEME).unwrap();
        assert_eq!(
            t.semantics()
                .filter(|s| s.role == crate::semantic::Role::Image)
                .count(),
            1
        );
        // Not a chart: ignored.
        assert_eq!(Tree::<2>::new().chart_samples(c), None);
    }

    #[test]
    fn a_line_chart_draws_on_the_integer_renderer_with_the_most_samples() {
        use crate::tree::ChartStyle;
        use fusuma_render_integer::{Embedded, Frame};
        let samples: std::vec::Vec<u8> = (0..32).map(|i| ((i * 37) % 256) as u8).collect();
        for style in [ChartStyle::Line, ChartStyle::Area, ChartStyle::Bars] {
            let mut t = Tree::<4>::new();
            let root = t.column(None, Style::new().padding(4)).unwrap();
            t.chart(Some(root), style, &samples).unwrap();
            let mut r = Embedded::new(Frame::new(200, 120, std::vec![0u16; 200 * 120]).unwrap());
            t.update(&r, &THEME, Rect::new(0, 0, 200, 120));
            t.paint(&mut r, &THEME)
                .unwrap_or_else(|e| panic!("{style:?}: {e:?}"));
        }
    }
}
