//! The frame step every board shares: read touch, run a frame, show what changed.

use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::{App, Input, Shell, TouchTracker};

use crate::board::Board;
use crate::surface::{DrawError, Surface};

/// What one [`Driver::step`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// The part of the screen that was redrawn and shown, or `None` when nothing changed.
    pub shown: Option<Rect>,
    /// Whether the app wants another frame soon (an animation, a change not drawn yet). When false a runtime can
    /// sleep until touch arrives, or the next poll if it has no interrupt.
    pub wants_frame: bool,
    /// When to run the next frame (see [`fusuma::Wake`]): now, at a deadline, or only when something arrives.
    pub wake: fusuma::Wake,
    /// How long each stage took, when the board has a clock ([`Board::now_us`]) and something was shown.
    pub timing: Option<Timing>,
}

/// Where the time of one step went, in microseconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Timing {
    /// Reading the touch panel and handing its events to the app.
    pub input_us: u32,
    /// The app's frame: layout and the paint commands (with the integer renderer, the drawing itself).
    pub draw_us: u32,
    /// Turning the paint commands into pixels, for a renderer that does it lazily (Vello); 0 for the integer one.
    pub raster_us: u32,
    /// The board showing the region: copying or converting the rows and the transfer to the panel.
    pub present_us: u32,
}

/// Why a step failed.
#[derive(Debug, PartialEq, Eq)]
pub enum DriverError<E> {
    /// The renderer failed.
    Draw(DrawError),
    /// The board failed to show the frame.
    Board(E),
}

/// An app, the board it runs on and the surface it draws on.
#[derive(Debug)]
pub struct Driver<A, B> {
    shell: Shell<A>,
    board: B,
    surface: Surface,
    touch: TouchTracker,
    screen: Rect,
}

impl<A: App, B: Board> Driver<A, B> {
    /// Runs `app` on `board` with `backend`. `None` when the surface cannot be made (the backend is not linked,
    /// or there is no room for it).
    pub fn new(app: A, board: B, backend: Backend) -> Option<Self> {
        let (w, h) = board.size();
        let surface = Surface::new(backend, w, h)?;
        let mut shell = Shell::new(app);
        let shape = board.shape();
        if shape != fusuma::Shape::Rect {
            shell.input(Input::Shape(shape));
        }
        let screen = board.screen();
        if screen != fusuma::Screen::new(w, h).with_shape(shape) {
            shell.input(Input::Screen(screen));
        }
        Some(Self {
            shell,
            board,
            surface,
            touch: TouchTracker::new(),
            screen: Rect::new(0, 0, w, h),
        })
    }

    /// The app and its frame statistics.
    pub const fn shell(&self) -> &Shell<A> {
        &self.shell
    }

    /// The app, to change it from outside.
    pub fn shell_mut(&mut self) -> &mut Shell<A> {
        &mut self.shell
    }

    /// The board.
    pub const fn board(&self) -> &B {
        &self.board
    }

    /// Draws the font roles with `set`'s families (`[fonts]` in the settings). Call it before the first
    /// [`step`](Self::step): the first frame lays out the text with these families; a later change would leave text
    /// laid out for the old ones until the app lays out again.
    #[cfg(feature = "fonts")]
    pub fn set_fonts(&mut self, set: fusuma::fonts::FontSet) {
        self.surface.set_fonts(set);
    }

    /// The board, to read and reset its own numbers or change a setting (brightness) from outside.
    pub fn board_mut(&mut self) -> &mut B {
        &mut self.board
    }

    /// The surface the app draws on.
    pub const fn surface(&self) -> &Surface {
        &self.surface
    }

    /// Hands an input that did not come from the touch panel (a button, a key, a serial command) to the app.
    pub fn input(&mut self, input: Input) {
        self.shell.input(input);
    }

    /// Tells the driver the runtime slept until `now_ms`: the time asleep is not time the app lived through.
    pub fn resume(&mut self, now_ms: u64) {
        self.shell.resume(now_ms);
    }

    /// Whether a frame is worth running now.
    pub const fn wants_frame(&self) -> bool {
        self.shell.wants_frame()
    }

    /// When the runtime should run the next frame, given the monotonic time `now_ms`.
    #[must_use]
    pub fn wake(&self, now_ms: u64) -> fusuma::Wake {
        self.shell.wake(now_ms)
    }

