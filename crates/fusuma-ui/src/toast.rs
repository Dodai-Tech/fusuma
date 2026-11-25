//! Toasts: a short message at the bottom of the screen that fades and slides in, stays for a while and fades out.
//! One toast is one node of the tree; showing a new message in it replaces the old one. It is out of the flow, drawn
//! over everything (dialogs included), takes no presses and never moves the focus.

use fusuma_anim::{Easing, Fx};
use fusuma_render::{Color, Point, Rect, Renderer, TextStyle};

use crate::paint::centred;
use crate::theme::Theme;
use crate::tree::{BuildError, Flags, Kind, NodeId, SmallText, Style, Tree};

/// How far a toast slides while it comes in, in pixels.
const SLIDE: i32 = 12;

/// Where a toast is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// Not shown, and taking no room.
    Hidden,
    /// Fading and sliding in.
    Entering,
    /// Fully shown.
    Holding,
    /// Fading out.
    Leaving,
}

/// How opaque a toast is, 0 to 255, and how many pixels lower than its place it sits.
fn look(phase: Phase, t: u32, theme: &Theme) -> (u8, i32) {
    let ms = theme.motion.normal_ms.max(1);
    let eased = |t: u32| Easing::CubicOut.apply(Fx::from_ratio(t.min(ms) as i32, ms as i32));
    let (open, _) = match phase {
        Phase::Hidden => (Fx::ZERO, 0),
        Phase::Entering => (eased(t), 0),
        Phase::Holding => (Fx::ONE, 0),
        Phase::Leaving => (Fx::ONE - eased(t), 0),
    };
    let alpha = (open * Fx::from_int(255)).round().clamp(0, 255) as u8;
    let slide = ((Fx::ONE - open) * Fx::from_int(SLIDE))
        .round()
        .clamp(0, SLIDE);
    (alpha, slide)
}

