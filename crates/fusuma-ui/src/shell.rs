//! What every platform needs and none should write twice: the input a platform feeds an app, the turning of
//! touch samples into pointer events, wheel units into pixels, a frame clock, and the per-frame step. A
//! board, a desktop and a phone implement only the parts that are really theirs (the screen, the raw input
//! devices, putting pixels on the display) and share this.

use fusuma_render::{Point, Rect, Renderer};

use crate::{App, Edit, InputText, Key, Pointer};

/// One thing a platform tells an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// A finger or a mouse, in screen pixels.
    Pointer(Pointer),
    /// A key.
    Key(Key),
    /// Text typed, pasted or committed by an input method (at most [`crate::INPUT_CAP`] bytes at a time; a
    /// platform sends longer text in pieces).
    Text(InputText),
    /// An input method is composing this text; empty ends it.
    Preedit(InputText),
    /// An edit command for a text field.
    Edit(Edit),
    /// The shape of the screen (rounded corners, a circle); a rectangle needs no message.
    Shape(crate::Shape),
    /// The colours the settings ask for (`[theme]` in `fusuma.toml`); a platform sends it once at start.
    Palette(crate::Palette),
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

/// Text of any length as inputs of at most [`crate::INPUT_CAP`] bytes each, cut at character boundaries, for a
/// platform that has pasted text to send.
pub fn text_inputs(text: &str) -> impl Iterator<Item = Input> + '_ {
    let mut rest = text;
    core::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let mut end = rest.len().min(crate::INPUT_CAP);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (head, tail) = rest.split_at(end);
        rest = tail;
        Some(Input::Text(InputText::new(head)))
    })
}

