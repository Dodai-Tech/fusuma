//! Waveshare's 13.3 inch six-colour e-paper (the ESP32-S3-ePaper-13.3E6): 1200x1600 pixels in black, white, yellow, red, blue and
//! green, four bits a pixel, over SPI. The panel is driven by **two** controllers, a master for the left 600 columns and a slave for
//! the right 600, each with its own chip select and sharing the clock, data, data/command, reset and busy lines.
//!
//! The sequences are the ones of Waveshare's ESP-IDF example (`epaper_port.c`: `EPD_Init`, `EPD_Display`, `EPD_TurnOnDisplay`, `EPD_Sleep`),
//! read from its source; the tests run them against a recording link, and they have **not** been checked on a panel. What this panel is:
//!
//! * a picture is two planes of 300 bytes a row, one for each controller: the master takes the left half of every row, the slave the right
//!   half, each written under its own chip select with `0x10`. A pixel is a four-bit code ([`INKS`]: 0 black, 1 white, 2 yellow, 3 red,
//!   5 blue, 6 green; 4 is not used), the first pixel of a byte in its high nibble;
//! * a refresh takes tens of seconds and there is **no partial one**. The driver powers the panel up, sets it up, sends the picture, runs
//!   the refresh and puts the panel to deep sleep and its power off, every time, as the vendor's example does;
//! * the busy line is **high** when the chip is idle; the init sets some registers on both controllers and some on the master only.
//!
//! The bus does not fit [`Bus`](super::bus::Bus): a command and its data are one chip-select frame, and the chip select is one of
//! two (or both). [`Link`] is that bus. Drive the panel with [`EinkDriver::new_color`](super::EinkDriver::new_color): the picture is drawn in
//! colour and quantised to the six inks. It needs the colour surface, 3.8 MB for 1200x1600, so PSRAM.

use alloc::vec::Vec;

use fusuma::render::Rect;

use super::{Palette, Panel};

/// Which of the two controllers a frame is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    /// The master: the left half of the picture; the one that holds the power registers.
    Master,
    /// The slave: the right half.
    Slave,
    /// Both at once, for the registers they share.
    Both,
}

/// The bus of a two-controller panel.
pub trait Link {
    /// What goes wrong talking to the panel.
    type Error: core::fmt::Debug;

    /// Switches the panel's power rail.
    ///
    /// # Errors
    /// The pin's error.
    fn power(&mut self, on: bool) -> Result<(), Self::Error>;

    /// Pulses the reset line the way the vendor does: high, low, high, low, high, 30 ms each.
    ///
    /// # Errors
    /// The pin's error.
    fn reset(&mut self) -> Result<(), Self::Error>;

    /// Starts a frame: selects `chips`, then sends `command` with the data/command line low.
    ///
    /// # Errors
    /// The bus error.
    fn begin(&mut self, chips: Chip, command: u8) -> Result<(), Self::Error>;

    /// Sends data bytes (the data/command line high) in the frame that [`begin`](Self::begin) started.
    ///
    /// # Errors
    /// The bus error.
    fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error>;

    /// Ends the frame: deselects the chips.
    ///
    /// # Errors
    /// The bus error.
    fn end(&mut self) -> Result<(), Self::Error>;

    /// Waits until the panel is idle: the busy line is high.
    ///
    /// # Errors
    /// A timeout, or the pin's error.
    fn wait_idle(&mut self) -> Result<(), Self::Error>;

    /// Waits `ms` milliseconds.
    ///
    /// # Errors
    /// None for a real link.
    fn delay_ms(&mut self, ms: u32) -> Result<(), Self::Error>;
}