impl<const N: usize> Tree<N> {
    /// Adds a toast under `parent` (usually the root), hidden. Show a message with [`show_toast`](Self::show_toast).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn toast(&mut self, parent: Option<NodeId>) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Toast {
                text: SmallText::EMPTY,
                phase: Phase::Hidden,
                t: 0,
                hold: 0,
            },
            Style::DEFAULT,
        )
    }

    /// Shows `text` in toast `id` for `hold_ms` once it is in: it replaces what the toast showed, starting its entrance
    /// again.
    pub fn show_toast(&mut self, id: NodeId, text: &str, hold_ms: u32) {
        let Some(i) = self.slot(id) else { return };
        if let Kind::Toast {
            text: tx,
            phase,
            t,
            hold,
        } = &mut self.nodes[i].kind
        {
            tx.set(text);
            (*phase, *t, *hold) = (Phase::Entering, 0, hold_ms);
        } else {
            return;
        }
        self.request(i, Flags::LAYOUT | Flags::PAINT | Flags::ANIM);
    }

    /// Starts the exit of toast `id` now, if it is showing.
    pub fn hide_toast(&mut self, id: NodeId) {
        let Some(i) = self.slot(id) else { return };
        if let Kind::Toast { phase, t, .. } = &mut self.nodes[i].kind
            && matches!(*phase, Phase::Entering | Phase::Holding)
        {
            (*phase, *t) = (Phase::Leaving, 0);
            self.request(i, Flags::PAINT | Flags::ANIM);
        }
    }

    /// Whether toast `id` is on the screen (coming in, shown or going out).
    #[must_use]
    pub fn is_toast_shown(&self, id: NodeId) -> bool {
        self.slot(id).is_some_and(
            |i| matches!(self.nodes[i].kind, Kind::Toast { phase, .. } if phase != Phase::Hidden),
        )
    }

    /// Where toast `i` sits: centred at the bottom, `margin` from the edges, or nowhere while hidden.
    pub(crate) fn toast_rect(&self, i: usize, bounds: Rect, margin: u32) -> Rect {
        let Kind::Toast { phase, .. } = self.nodes[i].kind else {
            return Rect::new(bounds.x, bounds.y, 0, 0);
        };
        if phase == Phase::Hidden {
            return Rect::new(bounds.x, bounds.y, 0, 0);
        }
        let s = self.nodes[i].size;
        let w = s.w.min(bounds.w.saturating_sub(margin.saturating_mul(2)));
        let h = s.h.min(bounds.h);
        let x = i64::from(bounds.x) + (i64::from(bounds.w) - i64::from(w)) / 2;
        let y = i64::from(bounds.bottom()) - i64::from(margin) - i64::from(h);
        let c = |v: i64| v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        Rect::new(c(x), c(y.max(i64::from(bounds.y))), w, h)
    }

    /// Moves the toasts' clocks by `dt_ms`. True while one is showing.
    pub(crate) fn advance_toasts(&mut self, theme: &Theme, dt_ms: u32) -> bool {
        let ms = theme.motion.normal_ms;
        let mut running = false;
        for i in 0..N {
            if !self.nodes[i].alive {
                continue;
            }
            let Kind::Toast { phase, t, hold, .. } = &mut self.nodes[i].kind else {
                continue;
            };
            if *phase == Phase::Hidden {
                continue;
            }
            let before = (*phase, look(*phase, *t, theme));
            *t = t.saturating_add(dt_ms);
            // A long frame may carry it across more than one phase.
            loop {
                let len = match *phase {
                    Phase::Entering | Phase::Leaving => ms,
                    Phase::Holding => *hold,
                    Phase::Hidden => break,
                };
                if *t < len {
                    break;
                }
                *t -= len;
                *phase = match *phase {
                    Phase::Entering => Phase::Holding,
                    Phase::Holding => Phase::Leaving,
                    _ => Phase::Hidden,
                };
            }
            let after = (*phase, look(*phase, *t, theme));
            let hidden = *phase == Phase::Hidden;
            let mut flags = 0;
            if before != after {
                flags |= Flags::PAINT;
            }
            if hidden || before.0 != after.0 {
                flags |= Flags::LAYOUT;
            }
            if !hidden {
                self.nodes[i].own.insert(Flags::ANIM);
                running = true;
            } else {
                self.nodes[i].own.remove(Flags::ANIM);
            }
            if flags != 0 {
                // The toast moves while it comes in: all of its travel needs drawing, not only its place.
                let rect = self.toast_travel(i);
                self.add_damage(rect);
                self.request_quiet(i, Flags::PAINT);
                if flags & Flags::LAYOUT != 0 {
                    self.request_quiet(i, Flags::LAYOUT);
                }
            }
        }
        running
    }

    /// The place of toast `i` and the pixels it slides through.
    fn toast_travel(&self, i: usize) -> Rect {
        let r = self.nodes[i].rect;
        Rect::new(r.x, r.y, r.w, r.h.saturating_add(SLIDE as u32))
    }

    /// Draws the toasts that are showing, over everything. `region` limits the drawing to what overlaps it.
    pub(crate) fn paint_toasts<R: Renderer>(
        &self,
        r: &mut R,
        theme: &Theme,
        region: Option<Rect>,
    ) -> Result<(), R::Error> {
        let alpha_ok = r.capabilities().alpha;
        for i in 0..N {
            let n = &self.nodes[i];
            let Kind::Toast { text, phase, t, .. } = n.kind else {
                continue;
            };
            if !n.alive || phase == Phase::Hidden || n.rect.is_empty() {
                continue;
            }
            let travel = self.toast_travel(i);
            if region.is_some_and(|g| {
                let o = travel.intersect(&g);
                o.w == 0 || o.h == 0
            }) {
                continue;
            }
            let (alpha, slide) = look(phase, t, theme);
            // Without alpha a half-shown toast is either there or not.
            let alpha = if alpha_ok {
                alpha
            } else if alpha >= 128 {
                255
            } else {
                continue;
            };
            let rect = Rect::new(n.rect.x, n.rect.y.saturating_add(slide), n.rect.w, n.rect.h);
            let with = |c: Color| {
                Color::rgba(
                    c.r,
                    c.g,
                    c.b,
                    (u32::from(c.a) * u32::from(alpha) / 255) as u8,
                )
            };
            let look = theme.panel(crate::theme::PanelKind::Toast);
            r.fill_rect(rect, look.radius, with(look.fill))?;
            if let Some(border) = look.border {
                r.stroke_rect(rect, look.radius, theme.border_width, with(border))?;
            }
            let style = TextStyle::new(theme.text.body, with(look.text)).font(theme.fonts.body);
            let size = r.measure_text(text.as_str(), &style);
            let at: Point = centred(rect, size);
            Self::in_clip(r, rect, |r| r.draw_text(text.as_str(), at, &style))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Color, Point, Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use crate::event::Pointer;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::{ActionKind, CrossAlign, NodeId, Style, Tree};

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const W: u32 = 200;
    const H: u32 = 160;
    const BOUNDS: Rect = Rect::new(0, 0, W, H);

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(W, H).unwrap()
    }

    struct Ui {
        t: Tree<16>,
        button: NodeId,
        toast: NodeId,
    }

    fn ui() -> Ui {
        let mut t = Tree::<16>::new();
        let root = t
            .column(None, Style::new().padding(8).align(CrossAlign::Start))
            .unwrap();
        let button = t
            .button(Some(root), "Press", ButtonVariant::Primary)
            .unwrap();
        let toast = t.toast(Some(root)).unwrap();
        t.update(&screen(), &THEME, BOUNDS);
        Ui { t, button, toast }
    }

    /// The whole picture, drawn from scratch.
    fn full(u: &mut Ui) -> Vec<Color> {
        let mut s = screen();
        u.t.update(&s, &THEME, BOUNDS);
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        pixels(&s)
    }

    fn pixels(s: &VelloCpu) -> Vec<Color> {
        (0..H as i32)
            .flat_map(|y| (0..W as i32).map(move |x| (x, y)))
            .map(|(x, y)| s.pixel(x, y).unwrap())
            .collect()
    }

    /// A platform's frame on a picture it keeps: what changed is drawn over it.
    fn frame(u: &mut Ui, s: &mut VelloCpu) {
        u.t.update(s, &THEME, BOUNDS);
        if let Some(d) = u.t.take_damage_in(BOUNDS) {
            s.save().unwrap();
            s.clip(d).unwrap();
            s.clear(THEME.colors.background).unwrap();
            u.t.paint_region(s, &THEME, d).unwrap();
            s.restore().unwrap();
        }
    }

    #[test]
    fn a_hidden_toast_takes_no_room_and_a_shown_one_sits_at_the_bottom_centre() {
        let mut u = ui();
        assert_eq!(u.t.rect(u.toast).map(|r| (r.w, r.h)), Some((0, 0)));
        assert!(!u.t.is_toast_shown(u.toast));
        u.t.show_toast(u.toast, "Saved", 1000);
        u.t.update(&screen(), &THEME, BOUNDS);
        let r = u.t.rect(u.toast).unwrap();
        assert!(u.t.is_toast_shown(u.toast));
        assert!(r.w > 0 && r.h > 0);
        assert!(
            (r.x + (r.w / 2) as i32 - (W / 2) as i32).abs() <= 1,
            "centred: {r:?}"
        );
        assert!(
            r.bottom() <= H as i32 - THEME.spacing.lg as i32 + 1 && r.bottom() > H as i32 / 2,
            "{r:?}"
        );
        assert_eq!(
            u.t.text(u.toast),
            None,
            "a toast is read through its message only while shown"
        );
    }

    #[test]
    fn it_comes_in_stays_and_goes_and_the_page_is_exactly_back_afterwards() {
        let mut u = ui();
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        let page = pixels(&s);
        u.t.show_toast(u.toast, "Saved", 400);
        let normal = THEME.motion.normal_ms;
        frame(&mut u, &mut s);
        let r = u.t.rect(u.toast).unwrap();
        let spot = (r.x + (r.w / 2) as i32, r.y + 2);
        assert_eq!(
            s.pixel(spot.0, spot.1),
            Some(THEME.colors.background),
            "not there yet at the first frame"
        );
        let mut seen_partial = false;
        let mut waited = 0;
        // Through the entrance: it gets more opaque at every step, and every frame equals a full redraw.
        let mut last = 0u32;
        while waited < normal {
            assert!(u.t.advance(&THEME, 20));
            waited += 20;
            frame(&mut u, &mut s);
            assert_eq!(pixels(&s), full(&mut u), "entering, {waited} ms");
            let p = s.pixel(spot.0, spot.1).unwrap();
            let grey = u32::from(p.r) + u32::from(p.g) + u32::from(p.b);
            assert!(grey >= last || waited >= normal, "{grey} after {last}");
            last = grey;
            seen_partial |= p != THEME.colors.background && p != THEME.colors.text;
        }
        assert!(seen_partial, "it faded in, it did not pop");
        assert_eq!(s.pixel(spot.0, spot.1), Some(THEME.colors.text), "fully in");
        // Holding: no change, no frames wanted for the picture, but the clock runs.
        assert!(u.t.advance(&THEME, 200));
        frame(&mut u, &mut s);
        assert_eq!(
            s.pixel(spot.0, spot.1),
            Some(THEME.colors.text),
            "still there"
        );
        // Out.
        let mut steps = 0;
        while u.t.advance(&THEME, 20) {
            frame(&mut u, &mut s);
            assert_eq!(pixels(&s), full(&mut u), "leaving, step {steps}");
            steps += 1;
            assert!(steps < 100, "it must end");
        }
        frame(&mut u, &mut s);
        assert!(!u.t.is_toast_shown(u.toast));
        assert!(!u.t.needs_frame(), "nothing left to do");
        assert_eq!(pixels(&s), page, "the page is exactly back");
    }

    #[test]
    fn showing_again_restarts_it_and_hiding_starts_the_exit() {
        let mut u = ui();
        u.t.show_toast(u.toast, "One", 5000);
        u.t.advance(&THEME, 1000);
        assert!(u.t.is_toast_shown(u.toast));
        u.t.show_toast(u.toast, "Two", 5000);
        // Restarted: after half the entrance it is still coming in, not fully in.
        u.t.advance(&THEME, THEME.motion.normal_ms / 2);
        let mut s = screen();
        u.t.update(&s, &THEME, BOUNDS);
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        let r = u.t.rect(u.toast).unwrap();
        assert_ne!(
            s.pixel(r.x + (r.w / 2) as i32, r.y + 2),
            Some(THEME.colors.text)
        );
        u.t.hide_toast(u.toast);
        assert!(u.t.advance(&THEME, THEME.motion.normal_ms - 1));
        assert!(!u.t.advance(&THEME, 5), "gone after the exit");
        assert!(!u.t.is_toast_shown(u.toast));
        // Hiding what is hidden, a stale handle and the wrong kind do nothing.
        u.t.hide_toast(u.toast);
        u.t.show_toast(u.button, "x", 10);
        u.t.hide_toast(u.button);
        let gone = u.toast;
        u.t.remove(gone);
        u.t.show_toast(gone, "x", 10);
        assert!(!u.t.is_toast_shown(gone));
    }

    #[test]
    fn a_very_long_frame_carries_it_through_every_phase() {
        let mut u = ui();
        u.t.show_toast(u.toast, "Long", 300);
        assert!(!u.t.advance(&THEME, 60_000));
        assert!(!u.t.is_toast_shown(u.toast));
        u.t.show_toast(u.toast, "Zero", 0);
        assert!(!u.t.advance(&THEME, u32::MAX));
        u.t.show_toast(
            u.toast,
            "A message much longer than the screen is wide, cut",
            100,
        );
        u.t.update(&screen(), &THEME, BOUNDS);
        let r = u.t.rect(u.toast).unwrap();
        assert!(r.x >= 0 && r.right() <= W as i32, "{r:?}");
    }

    #[test]
    fn a_toast_takes_no_presses_and_is_drawn_over_a_dialog() {
        let mut u = ui();
        // A press where the toast is goes through to what is under it.
        u.t.show_toast(u.toast, "Saved", 5000);
        u.t.update(&screen(), &THEME, BOUNDS);
        let b = u.t.rect(u.button).unwrap();
        // Put the button under the toast: a tall page.
        let p = Point::new(b.x + 3, b.y + 3);
        for e in [Pointer::Down(p), Pointer::Up(p), Pointer::Leave] {
            u.t.pointer(e);
        }
        assert_eq!(u.t.pop_action().map(|a| a.kind), Some(ActionKind::Clicked));
        // Over a dialog: the toast's pixels are the toast's.
        let dialog =
            u.t.dialog(
                Some(NodeId {
                    index: 0,
                    generation: 0,
                }),
                Style::new().padding(10),
            )
            .unwrap();
        u.t.label(Some(dialog), "Sure?", crate::theme::TextRole::Title)
            .unwrap();
        u.t.open_dialog(dialog);
        u.t.advance(&THEME, 1000);
        let mut s = screen();
        u.t.update(&s, &THEME, BOUNDS);
        s.clear(THEME.colors.background).unwrap();
        u.t.paint(&mut s, &THEME).unwrap();
        let r = u.t.rect(u.toast).unwrap();
        assert_eq!(
            s.pixel(r.x + (r.w / 2) as i32, r.y + 2),
            Some(THEME.colors.text),
            "over the scrim"
        );
    }
}
