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

    /// One step at the monotonic time `now_ms`: the touch panel is read and turned into pointer events, a frame
    /// runs, and the part that changed is shown. A touch panel that fails is treated as a lifted finger, so a
    /// flaky bus cannot leave a button pressed.
    ///
    /// # Errors
    /// [`DriverError`] when drawing or showing the frame failed.
    pub fn step(&mut self, now_ms: u64) -> Result<Step, DriverError<B::Error>> {
        let sample = self.board.touch().unwrap_or(None);
        for event in self.touch.step(sample).into_iter().flatten() {
            self.shell.input(Input::Pointer(event));
        }
        let drawn = self
            .surface
            .frame(&mut self.shell, now_ms)
            .map_err(DriverError::Draw)?;
        let shown = drawn
            .map(|r| r.intersect(&self.screen))
            .filter(|r| r.w > 0 && r.h > 0);
        if let Some(region) = shown {
            self.board
                .present(region, &mut self.surface)
                .map_err(DriverError::Board)?;
        }
        Ok(Step {
            shown,
            wants_frame: self.shell.wants_frame(),
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

        fn touch(&mut self) -> Result<Option<Point>, ()> {
            self.touches.pop_front().unwrap_or(Ok(None))
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

    fn backends() -> Vec<Backend> {
        crate::LINKED.to_vec()
    }

    fn driver(backend: Backend) -> Driver<Showcase, Fake> {
        Driver::new(Showcase::new(320, 200), Fake::new(320, 200), backend).unwrap()
    }

    /// A screen big enough to lay the showcase's page out (its sidebar alone is 240 px wide).
    fn wide(backend: Backend) -> Driver<Showcase, Fake> {
        Driver::new(Showcase::new(1024, 600), Fake::new(1024, 600), backend).unwrap()
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
            let theme_button = d.shell().app.theme_button().unwrap();
            let r = d.shell().app.rect_of(theme_button).unwrap();
            let finger = Point::new(r.x + 3, r.y + 3);
            d.board
                .touches
                .extend([Ok(Some(finger)), Ok(Some(finger)), Ok(None)]);
            let mut last = None;
            for t in 1..=3u64 {
                last = d.step(t * 16).unwrap().shown.or(last);
            }
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
                Showcase::new(320, 200),
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
}