mod cmd {
    pub const PANEL_SETTING: u8 = 0x00;
    pub const POWER_SETTING: u8 = 0x01;
    pub const POWER_OFF: u8 = 0x02;
    pub const POWER_ON: u8 = 0x04;
    pub const BOOSTER_N: u8 = 0x05;
    pub const BOOSTER_P: u8 = 0x06;
    pub const DEEP_SLEEP: u8 = 0x07;
    pub const DATA: u8 = 0x10;
    pub const REFRESH: u8 = 0x12;
    pub const VCOM_DATA: u8 = 0x50;
    pub const TCON: u8 = 0x60;
    pub const RESOLUTION: u8 = 0x61;
    pub const ANALOG_TEST: u8 = 0x74;
    pub const AGID: u8 = 0x86;
    pub const BUCK_BOOST_VDDN: u8 = 0xB0;
    pub const TFT_VCOM_POWER: u8 = 0xB1;
    pub const ENABLE_BUFFER: u8 = 0xB6;
    pub const BOOST_VDDP: u8 = 0xB7;
    pub const CCSET: u8 = 0xE0;
    pub const PWS: u8 = 0xE3;
    pub const COMMAND_66: u8 = 0xF0;
}

/// The inks: the six of `SPECTRA6`.
pub const INKS: Palette = super::SPECTRA6;

/// The panel: 1200x1600, a controller to each 600 columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across (both controllers).
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

impl Config {
    /// Waveshare's 13.3 inch six-colour panel.
    pub const EPD_13IN3E: Self = Self {
        width: 1200,
        height: 1600,
    };
}

/// A two-controller six-colour panel on a [`Link`].
#[derive(Debug)]
pub struct Epd13e6<L> {
    link: L,
    config: Config,
}

impl<L: Link> Epd13e6<L> {
    /// A panel of `config`'s size on `link`.
    pub const fn new(link: L, config: Config) -> Self {
        Self { link, config }
    }

    /// The link, for a test to read what was sent.
    pub fn link_mut(&mut self) -> &mut L {
        &mut self.link
    }

    /// A whole frame: `command` and `data` to `chips`.
    fn frame(&mut self, chips: Chip, command: u8, data: &[u8]) -> Result<(), L::Error> {
        self.link.begin(chips, command)?;
        if !data.is_empty() {
            self.link.data(data)?;
        }
        self.link.end()
    }

    /// Powers the panel and sets both controllers up, as `EPD_Init` does.
    fn wake(&mut self) -> Result<(), L::Error> {
        self.link.power(true)?;
        self.link.delay_ms(10)?;
        self.link.reset()?;
        self.link.wait_idle()?;
        self.frame(
            Chip::Master,
            cmd::ANALOG_TEST,
            &[0xC0, 0x1C, 0x1C, 0xCC, 0xCC, 0xCC, 0x15, 0x15, 0x55],
        )?;
        // What both controllers need to agree on.
        self.frame(
            Chip::Both,
            cmd::COMMAND_66,
            &[0x49, 0x55, 0x13, 0x5D, 0x05, 0x10],
        )?;
        self.frame(Chip::Both, cmd::PANEL_SETTING, &[0xDF, 0x69])?;
        self.frame(Chip::Both, cmd::VCOM_DATA, &[0xF7])?;
        self.frame(Chip::Both, cmd::TCON, &[0x03, 0x03])?;
        self.frame(Chip::Both, cmd::AGID, &[0x10])?;
        self.frame(Chip::Both, cmd::PWS, &[0x22])?;
        self.frame(Chip::Both, cmd::CCSET, &[0x01])?;
        // 0x04B0 x 0x0320, as the vendor sends it (1200 x 800: not the panel's 1200 x 1600, and not explained in the example).
        self.frame(Chip::Both, cmd::RESOLUTION, &[0x04, 0xB0, 0x03, 0x20])?;
        // The power tree is the master's.
        self.frame(
            Chip::Master,
            cmd::POWER_SETTING,
            &[0x0F, 0x00, 0x28, 0x2C, 0x28, 0x38],
        )?;
        self.frame(Chip::Master, cmd::ENABLE_BUFFER, &[0x07])?;
        self.frame(Chip::Master, cmd::BOOSTER_P, &[0xE8, 0x28])?;
        self.frame(Chip::Master, cmd::BOOST_VDDP, &[0x01])?;
        self.frame(Chip::Master, cmd::BOOSTER_N, &[0xE8, 0x28])?;
        self.frame(Chip::Master, cmd::BUCK_BOOST_VDDN, &[0x01])?;
        self.frame(Chip::Master, cmd::TFT_VCOM_POWER, &[0x02])
    }

