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
            Kind::Checkbox { checked, .. } => {
                self.set_flag(i, !checked);
                Some(ActionKind::Toggled(!checked))
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
            Kind::Checkbox { text: t, .. } | Kind::Radio { text: t, .. } => {
                let t = text(t.as_str());
                let w = theme
                    .indicator
                    .saturating_add(theme.spacing.md)
                    .saturating_add(t.w);
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
}
