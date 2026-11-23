//! What every platform needs and none should write twice: the input a platform feeds an app, the turning of
//! touch samples into pointer events, wheel units into pixels, a frame clock, and the per-frame step. A
//! board, a desktop and a phone implement only the parts that are really theirs (the screen, the raw input
//! devices, putting pixels on the display) and share this.

use fusuma_render::{Point, Rect, Renderer};

use crate::{App, Key, Pointer};

/// One thing a platform tells an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A finger or a mouse, in screen pixels.
    Pointer(Pointer),
    /// A key.
    Key(Key),
    /// The drawing area changed size, or the display's scale did.
    Resize {
        /// Width in pixels.
        width: u32,
        /// Height in pixels.
        height: u32,
        /// Display scale in percent: 100 is one pixel per logical pixel, 200 is a 2x phone or laptop screen.
        scale_percent: u32,
    },
}

/// Hands `input` to `app`.
pub fn deliver<A: App>(app: &mut A, input: Input) {
    match input {
        Input::Pointer(p) => app.pointer(p),
        Input::Key(k) => app.key(k),
        Input::Resize {
            width,
            height,
            scale_percent,
        } => app.resized(width, height, scale_percent),
    }
}

/// Turns the sampled state of a touch panel ("is a finger down, and where") into pointer events: a finger
/// that appears is a [`Pointer::Down`], one that moves is a [`Pointer::Move`], and one that is lifted is a
/// [`Pointer::Up`] followed by [`Pointer::Leave`], because a lifted finger hovers over nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TouchTracker {
    last: Option<Point>,
}

impl TouchTracker {
    /// A tracker with no finger down.
    #[must_use]
    pub const fn new() -> Self {
        Self { last: None }
    }

    /// Feeds one sample (`None` when no finger is down) and returns the events it makes, in order.
    pub fn step(&mut self, sample: Option<Point>) -> [Option<Pointer>; 2] {
        let before = self.last;
        self.last = sample;
        match (sample, before) {
            (Some(p), None) => [Some(Pointer::Down(p)), None],
            // A finger that has not moved says nothing new.
            (Some(p), Some(q)) if p == q => [None, None],
            (Some(p), Some(_)) => [Some(Pointer::Move(p)), None],
            (None, Some(q)) => [Some(Pointer::Up(q)), Some(Pointer::Leave)],
            (None, None) => [None, None],
        }
    }

    /// Whether a finger is down.
    #[must_use]
    pub const fn is_down(&self) -> bool {
        self.last.is_some()
    }
}

/// A wheel turn of `lines` (a mouse wheel notch is usually 1 to 3 lines) as the pixels [`Pointer::Wheel`]
/// takes, at `line_height` pixels a line. Positive scrolls the content up. Does not overflow.
#[must_use]
pub fn wheel_pixels(lines: i32, line_height: i32) -> i32 {
    lines.saturating_mul(line_height)
}

/// A frame clock: milliseconds since the last frame, from whatever monotonic time a platform has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    last: Option<u64>,
}

impl Clock {
    /// A clock that has not seen a frame.
    #[must_use]
    pub const fn new() -> Self {
        Self { last: None }
    }

    /// Milliseconds since the last call, given the monotonic time `now_ms`. The first call is 0, and a
    /// time that went backwards counts as no time having passed. The result never exceeds `u32::MAX`.
    pub fn dt(&mut self, now_ms: u64) -> u32 {
        let dt = self.last.map_or(0, |l| now_ms.saturating_sub(l));
        self.last = Some(self.last.map_or(now_ms, |l| l.max(now_ms)));
        u32::try_from(dt).unwrap_or(u32::MAX)
    }
}

/// How the frames are going, for a log line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Frames drawn.
    pub frames: u32,
    /// Milliseconds spent drawing and presenting them.
    pub busy_ms: u64,
}

impl Stats {
    /// Records a frame that took `ms`.
    pub fn record(&mut self, ms: u64) {
        self.frames = self.frames.saturating_add(1);
        self.busy_ms = self.busy_ms.saturating_add(ms);
    }