/// Hands `input` to `app`.
pub fn deliver<A: App>(app: &mut A, input: Input) {
    match input {
        Input::Pointer(p) => app.pointer(p),
        Input::Key(k) => app.key(k),
        Input::Text(t) => app.text(t.as_str()),
        Input::Preedit(t) => app.preedit(t.as_str()),
        Input::Edit(e) => app.edit(e),
        Input::Shape(s) => app.shape(s),
        Input::Palette(p) => app.palette(p),
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
    /// Something was handed to the app since the last frame.
    pending: bool,
    /// The last frame left the app with more to do (an animation, a change not yet drawn).
    busy: bool,
    /// The time at which the app asked to be woken (its [`deadline`](App::deadline) added to the last frame's time).
    due: Option<u64>,
    /// How the frames are going.
    pub stats: Stats,
}

impl<A: App> Shell<A> {
    /// Wraps `app`.
    pub const fn new(app: A) -> Self {
        Self {
            app,
            clock: Clock::new(),
            pending: false,
            // The first frame has to be drawn.
            busy: true,
            due: None,
            stats: Stats {
                frames: 0,
                busy_ms: 0,
            },
        }
    }

    /// Hands an input to the app.
    pub fn input(&mut self, input: Input) {
        self.pending = true;
        deliver(&mut self.app, input);
    }

    /// Tells the shell that the platform slept until `now_ms`. The time asleep is not time the app lived
    /// through, so the next frame's step starts from here, and an animation begun by the input that woke the
    /// platform runs from its start instead of jumping by seconds. Call it when waking from a wait, before
    /// the frame. An app that needs real time to pass while idle (a timer) returns `true` from
    /// [`App::tick`] for as long as it is pending, which keeps the platform awake.
    ///
    /// The exception is time the app asked for: while it waits on a [`deadline`](App::deadline) (a toast that will hide, a
    /// press that will become a long press) the time asleep is time it needs, so it is kept.
    pub fn resume(&mut self, now_ms: u64) {
        if self.due.is_none() {
            let _ = self.clock.dt(now_ms);
        }
    }

    /// When the platform should run the next frame: [`Wake::Now`] when something arrived or is moving, [`Wake::At`] when
    /// nothing moves but the app waits on the clock, [`Wake::Never`] when only an input will do. It replaces polling
    /// [`wants_frame`](Self::wants_frame) and sleeping a fixed time.
    #[must_use]
    pub fn wake(&self, now_ms: u64) -> crate::Wake {
        if self.wants_frame() {
            return crate::Wake::Now;
        }
        match self.due {
            Some(due) => {
                crate::Wake::At(u32::try_from(due.saturating_sub(now_ms)).unwrap_or(u32::MAX))
            }
            None => crate::Wake::Never,
        }
    }

    /// Whether a frame is worth running now: something arrived since the last one, or the app said it has
    /// more to do (an animation in progress). When this is false a platform can sleep until its next input
    /// instead of waking 60 times a second.
    #[must_use]
    pub const fn wants_frame(&self) -> bool {
        self.pending || self.busy
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
        let wants = self.app.tick(dt);
        self.pending = false;
        let drawn = self.app.frame(renderer);
        self.busy = wants || matches!(drawn, Ok(Some(_)) | Err(_));
        self.due = self
            .app
            .deadline()
            .map(|ms| now_ms.saturating_add(u64::from(ms)));
        drawn
    }

    /// The separate rectangles the last frame drew (see [`App::frame_rects`]); empty when the app does not say.
    #[must_use]
    pub fn frame_rects(&self) -> &[Rect] {
        self.app.frame_rects()
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

    /// An app with an animation that runs for `left` more milliseconds.
    struct Anim {
        left: u32,
        drawn: u32,
        dts: Vec<u32>,
    }

    impl App for Anim {
        fn pointer(&mut self, _: Pointer) {
            self.left = 100;
        }
        fn key(&mut self, _: Key) {}
        fn tick(&mut self, dt: u32) -> bool {
            self.dts.push(dt);
            self.left = self.left.saturating_sub(dt);
            self.left > 0
        }
        fn frame<R: Renderer>(&mut self, _: &mut R) -> Result<Option<Rect>, R::Error> {
            self.drawn += 1;
            Ok((self.left > 0).then(|| Rect::new(0, 0, 1, 1)))
        }
    }

    #[test]
    fn an_idle_shell_asks_for_no_frames_and_wakes_on_input_or_an_animation() {
        let mut shell = Shell::new(Anim {
            left: 0,
            drawn: 0,
            dts: Vec::new(),
        });
        let mut r = fusuma_render_vello_cpu::new(8, 8).unwrap();
        assert!(shell.wants_frame(), "the first frame is wanted");
        shell.frame(0, &mut r).unwrap();
        assert!(
            !shell.wants_frame(),
            "nothing to animate, nothing to draw: sleep"
        );
        shell.input(Input::Pointer(Pointer::Leave));
        assert!(shell.wants_frame(), "an input wakes it");
        shell.resume(5_000);
        shell.frame(5_000, &mut r).unwrap();
        assert!(
            shell.wants_frame(),
            "the animation it started is still running"
        );
        shell.frame(5_060, &mut r).unwrap();
        assert!(shell.wants_frame());
        shell.frame(5_120, &mut r).unwrap();
        assert!(
            !shell.wants_frame(),
            "and it ends, so the shell sleeps again"
        );
    }

    #[test]
    fn the_first_frame_after_a_sleep_does_not_see_the_time_asleep() {
        let mut shell = Shell::new(Anim {
            left: 0,
            drawn: 0,
            dts: Vec::new(),
        });
        let mut r = fusuma_render_vello_cpu::new(8, 8).unwrap();
        shell.frame(0, &mut r).unwrap();
        shell.input(Input::Pointer(Pointer::Leave));
        shell.resume(10_000);
        shell.frame(10_000, &mut r).unwrap();
        assert_eq!(
            shell.app.dts.last(),
            Some(&0),
            "ten seconds asleep are not ten seconds of animation"
        );
        shell.frame(10_016, &mut r).unwrap();
        assert_eq!(shell.app.dts.last(), Some(&16), "from then on time is real");
    }

    #[test]
    fn without_resume_the_time_is_real_so_a_polling_platform_loses_nothing() {
        let mut shell = Shell::new(Anim {
            left: 0,
            drawn: 0,
            dts: Vec::new(),
        });
        let mut r = fusuma_render_vello_cpu::new(8, 8).unwrap();
        shell.frame(0, &mut r).unwrap();
        shell.frame(7_000, &mut r).unwrap();
        assert_eq!(shell.app.dts.last(), Some(&7_000));
    }

    #[test]
    fn a_drawn_frame_keeps_the_shell_awake_for_one_more() {
        let mut shell = Shell::new(Anim {
            left: 1,
            drawn: 0,
            dts: Vec::new(),
        });
        let mut r = fusuma_render_vello_cpu::new(8, 8).unwrap();
        shell.frame(0, &mut r).unwrap();
        assert!(
            shell.wants_frame(),
            "a frame that drew something is followed by another to be sure"
        );
    }

    #[test]
    fn long_text_is_sent_in_pieces_that_rejoin_to_the_original() {
        let text = "é".repeat(2 * crate::INPUT_CAP) + "tail";
        let pieces: Vec<Input> = text_inputs(&text).collect();
        assert!(pieces.len() >= 4);
        let joined: String = pieces
            .iter()
            .map(|p| match p {
                Input::Text(t) => t.as_str().to_owned(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(joined, text);
        assert!(text_inputs("").next().is_none());
        assert_eq!(text_inputs("hi").count(), 1);
    }

    #[test]
    fn text_edits_and_preedit_reach_the_app_through_the_shell() {
        struct Typist {
            seen: Vec<String>,
        }
        impl App for Typist {
            fn pointer(&mut self, _: Pointer) {}
            fn key(&mut self, _: Key) {}
            fn tick(&mut self, _: u32) -> bool {
                false
            }
            fn frame<R: Renderer>(&mut self, _: &mut R) -> Result<Option<Rect>, R::Error> {
                Ok(None)
            }
            fn text(&mut self, t: &str) {
                self.seen.push(format!("text {t}"));
            }
            fn edit(&mut self, e: Edit) {
                self.seen.push(format!("edit {e:?}"));
            }
            fn preedit(&mut self, t: &str) {
                self.seen.push(format!("preedit {t}"));
            }
        }
        let mut shell = Shell::new(Typist { seen: Vec::new() });
        shell.inputs([
            Input::Text(InputText::new("ab")),
            Input::Preedit(InputText::new("に")),
            Input::Edit(Edit::Backspace),
        ]);
        assert_eq!(shell.app.seen, ["text ab", "preedit に", "edit Backspace"]);
        assert!(
            shell.wants_frame(),
            "text wakes a sleeping platform like any input"
        );
        assert!(
            !shell.app.wants_text()
                && shell.app.selected_text().is_none()
                && shell.app.text_cursor().is_none()
        );
    }

    /// An app that is idle but has a deadline (a message that hides in 400 ms): the shell says when to wake, and the
    /// time asleep until then counts, so the deadline is reached by the time the platform comes back.
    struct Timed {
        left: u32,
        ticks: u32,
    }

    impl App for Timed {
        fn pointer(&mut self, _: Pointer) {}
        fn key(&mut self, _: crate::Key) {}
        fn tick(&mut self, dt_ms: u32) -> bool {
            self.ticks += 1;
            self.left = self.left.saturating_sub(dt_ms);
            false
        }
        fn deadline(&self) -> Option<u32> {
            (self.left > 0).then_some(self.left)
        }
        fn frame<R: fusuma_render::Renderer>(
            &mut self,
            _: &mut R,
        ) -> Result<Option<Rect>, R::Error> {
            Ok(None)
        }
    }

    #[test]
    fn an_idle_app_with_a_deadline_sleeps_until_it_and_the_time_asleep_counts() {
        let mut r = fusuma_render_vello_cpu::new(8, 8).unwrap();
        let mut s = Shell::new(Timed {
            left: 400,
            ticks: 0,
        });
        assert_eq!(s.wake(0), crate::Wake::Now, "the first frame is wanted");
        s.frame(1000, &mut r).unwrap();
        assert!(!s.wants_frame());
        assert_eq!(s.wake(1000), crate::Wake::At(400));
        assert_eq!(
            s.wake(1150),
            crate::Wake::At(250),
            "it counts down as time passes"
        );
        assert_eq!(
            s.wake(5000),
            crate::Wake::At(0),
            "late is now, not negative"
        );
        // The platform slept 400 ms and calls `resume`: the 400 ms are kept, so the app's time is up.
        s.resume(1400);
        s.frame(1400, &mut r).unwrap();
        assert_eq!(s.app.left, 0, "the sleep was not thrown away");
        assert_eq!(s.wake(1400), crate::Wake::Never, "nothing left to wait for");
        // With nothing pending the time asleep is thrown away, as before.
        s.resume(9000);
        s.frame(9000, &mut r).unwrap();
        assert_eq!(s.app.ticks, 3);
        // An input is a frame now.
        s.input(Input::Pointer(Pointer::Leave));
        assert_eq!(s.wake(9000), crate::Wake::Now);
    }
}
