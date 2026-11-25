//! A scripted run for measuring a board with nobody touching it: a few seconds of wheel scrolling down, then up,
//! then taps, over and over. Built with the `demo` feature only.

use fusuma::render::Point;
use fusuma::{App, Input, Pointer};
use fusuma_platform_embedded::{Board, Driver};

/// How often an input is sent, in milliseconds.
const PERIOD_MS: u64 = 60;
/// One round of the script, in milliseconds.
const ROUND_MS: u64 = 10_000;

/// The script's state.
pub struct Demo {
    last: u64,
    centre: Point,
    top: Point,
}

impl Demo {
    /// A script for a `width` x `height` screen.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            last: u64::MAX,
            centre: Point::new((width / 2) as i32, (height / 2) as i32),
            top: Point::new((width / 4) as i32, (height / 6) as i32),
        }
    }

    /// Sends what the script has for time `now_ms`.
    pub fn drive<A: App, B: Board>(&mut self, driver: &mut Driver<A, B>, now_ms: u64) {
        let tick = now_ms / PERIOD_MS;
        if tick == self.last {
            return;
        }
        self.last = tick;
        let at = now_ms % ROUND_MS;
        let wheel = |driver: &mut Driver<A, B>, dy: i32, at: Point| {
            driver.input(Input::Pointer(Pointer::Wheel(at, dy)));
        };
        match at {
            0..2500 => wheel(driver, 14, self.centre),
            3000..5500 => wheel(driver, -14, self.centre),
            // A hover and a tap on the top of the page, a few frames apart.
            6000..6100 => driver.input(Input::Pointer(Pointer::Move(self.top))),
            6500..6600 => driver.input(Input::Pointer(Pointer::Down(self.top))),
            6700..6800 => {
                driver.input(Input::Pointer(Pointer::Up(self.top)));
                driver.input(Input::Pointer(Pointer::Leave));
            }
            // Wheel a bit while a hover is elsewhere.
            7500..9000 => wheel(driver, 9, self.centre),
            _ => {}
        }
    }
}