    /// One step at the monotonic time `now_ms`: the touch panel is read and turned into pointer events, a frame
    /// runs, and the part that changed is shown. A touch panel that fails is treated as a lifted finger, so a
    /// flaky bus cannot leave a button pressed.
    ///
    /// # Errors
    /// [`DriverError`] when drawing or showing the frame failed.
    pub fn step(&mut self, now_ms: u64) -> Result<Step, DriverError<B::Error>> {
        let t0 = self.board.now_us();
        let sample = self.board.touch().unwrap_or(None);
        for event in self.touch.step(sample).into_iter().flatten() {
            self.shell.input(Input::Pointer(event));
        }
        let t1 = self.board.now_us();
        let drawn = self
            .surface
            .frame(&mut self.shell, now_ms)
            .map_err(DriverError::Draw)?;
        let shown = drawn
            .map(|r| r.intersect(&self.screen))
            .filter(|r| r.w > 0 && r.h > 0);
        let (mut t2, mut t3, mut t4) = (t1, t1, t1);
        if let Some(region) = shown {
            t2 = self.board.now_us();
            self.surface.rasterise();
            t3 = self.board.now_us();
            // Several separate rectangles are sent one by one, so what lies between them costs nothing on the bus.
            let mut parts = [Rect::new(0, 0, 0, 0); fusuma::MAX_PIECES];
            let mut n = 0;
            for r in self.shell.frame_rects().iter().take(parts.len()) {
                let r = r.intersect(&self.screen);
                if r.w > 0 && r.h > 0 {
                    parts[n] = r;
                    n += 1;
                }
            }
            if n < 2 {
                (parts[0], n) = (region, 1);
            }
            for part in &parts[..n] {
                self.board
                    .present(*part, &mut self.surface)
                    .map_err(DriverError::Board)?;
            }
            t4 = self.board.now_us();
        }
        let span = |a: Option<u64>, b: Option<u64>| {
            a.zip(b).map_or(0, |(a, b)| {
                u32::try_from(b.saturating_sub(a)).unwrap_or(u32::MAX)
            })
        };
        let timing = (shown.is_some() && t0.is_some()).then(|| Timing {
            input_us: span(t0, t1),
            draw_us: span(t1, t2),
            raster_us: span(t2, t3),
            present_us: span(t3, t4),
        });
        Ok(Step {
            shown,
            wants_frame: self.shell.wants_frame(),
            wake: self.shell.wake(now_ms),
            timing,
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::collections::VecDeque;
    use std::vec;
    use std::vec::Vec;

    use super::*;
    use crate::board::{Pixels, rgb565};
    use fusuma::render::Point;
    use fusuma_showcase::Showcase;

    /// A screen in memory and a script of touch samples.
    struct Fake {
        w: u32,
        h: u32,
        screen: Vec<u16>,
        touches: VecDeque<Result<Option<Point>, ()>>,
        shown: Vec<Rect>,
        /// How many pixels are asked for at a time: a bus that sends a few at a time.
        chunk: usize,
        fail_present: bool,
        shape: fusuma::Shape,
        /// What the board says the device is, if it says.
        facts: Option<fusuma::Screen>,
        /// A clock that moves 10 us at every reading; `None`: no clock.
        clock: Option<core::cell::Cell<u64>>,
    }

    impl Fake {
        fn new(w: u32, h: u32) -> Self {
            Self {
                w,
                h,
                screen: vec![0; (w * h) as usize],
                touches: VecDeque::new(),
                shown: Vec::new(),
                chunk: 1_000_000,
                fail_present: false,
                shape: fusuma::Shape::Rect,
                facts: None,
                clock: None,
            }
        }

        fn at(&self, x: u32, y: u32) -> u16 {
            self.screen[(y * self.w + x) as usize]
        }
    }

    impl Board for Fake {
        type Error = ();

        fn size(&self) -> (u32, u32) {
            (self.w, self.h)
        }

        fn shape(&self) -> fusuma::Shape {
            self.shape
        }

        fn screen(&self) -> fusuma::Screen {
            self.facts
                .unwrap_or_else(|| fusuma::Screen::new(self.w, self.h).with_shape(self.shape))
        }

        fn touch(&mut self) -> Result<Option<Point>, ()> {
            self.touches.pop_front().unwrap_or(Ok(None))
        }

        fn now_us(&self) -> Option<u64> {
            self.clock.as_ref().map(|c| {
                c.set(c.get() + 10);
                c.get()
            })
        }

        fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) -> Result<(), ()> {
            if self.fail_present {
                return Err(());
            }
            self.shown.push(region);
            for y in region.y as u32..region.bottom() as u32 {
                let mut x = region.x as u32;
                while x < region.right() as u32 {
                    let n = self.chunk.min((region.right() as u32 - x) as usize);
                    let at = (y * self.w + x) as usize;
                    pixels.row(y, x, &mut self.screen[at..at + n]);
                    x += n as u32;
                }
            }
            Ok(())
        }
    }

    #[test]
    fn far_apart_changes_are_shown_as_separate_rectangles_and_the_middle_is_not_sent() {
        for backend in backends() {
            let mut d = Driver::new(buttons_app(1024, 600), Fake::new(1024, 600), backend).unwrap();
            d.step(0).unwrap();
            d.board_mut().shown.clear();
            // A click on the Buttons page changes the button and its counter near the top, and the status line at
            // the bottom of the screen.
            let button = d.shell().app.widgets().click_me.unwrap();
            let r = d.shell().app.rect_of(button).unwrap();
            let p = Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32);
            d.input(Input::Pointer(fusuma::Pointer::Down(p)));
            d.input(Input::Pointer(fusuma::Pointer::Up(p)));
            let step = d.step(16).unwrap();
            let shown = d.board().shown.clone();
            let bounding = step.shown.unwrap();
            assert!(shown.len() >= 2, "{backend:?}: {shown:?}");
            let sent: u32 = shown.iter().map(|r| r.w * r.h).sum();
            assert!(
                sent * 2 < bounding.w * bounding.h,
                "{backend:?}: sent {sent} px of {}",
                bounding.w * bounding.h
            );
            for r in &shown {
                assert!(
                    bounding.intersect(r) == *r,
                    "{backend:?}: {r:?} inside {bounding:?}"
                );
            }
        }
    }

    /// A press held on the card of the Navigation page: nothing moves while it waits to become a long press, so the
    /// runtime is told when to wake (and may sleep until then), and the wake comes true.
    #[test]
    fn a_held_press_sleeps_until_its_deadline_and_then_fires() {
        for backend in backends() {
            let mut app = Showcase::new(1024, 600);
            app.set_start(Some("Navigation"), None, None, true, false);
            let mut d = Driver::new(app, Fake::new(1024, 600), backend).unwrap();
            d.step(0).unwrap();
            d.step(100).unwrap();
            assert_eq!(
                d.step(200).unwrap().wake,
                fusuma::Wake::Never,
                "{backend:?}: idle"
            );
            let card = d.shell().app.widgets().card.unwrap();
            let r = d.shell().app.rect_of(card).unwrap();
            let p = Point::new(r.x + 20, r.y + (r.h / 2) as i32);
            d.input(Input::Pointer(fusuma::Pointer::Down(p)));
            d.step(300).unwrap();
            let held = d.step(332).unwrap();
            assert!(
                !held.wants_frame,
                "{backend:?}: nothing moves while it is timed"
            );
            let fusuma::Wake::At(ms) = held.wake else {
                panic!("{backend:?}: expected a deadline, got {:?}", held.wake);
            };
            assert!(
                (300..=500).contains(&ms),
                "{backend:?}: about half a second to go: {ms}"
            );
            // The runtime sleeps until the deadline and runs a frame: the long press fires, with no frames between.
            let at = 332 + u64::from(ms);
            d.resume(at);
            d.step(at).unwrap();
            assert!(
                d.shell().app.toast_shown(),
                "{backend:?}: the long press showed its toast"
            );
        }
    }

    #[test]
    fn each_stage_is_timed_when_the_board_has_a_clock_and_only_then() {
        for backend in backends() {
            let mut board = Fake::new(320, 200);
            board.clock = Some(core::cell::Cell::new(0));
            let mut d = Driver::new(buttons_app(320, 200), board, backend).unwrap();
            let step = d.step(0).unwrap();
            assert!(step.shown.is_some());
            // The fake clock moves 10 us a reading, so each stage between two readings is 10 us.
            assert_eq!(
                step.timing,
                Some(Timing {
                    input_us: 10,
                    draw_us: 10,
                    raster_us: 10,
                    present_us: 10
                }),
                "{backend:?}"
            );
            let idle = d.step(16).unwrap();
            assert_eq!(
                (idle.shown, idle.timing),
                (None, None),
                "nothing shown, nothing timed"
            );
            let mut plain = driver(backend);
            assert_eq!(plain.step(0).unwrap().timing, None, "no clock");
        }
    }

    fn backends() -> Vec<Backend> {
        crate::LINKED.to_vec()
    }

    /// The families decide how text looks: with none every role is the bitmap font (one bit a pixel), with the
    /// default set the integer renderer blends its atlases (pixels in between).
    #[cfg(feature = "fonts")]
    #[test]
    fn the_font_set_decides_how_text_is_drawn() {
        let partial = |set: fusuma::fonts::FontSet| {
            let mut d = wide(Backend::Integer);
            d.set_fonts(set);
            d.step(0).unwrap();
            let lit = |p: &u16| *p != 0 && *p != 0xffff;
            let screen = &d.board().screen;
            let colours: std::collections::BTreeSet<u16> =
                screen.iter().copied().filter(lit).collect();
            colours.len()
        };
        let (smooth, bitmap) = (
            partial(fusuma::fonts::FontSet::default_set()),
            partial(fusuma::fonts::FontSet::NONE),
        );
        assert!(
            smooth > bitmap + 20,
            "{smooth} colours with atlases, {bitmap} without"
        );
    }

    /// The showcase on its Buttons page, which has the widgets these tests press.
    fn buttons_app(w: u32, h: u32) -> Showcase {
        let mut app = Showcase::new(w, h);
        app.set_start(Some("Buttons"), None, None, true, false);
        app
    }

    fn driver(backend: Backend) -> Driver<Showcase, Fake> {
        Driver::new(buttons_app(320, 200), Fake::new(320, 200), backend).unwrap()
    }

    /// A screen big enough to lay the showcase's page out (its sidebar alone is 240 px wide).
    fn wide(backend: Backend) -> Driver<Showcase, Fake> {
        Driver::new(buttons_app(1024, 600), Fake::new(1024, 600), backend).unwrap()
    }

    #[test]
    fn the_first_step_shows_the_whole_screen_and_the_second_nothing() {
        for b in backends() {
            let mut d = driver(b);
            let s = d.step(0).unwrap();
            assert_eq!(s.shown, Some(Rect::new(0, 0, 320, 200)), "{b:?}");
            let bg = d.shell().app.theme().colors.background;
            assert_eq!(
                d.board().at(0, 0),
                rgb565(bg.r, bg.g, bg.b),
                "{b:?}: the page colour reached the panel"
            );
            let s = d.step(16).unwrap();
            assert_eq!(s.shown, None, "{b:?}: nothing changed");
            assert!(!s.wants_frame, "{b:?}: so the runtime can sleep");
        }
    }

    #[test]
    fn every_backend_puts_the_same_colours_on_the_panel() {
        let mut first: Option<Vec<u16>> = None;
        for b in backends() {
            let mut d = driver(b);
            d.step(0).unwrap();
            let got = d.board().screen.clone();
            if let Some(reference) = &first {
                // Anti-aliased edges and text differ between renderers; the flat areas must agree.
                let same = got.iter().zip(reference).filter(|(a, b)| a == b).count();
                assert!(
                    same * 100 >= got.len() * 80,
                    "{b:?}: only {same} of {} pixels agree",
                    got.len()
                );
            } else {
                first = Some(got);
            }
        }
    }

    #[test]
    fn a_tap_on_the_panel_clicks_a_widget_and_only_the_changed_part_is_shown() {
        for b in backends() {
            let mut d = driver(b);
            d.step(0).unwrap();
            // The theme dropdown: one tap opens its menu, a second on its Light row switches the theme.
            let tap = |d: &mut Driver<Showcase, Fake>, id, t0: u64| {
                let r = d.shell().app.rect_of(id).unwrap();
                let finger = Point::new(r.x + 3, r.y + 3);
                d.board
                    .touches
                    .extend([Ok(Some(finger)), Ok(Some(finger)), Ok(None)]);
                let mut last = None;
                for t in 1..=3u64 {
                    last = d.step((t0 + t) * 16).unwrap().shown.or(last);
                }
                last
            };
            let theme_button = d.shell().app.theme_button().unwrap();
            let opened = tap(&mut d, theme_button, 0).unwrap();
            assert!(
                opened.w * opened.h < 320 * 200,
                "{b:?}: opening a menu repaints only the menu, not {opened:?}"
            );
            let light = d.shell().app.theme_row(1).unwrap();
            let last = tap(&mut d, light, 10);
            assert!(
                !d.shell().app.is_dark(),
                "{b:?}: the tap switched the theme"
            );
            assert_eq!(
                last,
                Some(Rect::new(0, 0, 320, 200)),
                "{b:?}: a theme change repaints everything"
            );
            let bg = d.shell().app.theme().colors.background;
            assert_eq!(
                d.board().at(0, 0),
                rgb565(bg.r, bg.g, bg.b),
                "{b:?}: and the panel shows the new colour"
            );
        }
    }

    #[test]
    fn a_hover_shows_a_small_region_not_the_screen() {
        let mut d = wide(Backend::Integer);
        d.step(0).unwrap();
        let id = d.shell().app.widgets().click_me.unwrap();
        let r = d.shell().app.rect_of(id).unwrap();
        d.board
            .touches
            .push_back(Ok(Some(Point::new(r.x + 2, r.y + 2))));
        let before = d.board.shown.len();
        let s = d.step(16).unwrap();
        let region = s.shown.unwrap();
        assert!(
            u64::from(region.w) * u64::from(region.h) < 320 * 200 / 2,
            "{region:?}"
        );
        assert_eq!(d.board.shown.len(), before + 1);
    }

    #[test]
    fn a_panel_that_takes_a_few_pixels_at_a_time_gets_the_same_picture() {
        let whole = {
            let mut d = driver(Backend::Integer);
            d.step(0).unwrap();
            d.board.screen.clone()
        };
        for chunk in [1, 3, 7, 64] {
            let mut d = Driver::new(
                buttons_app(320, 200),
                {
                    let mut f = Fake::new(320, 200);
                    f.chunk = chunk;
                    f
                },
                Backend::Integer,
            )
            .unwrap();
            d.step(0).unwrap();
            assert_eq!(d.board.screen, whole, "chunks of {chunk}");
        }
    }

    #[test]
    fn a_failing_touch_panel_is_a_lifted_finger_and_a_failing_screen_is_an_error() {
        let mut d = wide(Backend::Integer);
        d.step(0).unwrap();
        let id = d.shell().app.widgets().click_me.unwrap();
        let r = d.shell().app.rect_of(id).unwrap();
        let p = Point::new(r.x + 2, r.y + 2);
        d.board.touches.extend([Ok(Some(p)), Err(()), Ok(None)]);
        for t in 1..=3u64 {
            d.step(t * 16).unwrap();
        }
        assert_eq!(
            d.shell().app.clicks(),
            1,
            "down, then an error counts as the lift: a click, not a stuck press"
        );
        d.board.fail_present = true;
        d.shell_mut().input(Input::Key(fusuma::Key::Next));
        assert_eq!(d.step(99).unwrap_err(), DriverError::Board(()));
    }

    #[test]
    fn inputs_from_outside_the_panel_and_a_resume_work_and_a_backend_that_is_not_linked_makes_no_driver()
     {
        let mut d = driver(Backend::Integer);
        d.step(0).unwrap();
        d.input(Input::Key(fusuma::Key::Next));
        assert!(d.wants_frame());
        d.resume(10_000);
        assert!(
            d.step(10_000).unwrap().shown.is_some(),
            "the focus ring is drawn"
        );
        assert!(Driver::new(Showcase::new(10, 10), Fake::new(10, 10), Backend::VelloGpu).is_none());
    }

    #[test]
    fn the_boards_shape_reaches_the_app_before_the_first_frame() {
        let mut board = Fake::new(410, 502);
        board.shape = fusuma::Shape::Rounded(60);
        let mut round = Driver::new(Showcase::new(410, 502), board, Backend::Integer).unwrap();
        let plain = Driver::new(
            Showcase::new(410, 502),
            Fake::new(410, 502),
            Backend::Integer,
        )
        .unwrap();
        round.step(0).unwrap();
        let (a, b) = (
            round.shell().app.theme_button().unwrap(),
            plain.shell().app.theme_button().unwrap(),
        );
        let (ra, rb) = (round.shell().app.rect_of(a), plain.shell().app.rect_of(b));
        assert_ne!(ra, rb, "the page was laid out inside the corners");
    }

    #[test]
    fn what_the_board_says_the_device_is_reaches_the_app_and_a_plain_board_says_nothing() {
        let eink = fusuma::Screen::new(296, 128)
            .with_form_factor(fusuma::FormFactor::Micro)
            .with_glass(fusuma::DisplayKind::Reflective, 1, 300);
        let mut board = Fake::new(296, 128);
        board.facts = Some(eink);
        let told = Driver::new(Showcase::new(296, 128), board, Backend::Integer).unwrap();
        assert_eq!(told.shell().app.device(), eink);
        let plain = Driver::new(
            Showcase::new(296, 128),
            Fake::new(296, 128),
            Backend::Integer,
        )
        .unwrap();
        assert!(!plain.shell().app.device().is_reflective());
        assert_eq!(plain.shell().app.device().form, None, "nothing was sent");
    }
}
