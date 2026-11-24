//! The scrollbar of a scroll container, as Material and Android show it: hidden at rest, there at once while the
//! content moves (a drag, a fling, a bounce, the wheel, the keyboard) and while a finger drags it, then gone after
//! [`MotionTimes::bar_hold_ms`](crate::theme::MotionTimes) with a fade of `bar_fade_ms`. A fade step redraws only the
//! bar's column, not the window.

use fusuma_render::{Color, Rect};

use crate::theme::Theme;
use crate::tree::{BAR_HIDDEN, Kind, Tree};

/// The column a scrollbar is drawn in, inside container rect `r`, with a pixel of room on each side: what has to be
/// drawn anew when the bar moves or fades.
pub(crate) fn bar_column(r: Rect, theme: &Theme) -> Rect {
    let w = (theme.track_height.clamp(2, 6) + 2).min(r.w);
    Rect::new(r.right() - w as i32, r.y, w, r.h)
}

/// How strongly a bar `ms` milliseconds after the last move shows, 0 (gone) to 255.
pub(crate) fn bar_alpha(ms: u16, theme: &Theme) -> u8 {
    if ms == BAR_HIDDEN {
        return 0;
    }
    let (hold, fade) = (theme.motion.bar_hold_ms, theme.motion.bar_fade_ms);
    let ms = u32::from(ms);
    if ms < hold {
        255
    } else if ms >= hold.saturating_add(fade) {
        0
    } else {
        (255 - (ms - hold) * 255 / fade) as u8
    }
}

/// The colour a bar at `alpha` is drawn in: the theme's strong border, faded; without blending (`can_blend` false)
/// it is drawn whole until half faded, then not at all. `None` when nothing is drawn.
pub(crate) fn bar_color(theme: &Theme, alpha: u8, can_blend: bool) -> Option<Color> {
    let base = theme.colors.border_strong;
    match (alpha, can_blend) {
        (0, _) => None,
        (a, true) => Some(base.with_alpha((u16::from(base.a) * u16::from(a) / 255) as u8)),
        (a, false) if a >= 128 => Some(base),
        _ => None,
    }
}

impl<const N: usize> Tree<N> {
    /// Container `i`'s content moved: its bar shows at full strength from now. The move itself redraws the bar.
    pub(crate) fn wake_bar(&mut self, i: usize) {
        if let Kind::Scroll { bar, .. } = &mut self.nodes[i].kind {
            *bar = 0;
            self.bars_awake = true;
        }
    }

    /// Steps every showing scrollbar by `dt_ms`, redrawing the column of each one whose strength changed. True while
    /// one is still showing, or changed in this step (its last redraw needs a frame).
    pub(crate) fn advance_bars(&mut self, theme: &Theme, dt_ms: u32) -> bool {
        if !self.bars_awake {
            return false;
        }
        let held = self
            .drag
            .filter(|d| d.moving)
            .and_then(|d| self.slot(d.scroll));
        let shifting = self.shift.and_then(|s| self.slot(s.scroll));
        let step = u16::try_from(dt_ms).unwrap_or(u16::MAX - 1);
        let (mut awake, mut changed) = (false, false);
        for i in 0..N {
            if !self.nodes[i].alive {
                continue;
            }
            let Kind::Scroll { bar, .. } = self.nodes[i].kind else {
                continue;
            };
            if bar == BAR_HIDDEN {
                continue;
            }
            let next = if held == Some(i) {
                0
            } else {
                let t = bar.saturating_add(step);
                let end = theme
                    .motion
                    .bar_hold_ms
                    .saturating_add(theme.motion.bar_fade_ms);
                if u32::from(t) >= end { BAR_HIDDEN } else { t }
            };
            if let Kind::Scroll { bar, .. } = &mut self.nodes[i].kind {
                *bar = next;
            }
            if bar_alpha(bar, theme) != bar_alpha(next, theme) {
                changed = true;
                // A container whose pixels are being moved redraws its bar column anyway (see `shift_plan`).
                if shifting != Some(i) {
                    self.add_damage(bar_column(self.nodes[i].rect, theme));
                }
            }
            awake |= next != BAR_HIDDEN;
        }
        self.bars_awake = awake;
        awake || changed
    }

    /// How strongly container `id`'s scrollbar shows now, 0 (hidden) to 255; `None` for a stale handle or another
    /// kind of node.
    #[must_use]
    pub fn scrollbar_alpha(&self, id: crate::NodeId, theme: &Theme) -> Option<u8> {
        let i = self.slot(id)?;
        match self.nodes[i].kind {
            Kind::Scroll { bar, .. } => Some(bar_alpha(bar, theme)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    const T: Theme = crate::theme::BITMAP_TEST;

    #[test]
    fn a_bar_holds_then_fades_then_is_gone() {
        let (hold, fade) = (T.motion.bar_hold_ms as u16, T.motion.bar_fade_ms as u16);
        assert_eq!(bar_alpha(BAR_HIDDEN, &T), 0);
        assert_eq!(bar_alpha(0, &T), 255);
        assert_eq!(bar_alpha(hold - 1, &T), 255);
        assert!((100..160).contains(&bar_alpha(hold + fade / 2, &T)));
        assert_eq!(bar_alpha(hold + fade, &T), 0);
        let mut instant = T;
        instant.motion.bar_fade_ms = 0;
        assert_eq!(
            bar_alpha(hold, &instant),
            0,
            "no fade: gone at once, no division by zero"
        );
    }

    #[test]
    fn without_blending_a_bar_is_whole_until_half_faded() {
        let full = T.colors.border_strong;
        assert_eq!(bar_color(&T, 255, true), Some(full));
        assert_eq!(bar_color(&T, 0, true), None);
        assert!(bar_color(&T, 100, true).is_some_and(|c| c.a < full.a));
        assert_eq!(bar_color(&T, 128, false), Some(full));
        assert_eq!(bar_color(&T, 127, false), None);
    }

    #[test]
    fn the_column_is_at_the_right_edge_and_never_wider_than_the_container() {
        let c = bar_column(Rect::new(10, 20, 100, 50), &T);
        assert_eq!((c.right(), c.y, c.h), (110, 20, 50));
        assert!(c.w >= T.track_height.clamp(2, 6));
        assert!(bar_column(Rect::new(0, 0, 3, 9), &T).w <= 3);
    }
}