    /// Average milliseconds per frame; 0 before the first.
    #[must_use]
    pub fn average_ms(&self) -> u64 {
        self.busy_ms
            .checked_div(u64::from(self.frames))
            .unwrap_or(0)
    }
}

/// An app and the state every platform keeps for it: the clock and the frame statistics.
#[derive(Debug)]
pub struct Shell<A> {
    /// The application.
    pub app: A,
    clock: Clock,
    /// How the frames are going.
    pub stats: Stats,
}

impl<A: App> Shell<A> {
    /// Wraps `app`.
    pub const fn new(app: A) -> Self {
        Self {
            app,
            clock: Clock::new(),
            stats: Stats {
                frames: 0,
                busy_ms: 0,
            },
        }
    }

    /// Hands an input to the app.
    pub fn input(&mut self, input: Input) {
        deliver(&mut self.app, input);
    }

    /// Hands several inputs to the app, in order.
    pub fn inputs(&mut self, inputs: impl IntoIterator<Item = Input>) {
        for i in inputs {
            self.input(i);
        }
    }

    /// One frame at the monotonic time `now_ms`: time passes in the app, then what changed is drawn on
    /// `renderer`. Returns the rectangle that was redrawn, which is all the platform has to put on the
    /// display, or `None` when nothing changed.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn frame<R: Renderer>(
        &mut self,
        now_ms: u64,
        renderer: &mut R,
    ) -> Result<Option<Rect>, R::Error> {
        let dt = self.clock.dt(now_ms);
        self.app.tick(dt);
        self.app.frame(renderer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ButtonVariant, Style, Theme, Tree};

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn a_tap_is_down_then_up_and_leave_at_the_last_position() {
        let mut t = TouchTracker::new();
        assert_eq!(t.step(None), [None, None]);
        assert!(!t.is_down());
        assert_eq!(
            t.step(Some(p(10, 20))),
            [Some(Pointer::Down(p(10, 20))), None]
        );
        assert!(t.is_down());
        assert_eq!(
            t.step(Some(p(12, 22))),
            [Some(Pointer::Move(p(12, 22))), None]
        );
        assert_eq!(
            t.step(None),
            [Some(Pointer::Up(p(12, 22))), Some(Pointer::Leave)]
        );
        assert_eq!(t.step(None), [None, None]);
        assert!(!t.is_down());
    }

    #[test]
    fn a_finger_that_has_not_moved_makes_no_event() {
        let mut t = TouchTracker::new();
        t.step(Some(p(5, 5)));
        assert_eq!(t.step(Some(p(5, 5))), [None, None]);
        assert_eq!(t.step(Some(p(5, 6))), [Some(Pointer::Move(p(5, 6))), None]);
    }

    #[test]
    fn a_second_touch_after_a_lift_starts_a_new_press() {
        let mut t = TouchTracker::new();
        t.step(Some(p(1, 1)));
        t.step(None);
        assert_eq!(t.step(Some(p(9, 9))), [Some(Pointer::Down(p(9, 9))), None]);
    }

    #[test]
    fn wheel_units_become_pixels_without_overflow() {
        assert_eq!(wheel_pixels(3, 20), 60);
        assert_eq!(wheel_pixels(-2, 20), -40);
        assert_eq!(wheel_pixels(i32::MAX, 20), i32::MAX);
        assert_eq!(wheel_pixels(i32::MIN, 20), i32::MIN);
        assert_eq!(wheel_pixels(5, 0), 0);
    }

    #[test]
    fn the_clock_gives_the_time_since_the_last_frame() {
        let mut c = Clock::new();
        assert_eq!(c.dt(1000), 0, "the first frame has no past");
        assert_eq!(c.dt(1016), 16);
        assert_eq!(c.dt(1016), 0);
        assert_eq!(c.dt(1000), 0, "time going backwards is no time");
        assert_eq!(c.dt(1020), 4, "and does not make the next frame longer");
        assert_eq!(
            c.dt(u64::MAX),
            u32::MAX,
            "a huge gap is cut to the largest step"
        );
    }

    #[test]
    fn the_stats_average_and_do_not_divide_by_zero() {
        let mut s = Stats::default();
        assert_eq!(s.average_ms(), 0);
        s.record(10);
        s.record(30);
        assert_eq!((s.frames, s.average_ms()), (2, 20));
        s.record(u64::MAX);
        assert_eq!(s.busy_ms, u64::MAX, "saturates");
    }

    /// An app that records what it is told, to check the shared plumbing and nothing else.
    #[derive(Default)]
    struct Spy {
        seen: [u32; 4],
        size: Option<(u32, u32, u32)>,
        ticks: u32,
    }

    impl App for Spy {
        fn pointer(&mut self, _: Pointer) {
            self.seen[0] += 1;
        }
        fn key(&mut self, _: Key) {
            self.seen[1] += 1;
        }
        fn resized(&mut self, w: u32, h: u32, s: u32) {
            self.size = Some((w, h, s));
        }
        fn tick(&mut self, dt: u32) -> bool {
            self.ticks += dt;
            false
        }
        fn frame<R: Renderer>(&mut self, _: &mut R) -> Result<Option<Rect>, R::Error> {
            self.seen[3] += 1;
            Ok(None)
        }
    }

    #[test]
    fn inputs_reach_the_app_and_a_frame_ticks_by_the_clock_then_draws() {
        let mut shell = Shell::new(Spy::default());
        shell.inputs([
            Input::Pointer(Pointer::Leave),
            Input::Key(Key::Next),
            Input::Resize {
                width: 800,
                height: 600,
                scale_percent: 200,
            },
        ]);
        assert_eq!(shell.app.seen[..2], [1, 1]);
        assert_eq!(shell.app.size, Some((800, 600, 200)));
        let mut r = fusuma_render_vello_cpu::new(8, 8).unwrap();
        shell.frame(100, &mut r).unwrap();
        shell.frame(116, &mut r).unwrap();
        assert_eq!((shell.app.ticks, shell.app.seen[3]), (16, 2));
    }

    #[test]
    fn an_app_that_ignores_resizes_still_works_because_the_hook_has_a_default() {
        struct Quiet;
        impl App for Quiet {
            fn pointer(&mut self, _: Pointer) {}
            fn key(&mut self, _: Key) {}
            fn tick(&mut self, _: u32) -> bool {
                false
            }
            fn frame<R: Renderer>(&mut self, _: &mut R) -> Result<Option<Rect>, R::Error> {
                Ok(None)
            }
        }
        let mut shell = Shell::new(Quiet);
        shell.input(Input::Resize {
            width: 1,
            height: 1,
            scale_percent: 100,
        });
    }

    /// A real tree behind the shell: a platform feeding touch samples clicks a button.
    #[test]
    fn touch_samples_through_the_tracker_and_shell_click_a_button() {
        struct Ui {
            tree: Tree<4>,
            ok: crate::NodeId,
            clicked: bool,
        }
        impl App for Ui {
            fn pointer(&mut self, e: Pointer) {
                self.tree.pointer(e);
                self.clicked |= self.tree.pop_action().is_some_and(|a| a.source == self.ok);
            }
            fn key(&mut self, _: Key) {}
            fn tick(&mut self, _: u32) -> bool {
                false
            }
            fn frame<R: Renderer>(&mut self, r: &mut R) -> Result<Option<Rect>, R::Error> {
                self.tree.update(r, &Theme::DARK, Rect::new(0, 0, 100, 100));
                Ok(None)
            }
        }
        let mut tree = Tree::<4>::new();
        let root = tree.column(None, Style::new()).unwrap();
        let ok = tree
            .button(Some(root), "OK", ButtonVariant::Primary)
            .unwrap();
        let mut shell = Shell::new(Ui {
            tree,
            ok,
            clicked: false,
        });
        let mut r = fusuma_render_vello_cpu::new(100, 100).unwrap();
        shell.frame(0, &mut r).unwrap();
        let at = shell.app.tree.rect(ok).unwrap();
        let finger = Point::new(at.x + 3, at.y + 3);
        let mut touch = TouchTracker::new();
        for sample in [Some(finger), Some(finger), None] {
            for event in touch.step(sample).into_iter().flatten() {
                shell.input(Input::Pointer(event));
            }
        }
        assert!(shell.app.clicked);
    }
}
