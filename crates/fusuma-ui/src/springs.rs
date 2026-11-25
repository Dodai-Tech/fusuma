//! A node that moves by a spring: it is told where it should be, and the toolkit carries it there with physics (stiffness,
//! damping, mass), from wherever it is and at whatever speed it has, so a new target in mid-flight bends the motion and never
//! jumps. The node owns the spring: the app's `view` runs once for the new target, not once a frame.

use fusuma_anim::{Fx, Spring, SpringConfig};

use crate::theme::Theme;
use crate::tree::{NodeId, Tree};

/// How many nodes can be on a spring at once.
pub const MAX_SPRINGS: usize = 4;

/// The longest a spring is stepped in one go, in milliseconds (the spring itself takes at most this much per call).
const STEP_MS: u32 = 50;

/// A node on a spring.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NodeSpring {
    node: NodeId,
    x: Spring,
    y: Spring,
    to: (i32, i32),
}

impl<const N: usize> Tree<N> {
    /// Moves node `id` to `target` (pixels from where its parent puts it) on a spring made from `config`: from where it is
    /// now, with the speed it has if it is already moving. A node whose target is where it is does not start.
    pub fn animate_to(&mut self, id: NodeId, target: (i32, i32), config: SpringConfig) {
        let Some(i) = self.slot(id) else { return };
        if let Some(s) = self.springs.iter_mut().flatten().find(|s| s.node == id) {
            s.x.set_target(Fx::from_int(target.0));
            s.y.set_target(Fx::from_int(target.1));
            s.to = target;
            return;
        }
        let now = {
            let (x, y) = self.nodes[i].shift;
            (i32::from(x), i32::from(y))
        };
        if now == target {
            return;
        }
        let slot = self
            .springs
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                // All busy: the oldest is put where it was going, and its place taken.
                if let Some(old) = self.springs[0].take() {
                    self.set_offset(old.node, old.to);
                }
                0
            });
        let mut x = Spring::new(config, Fx::from_int(now.0));
        let mut y = Spring::new(config, Fx::from_int(now.1));
        x.set_target(Fx::from_int(target.0));
        y.set_target(Fx::from_int(target.1));
        self.springs[slot] = Some(NodeSpring {
            node: id,
            x,
            y,
            to: target,
        });
    }

    /// Puts node `id` at `offset` at once, and stops any spring it was on.
    pub fn set_offset(&mut self, id: NodeId, offset: (i32, i32)) {
        let Some(i) = self.slot(id) else { return };
        for s in &mut self.springs {
            if s.is_some_and(|s| s.node == id) {
                *s = None;
            }
        }
        self.set_shift(i, offset);
    }

    /// Moves the springs on by `dt_ms`. True while one is moving.
    pub(crate) fn advance_springs(&mut self, theme: &Theme, dt_ms: u32) -> bool {
        // With no motion (a budget of none) everything is where it is going, at once.
        let instant = theme.motion.normal_ms == 0;
        for k in 0..MAX_SPRINGS {
            let Some(mut s) = self.springs[k] else {
                continue;
            };
            let Some(i) = self.slot(s.node) else {
                self.springs[k] = None;
                continue;
            };
            let mut moving = !instant;
            if moving {
                let mut left = dt_ms;
                while left > 0 && moving {
                    let step = left.min(STEP_MS);
                    let (a, b) = (s.x.step(step), s.y.step(step));
                    moving = a || b;
                    left -= step;
                }
            }
            if moving {
                let round = |v: Fx| v.round();
                self.set_shift(i, (round(s.x.position()), round(s.y.position())));
                self.springs[k] = Some(s);
            } else {
                // At rest: exactly on the target, and done.
                self.springs[k] = None;
                self.set_shift(i, s.to);
            }
        }
        self.springs.iter().any(Option::is_some)
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::{Probe, Rect, Renderer};
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::theme::ButtonVariant;
    use crate::tree::{CrossAlign, Style};

    const THEME: Theme = crate::theme::BITMAP_TEST;
    const BOUNDS: Rect = Rect::new(0, 0, 200, 120);

    fn screen() -> VelloCpu {
        fusuma_render_vello_cpu::new(200, 120).unwrap()
    }

    fn page() -> (Tree<8>, NodeId) {
        let mut t = Tree::<8>::new();
        let root = t
            .column(None, Style::new().padding(10).align(CrossAlign::Start))
            .unwrap();
        let b = t.button(Some(root), "Go", ButtonVariant::Primary).unwrap();
        t.update(&screen(), &THEME, BOUNDS);
        (t, b)
    }

    fn x_of(t: &mut Tree<8>, b: NodeId) -> i32 {
        t.update(&screen(), &THEME, BOUNDS);
        t.rect(b).unwrap().x
    }

    #[test]
    fn a_node_is_carried_to_its_target_and_ends_exactly_on_it_and_asks_for_no_more_frames() {
        let (mut t, b) = page();
        let home = x_of(&mut t, b);
        t.animate_to(b, (80, 0), SpringConfig::STIFF);
        assert!(t.needs_frame());
        let mut xs = vec![home];
        for _ in 0..400 {
            if !t.advance(&THEME, 16) {
                break;
            }
            xs.push(x_of(&mut t, b));
        }
        let end = x_of(&mut t, b);
        assert_eq!(end, home + 80, "exactly on the target");
        assert!(xs.len() > 4, "it took several frames: {xs:?}");
        assert!(
            xs[1] > home && xs[1] < home + 80,
            "a spring starts moving at once: {xs:?}"
        );
        t.paint_frame(&mut screen(), &THEME, BOUNDS).unwrap();
        assert!(
            !t.needs_frame() && !t.advance(&THEME, 16),
            "and then nothing is moving"
        );
    }

    #[test]
    fn a_bouncy_spring_overshoots_and_a_stiff_one_does_not() {
        let peak = |cfg: SpringConfig| {
            let (mut t, b) = page();
            let home = x_of(&mut t, b);
            t.animate_to(b, (60, 0), cfg);
            let mut peak = 0;
            while t.advance(&THEME, 16) {
                peak = peak.max(x_of(&mut t, b) - home);
            }
            peak
        };
        assert!(
            peak(SpringConfig::WOBBLY) > 66,
            "wobbly overshoots: {}",
            peak(SpringConfig::WOBBLY)
        );
        assert!(
            peak(SpringConfig::EFFECTS) <= 61,
            "effects do not: {}",
            peak(SpringConfig::EFFECTS)
        );
    }

    #[test]
    fn a_new_target_in_flight_bends_the_motion_and_never_jumps() {
        let (mut t, b) = page();
        let home = x_of(&mut t, b);
        t.animate_to(b, (100, 0), SpringConfig::GENTLE);
        for _ in 0..10 {
            t.advance(&THEME, 16);
        }
        let before = x_of(&mut t, b);
        assert!(before > home + 5 && before < home + 100);
        t.animate_to(b, (0, 0), SpringConfig::GENTLE);
        assert_eq!(
            x_of(&mut t, b),
            before,
            "the target changed, the node did not"
        );
        let mut last = before;
        let mut max_jump = 0;
        while t.advance(&THEME, 16) {
            let x = x_of(&mut t, b);
            max_jump = max_jump.max((x - last).abs());
            last = x;
        }
        assert!(max_jump < 25, "no frame jumps: {max_jump}");
        assert_eq!(x_of(&mut t, b), home, "and it came back");
    }

    #[test]
    fn what_is_drawn_on_the_way_is_what_a_full_redraw_draws() {
        let (mut t, b) = page();
        let mut s = screen();
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        let _ = t.take_damage();
        t.animate_to(b, (70, 30), SpringConfig::SPATIAL);
        let mut frames = 0;
        loop {
            let moving = t.advance(&THEME, 16);
            t.update(&s, &THEME, BOUNDS);
            if let Some(d) = t.take_damage_in(BOUNDS) {
                s.save().unwrap();
                s.clip(d).unwrap();
                s.clear(THEME.colors.background).unwrap();
                t.paint_region(&mut s, &THEME, d).unwrap();
                s.restore().unwrap();
            }
            let mut full = screen();
            full.clear(THEME.colors.background).unwrap();
            t.paint(&mut full, &THEME).unwrap();
            for y in (0..120).step_by(3) {
                for x in (0..200).step_by(3) {
                    assert_eq!(
                        s.pixel(x, y),
                        full.pixel(x, y),
                        "frame {frames} at ({x}, {y})"
                    );
                }
            }
            frames += 1;
            if !moving || frames > 300 {
                break;
            }
        }
        assert!(frames > 5);
    }

    #[test]
    fn no_motion_budget_puts_it_there_at_once_and_odd_calls_are_harmless() {
        let (mut t, b) = page();
        let home = x_of(&mut t, b);
        let off = THEME.with_motion(crate::MotionBudget::Off);
        t.animate_to(b, (40, 0), SpringConfig::WOBBLY);
        assert!(!t.advance(&off, 0));
        assert_eq!(x_of(&mut t, b), home + 40);
        // A target where it already is does not start a spring; a stale handle is ignored; set_offset stops one.
        t.paint_frame(&mut screen(), &THEME, BOUNDS).unwrap();
        t.animate_to(b, (40, 0), SpringConfig::WOBBLY);
        assert!(!t.needs_frame());
        t.animate_to(b, (0, 0), SpringConfig::WOBBLY);
        t.set_offset(b, (9, 0));
        assert!(!t.advance(&THEME, 16));
        assert_eq!(x_of(&mut t, b), home + 9);
        let gone = b;
        t.remove(gone);
        t.animate_to(gone, (5, 5), SpringConfig::GENTLE);
        t.set_offset(gone, (1, 1));
        assert!(!t.advance(&THEME, 16));
    }

    #[test]
    fn more_springs_than_room_finish_the_oldest_where_it_was_going() {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new()).unwrap();
        let nodes: Vec<NodeId> = (0..MAX_SPRINGS + 2)
            .map(|_| t.button(Some(root), "x", ButtonVariant::Primary).unwrap())
            .collect();
        for (k, n) in nodes.iter().enumerate() {
            t.animate_to(*n, (10 + k as i32, 0), SpringConfig::STIFF);
        }
        assert_eq!(
            t.slide_offset(nodes[0]),
            Some((10, 0)),
            "the first was put at its target"
        );
        while t.advance(&THEME, 16) {}
        for (k, n) in nodes.iter().enumerate() {
            assert_eq!(t.slide_offset(*n), Some((10 + k as i32, 0)));
        }
    }
}