    /// Writes a picture of four-bit codes (two pixels to a byte, rows of `width / 2` bytes) and refreshes, then puts the panel to sleep.
    ///
    /// # Errors
    /// The link's error.
    pub fn show(&mut self, codes: &[u8]) -> Result<(), L::Error> {
        let row = self.config.width as usize / 2;
        let half = row / 2;
        self.wake()?;
        // The left half of every row to the master, the right half to the slave.
        let mut plane: Vec<u8> = Vec::with_capacity(half * self.config.height as usize);
        for (chip, from) in [(Chip::Master, 0), (Chip::Slave, half)] {
            plane.clear();
            for line in codes.chunks_exact(row) {
                plane.extend_from_slice(&line[from..from + half]);
            }
            self.frame(chip, cmd::DATA, &plane)?;
        }
        // Turn on: power up, refresh, power down, as `EPD_TurnOnDisplay` does.
        self.frame(Chip::Both, cmd::POWER_ON, &[])?;
        self.link.wait_idle()?;
        self.link.delay_ms(50)?;
        self.frame(Chip::Both, cmd::REFRESH, &[0x00])?;
        self.link.wait_idle()?;
        self.link.delay_ms(50)?;
        self.frame(Chip::Both, cmd::POWER_OFF, &[0x00])?;
        self.sleep_now()
    }

    /// Deep sleep and power off, as `EPD_Sleep` does: the picture stays, with no power.
    fn sleep_now(&mut self) -> Result<(), L::Error> {
        self.frame(Chip::Both, cmd::DEEP_SLEEP, &[0xA5])?;
        self.link.delay_ms(100)?;
        self.link.power(false)
    }
}

/// One byte of a one-bit picture (eight pixels, a set bit white) as four bytes of codes: a clear bit is black (0), a set one white (1).
const fn widen(byte: u8) -> [u8; 4] {
    let mut out = [0u8; 4];
    let mut i = 0;
    while i < 8 {
        let bit = (byte >> (7 - i)) & 1;
        out[i / 2] |= bit << (4 * (1 - i % 2));
        i += 1;
    }
    out
}

