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

impl<const N: usize> Tree<N> {
    /// What pressing node `i` does, from the pointer or the keyboard: the one place the widgets'
    /// behaviour lives.
    pub(crate) fn activate(&mut self, i: usize) {
        if self.nodes[i].disabled_eff {
            return;
        }
        let source = self.id_of(i);
        let kind = match self.nodes[i].kind {
            Kind::Button { .. } => Some(ActionKind::Clicked),
            Kind::Checkbox { checked: on, .. } | Kind::Switch { on, .. } => {
                self.set_flag(i, !on);
                Some(ActionKind::Toggled(!on))
            }
            Kind::Radio { checked, .. } if !checked => {
                self.choose(i);
                Some(ActionKind::Selected)
            }
            Kind::ListItem { selected, .. } if !selected => {
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
            Kind::ListItem { selected, .. } => selected,
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
            Kind::Radio { .. } | Kind::ListItem { .. }
        ) && on
        {
            self.choose(i);
        } else {
            self.set_flag(i, on);
        }
    }

    pub(crate) const fn is_slider(&self, i: usize) -> bool {
        matches!(self.nodes[i].kind, Kind::Slider { .. })
    }

    /// Moves slider `i` to `to` thousandths; sends [`ActionKind::Changed`] if that is a change.
    pub(crate) fn move_slider(&mut self, i: usize, to: u16) {
        let to = to.min(1000);
        if let Kind::Slider { permille } = &mut self.nodes[i].kind
            && *permille != to
        {
            *permille = to;
            let source = self.id_of(i);
            self.request(i, Flags::PAINT);
            self.push_action(Action {
                source,
                kind: ActionKind::Changed(to),
            });
        }
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
            Kind::Slider { permille } => Some(permille),
            _ => None,
        }
    }

    /// Sets a slider (0 to 1000; more is 1000). It sends no action: actions report what the user did.
    pub fn set_slider(&mut self, id: NodeId, permille: u16) {
        let Some(i) = self.slot(id) else { return };
        let permille = permille.min(1000);
        if let Kind::Slider { permille: p } = &mut self.nodes[i].kind
            && *p != permille
        {
            *p = permille;
            self.request(i, Flags::PAINT);
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
    pub(crate) fn measure_widget<R: Renderer>(&self, i: usize, r: &R, theme: &Theme) -> Size {
        let body = TextStyle::new(theme.text.body, Color::BLACK);
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
                Size::new(w, t.h.max(theme.indicator).max(theme.control_height))
            }
            Kind::ListItem { text: t, .. } => {
                let t = text(t.as_str());
                Size::new(
                    t.w.saturating_add(theme.spacing.md.saturating_mul(2)),
                    t.h.max(theme.control_height),
                )
            }
            Kind::Progress { .. } => Size::new(0, theme.track_height),
            Kind::Slider { .. } => Size::new(
                theme.indicator.saturating_mul(6),
                theme.indicator.max(theme.track_height),
            ),
            _ => Size::new(0, 0),
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
                let top = rect.y + (rect.h.saturating_sub(ind) / 2) as i32;
                let bx = Rect::new(rect.x, top, ind, ind);
                let bw = theme.border_width;
                if round {
                    let centre = Point::new(bx.x + (ind / 2) as i32, bx.y + (ind / 2) as i32);
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
                                bx.x + (ind as i32 * fx) / 100,
                                bx.y + (ind as i32 * fy) / 100,
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
                let style = TextStyle::new(theme.text.body, look.text);
                let size = r.measure_text(text.as_str(), &style);
                let at = Point::new(
                    rect.x + (ind + theme.spacing.md) as i32,
                    centred(rect, size).y,
                );
                Self::in_clip(r, rect, |r| r.draw_text(text.as_str(), at, &style))?;
                self.ring(i, r, theme, theme.radii.sm)
            }
            Kind::ListItem { text, selected } => {
                let (fill, label) = if n.disabled_eff {
                    (selected.then_some(c.border), c.text_disabled)
                } else if selected {
                    (Some(c.primary), c.on_primary)
                } else if n.active {
                    (Some(c.border), c.text)
                } else if n.hovered {
                    (Some(c.surface_raised), c.text)
                } else {
                    (None, c.text)
                };
                if let Some(fill) = fill {
                    r.fill_rect(rect, theme.radii.sm, fill)?;
                }
                let style = TextStyle::new(theme.text.body, label);
                let size = r.measure_text(text.as_str(), &style);
                let at = Point::new(rect.x + theme.spacing.md as i32, centred(rect, size).y);
                Self::in_clip(r, rect, |r| r.draw_text(text.as_str(), at, &style))?;
                self.ring(i, r, theme, theme.radii.sm)
            }
            Kind::Switch { text, on } => {
                let look = self.mark_look(i, theme, on);
                let h = theme.indicator;
                let w = h.saturating_mul(2);
                let top = rect.y + (rect.h.saturating_sub(h) / 2) as i32;
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
                let d = h.saturating_sub(2 * pad);
                let knob_x = if on {
                    track.x + (w - pad - d) as i32
                } else {
                    track.x + pad as i32
                };
                let knob = Point::new(knob_x + (d / 2) as i32, top + (h / 2) as i32);
                let knob_color = if on { look.glyph } else { look.border };
                r.fill_circle(knob, d / 2, knob_color)?;
                let style = TextStyle::new(theme.text.body, look.text);
                let size = r.measure_text(text.as_str(), &style);
                let at = Point::new(
                    rect.x + (w + theme.spacing.md) as i32,
                    centred(rect, size).y,
                );
                Self::in_clip(r, rect, |r| r.draw_text(text.as_str(), at, &style))?;
                self.ring(i, r, theme, theme.radii.sm)
            }
            Kind::Slider { permille } => {
                let thumb = rect.h.min(rect.w);
                let span = rect.w.saturating_sub(thumb);
                let th = theme.track_height.min(rect.h);
                let track = Rect::new(
                    rect.x + (thumb / 2) as i32,
                    rect.y + (rect.h.saturating_sub(th) / 2) as i32,
                    span,
                    th,
                );
                let cx = track.x + (u64::from(span) * u64::from(permille) / 1000) as i32;
                r.fill_rect(track, th / 2, c.surface_raised)?;
                let filled = (cx - track.x).max(0) as u32;
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
                let centre = Point::new(cx, rect.y + (rect.h / 2) as i32);
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
                let track = Rect::new(
                    rect.x,
                    rect.y + (rect.h.saturating_sub(h) / 2) as i32,
                    rect.w,
                    h,
                );
                r.fill_rect(track, h / 2, c.surface_raised)?;
                let w = (u64::from(track.w) * u64::from(permille) / 1000) as u32;
                if w > 0 {
                    let color = if n.disabled_eff { c.border } else { c.primary };
                    r.fill_rect(Rect::new(track.x, track.y, w, h), (h / 2).min(w / 2), color)?;
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

    const THEME: Theme = Theme::DARK;

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
}
