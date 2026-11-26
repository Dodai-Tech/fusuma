//! The board as a `fusuma_platform_embedded::Board`, made of the parts its board file names: the display controller,
//! the touch controller (turned to the screen) and the backlight. Nothing here knows a particular board.

use std::convert::Infallible;

use fusuma::render::{Point, Rect};
use fusuma_platform_embedded::{Board, HeldTouch, Pixels, orient};

use crate::board::{HEIGHT, SCREEN, SHAPE, TOUCH_MIRROR, TOUCH_SWAP_XY, WIDTH};
use crate::display::Display;
use crate::{backlight, touch};

/// The board this firmware was built for.
pub(crate) struct EspBoard {
    display: Display,
    touch: bool,
    held: HeldTouch,
}

impl EspBoard {
    /// Brings every part up, at `percent` brightness (0 to 100).
    pub(crate) fn open(percent: i32) -> anyhow::Result<Self> {
        backlight::init();
        let mut display = Display::open()?;
        let touch = touch::init();
        if !display.brightness(percent) {
            backlight::set(percent);
        }
        Ok(Self {
            display,
            touch,
            held: HeldTouch::default(),
        })
    }

    /// The parts' own numbers since the last report, as a log line.
    pub(crate) fn report(&mut self) -> Option<String> {
        let display = self.display.report();
        let touch = format!("touch: {} failed reads", self.held.errors);
        Some(display.map_or(touch.clone(), |d| format!("{d}; {touch}")))
    }
}

impl Board for EspBoard {
    type Error = Infallible;

    fn size(&self) -> (u32, u32) {
        (WIDTH, HEIGHT)
    }

    fn shape(&self) -> fusuma::Shape {
        SHAPE
    }

    fn screen(&self) -> fusuma::Screen {
        SCREEN
    }

    fn touch(&mut self) -> Result<Option<Point>, Infallible> {
        if !self.touch {
            return Ok(None);
        }
        let read = touch::read().map(|(down, x, y)| {
            down.then(|| orient(x, y, WIDTH, HEIGHT, TOUCH_SWAP_XY, TOUCH_MIRROR))
        });
        Ok(self.held.sample(read))
    }

    fn now_us(&self) -> Option<u64> {
        // SAFETY: a plain getter.
        Some(unsafe { esp_idf_svc::sys::esp_timer_get_time() } as u64)
    }

    fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) -> Result<(), Infallible> {
        self.display.present(region, pixels);
        Ok(())
    }
}