impl<L: Link> Panel for Epd13e6<L> {
    type Error = L::Error;

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), L::Error> {
        let mut codes = Vec::with_capacity(frame.len() * 4);
        for &b in frame {
            codes.extend_from_slice(&widen(b));
        }
        self.show(&codes)
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), L::Error> {
        // The panel cannot refresh part of itself. A driver made for it ([`new_color`](super::EinkDriver::new_color)) never asks; asked
        // anyway, there is no whole picture here to show, so nothing is sent.
        let _ = (region, data);
        Ok(())
    }

    fn sleep(&mut self) -> Result<(), L::Error> {
        // Asleep after every refresh.
        Ok(())
    }

    fn palette(&self) -> Option<Palette> {
        Some(INKS)
    }

    fn full_color(&mut self, packed: &[u8]) -> Result<(), L::Error> {
        self.show(packed)
    }

    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        // Tens of seconds to a refresh.
        fusuma::Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Tablet)
            .with_glass(fusuma::DisplayKind::Reflective, 2, 30_000)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;

    /// A link that records what it is asked to do.
    #[derive(Debug, Default)]
    struct Rec {
        log: Vec<Step>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Step {
        Power(bool),
        Reset,
        Begin(Chip, u8),
        Data(Vec<u8>),
        End,
        Idle,
        Delay(u32),
    }

    impl Link for Rec {
        type Error = core::convert::Infallible;

        fn power(&mut self, on: bool) -> Result<(), Self::Error> {
            self.log.push(Step::Power(on));
            Ok(())
        }
        fn reset(&mut self) -> Result<(), Self::Error> {
            self.log.push(Step::Reset);
            Ok(())
        }
        fn begin(&mut self, chips: Chip, command: u8) -> Result<(), Self::Error> {
            self.log.push(Step::Begin(chips, command));
            Ok(())
        }
        fn data(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
            self.log.push(Step::Data(bytes.to_vec()));
            Ok(())
        }
        fn end(&mut self) -> Result<(), Self::Error> {
            self.log.push(Step::End);
            Ok(())
        }
        fn wait_idle(&mut self) -> Result<(), Self::Error> {
            self.log.push(Step::Idle);
            Ok(())
        }
        fn delay_ms(&mut self, ms: u32) -> Result<(), Self::Error> {
            self.log.push(Step::Delay(ms));
            Ok(())
        }
    }

    fn frames(log: &[Step]) -> Vec<(Chip, u8)> {
        log.iter()
            .filter_map(|s| match s {
                Step::Begin(c, cmd) => Some((*c, *cmd)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_picture_wakes_the_panel_sets_both_controllers_up_sends_the_halves_refreshes_and_sleeps() {
        // A small panel of 8 x 2 (4 bytes a row, 2 a half) so the planes can be read.
        let mut p = Epd13e6::new(
            Rec::default(),
            Config {
                width: 8,
                height: 2,
            },
        );
        p.show(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let log = &p.link_mut().log;
        assert_eq!(
            &log[..4],
            [Step::Power(true), Step::Delay(10), Step::Reset, Step::Idle]
        );
        let f = frames(log);
        // The analog test register on the master, then the shared registers on both, then the master's power tree.
        assert_eq!(
            f[..8],
            [
                (Chip::Master, 0x74),
                (Chip::Both, 0xF0),
                (Chip::Both, 0x00),
                (Chip::Both, 0x50),
                (Chip::Both, 0x60),
                (Chip::Both, 0x86),
                (Chip::Both, 0xE3),
                (Chip::Both, 0xE0),
            ]
        );
        assert_eq!(
            f[8..16],
            [
                (Chip::Both, 0x61),
                (Chip::Master, 0x01),
                (Chip::Master, 0xB6),
                (Chip::Master, 0x06),
                (Chip::Master, 0xB7),
                (Chip::Master, 0x05),
                (Chip::Master, 0xB0),
                (Chip::Master, 0xB1),
            ]
        );
        // The picture: the left half of each row to the master, the right half to the slave.
        assert_eq!(f[16], (Chip::Master, 0x10));
        assert_eq!(f[17], (Chip::Slave, 0x10));
        assert!(log.contains(&Step::Data(vec![1, 2, 5, 6])));
        assert!(log.contains(&Step::Data(vec![3, 4, 7, 8])));
        // Power on, refresh and off on both, then deep sleep and the rail off.
        assert_eq!(
            f[18..],
            [
                (Chip::Both, 0x04),
                (Chip::Both, 0x12),
                (Chip::Both, 0x02),
                (Chip::Both, 0x07)
            ]
        );
        assert_eq!(
            &log[log.len() - 2..],
            [Step::Delay(100), Step::Power(false)]
        );
        // The busy line is waited for after the reset, the power-on and the refresh.
        assert_eq!(log.iter().filter(|s| **s == Step::Idle).count(), 3);
        // Every frame is ended.
        let begins = log.iter().filter(|s| matches!(s, Step::Begin(..))).count();
        assert_eq!(log.iter().filter(|s| **s == Step::End).count(), begins);
    }

    #[test]
    fn a_one_bit_picture_is_black_and_white_codes() {
        // 8 pixels: white, black, black, white | white, white, black, black -> codes 1 0 0 1 | 1 1 0 0.
        assert_eq!(widen(0b1001_1100), [0x10, 0x01, 0x11, 0x00]);
        assert_eq!(widen(0xFF), [0x11; 4]);
        assert_eq!(widen(0x00), [0; 4]);
    }

    #[test]
    fn the_panel_says_it_has_six_inks_and_cannot_refresh_a_part() {
        let mut p = Epd13e6::new(Rec::default(), Config::EPD_13IN3E);
        assert_eq!(p.palette(), Some(INKS));
        p.partial(Rect::new(0, 0, 8, 1), &[0]).unwrap();
        assert!(p.link_mut().log.is_empty());
        let s = p.screen();
        assert_eq!((s.width, s.height, s.refresh_ms), (1200, 1600, 30_000));
    }
}
