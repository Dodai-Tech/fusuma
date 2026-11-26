//! Waveshare's four-colour "G" e-paper modules (the 1.54 inch 200x200 in the ESP32-S3-ePaper-1.54G): black, white, yellow and
//! red, two bits a pixel, over SPI. Waveshare's repository does not name the controller (its command set is of the UC81xx
//! kind: panel setting `0x00`, booster `0x06`, resolution `0x61`, power `0x04`/`0x02`, refresh `0x12`, sleep `0x07`); this
//! module is named for the product line.
//!
//! The sequences are the ones of Waveshare's ESP-IDF example (`epaper_port.c`: `epaper_port_init`, `epaper_port_display` and
//! `epaper_port_sleep`), read from its source; the tests run them against a recording bus, and they have **not** been checked on a
//! panel. What this panel is:
//!
//! * a picture is **one** plane of two bits a pixel, four pixels to a byte with the first in the top bits, written with `0x10`;
//!   the codes are 0 black, 1 white, 2 yellow, 3 red ([`COLORS`]);
//! * a refresh (`0x12`) takes seconds and there is **no partial one**: every update is a whole picture. The driver therefore powers
//!   the panel off and puts it to sleep after each one, as the vendor's example does, and runs the whole init again for the next;
//! * the busy line is **high** when the chip is idle ([`Bus::wait_busy_low`]); unlike the UC8179 the vendor does not ask the
//!   chip's status first.
//!
//! Drive it with [`EinkDriver::new_color4`](super::EinkDriver::new_color4): the picture is drawn in colour and quantised to the four
//! inks. Used as a plain black and white [`Panel`] it shows [`Panel::full`]'s picture in black and white.

use fusuma::render::Rect;

use super::bus::Bus;
use super::{Panel, row_bytes};

mod cmd {
    pub const PANEL_SETTING: u8 = 0x00;
    pub const POWER_OFF: u8 = 0x02;
    pub const POWER_ON: u8 = 0x04;
    pub const BOOSTER: u8 = 0x06;
    pub const DEEP_SLEEP: u8 = 0x07;
    pub const DATA: u8 = 0x10;
    pub const REFRESH: u8 = 0x12;
    pub const PLL: u8 = 0x30;
    pub const VCOM_DATA: u8 = 0x50;
    pub const RESOLUTION: u8 = 0x61;
    pub const UNDOCUMENTED_4D: u8 = 0x4D;
    pub const UNDOCUMENTED_E9: u8 = 0xE9;
}

/// The colours the two-bit codes show, in code order (black, white, yellow, red), as RGB565: what the inks look like to the
/// quantiser, not what the glass is exactly.
pub const COLORS: [u16; 4] = [0x0000, 0xFFFF, 0xFFE0, 0xF800];

/// The panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

impl Config {
    /// Waveshare's 1.54 inch four-colour panel, 200x200.
    pub const EPD_1IN54G: Self = Self {
        width: 200,
        height: 200,
    };
}

/// A four-colour panel on a [`Bus`].
#[derive(Debug)]
pub struct Epd4g<B> {
    bus: B,
    config: Config,
}

impl<B: Bus> Epd4g<B> {
    /// A panel of `config`'s size on `bus`.
    pub const fn new(bus: B, config: Config) -> Self {
        Self { bus, config }
    }

    /// The bus, for a test to read what was sent.
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    fn send(&mut self, command: u8, data: &[u8]) -> Result<(), B::Error> {
        self.bus.command(command)?;
        if data.is_empty() {
            Ok(())
        } else {
            self.bus.data(data)
        }
    }

    /// Resets the chip and runs Waveshare's init.
    fn wake(&mut self) -> Result<(), B::Error> {
        self.bus.reset()?;
        self.send(cmd::UNDOCUMENTED_4D, &[0x78])?;
        self.send(cmd::PANEL_SETTING, &[0x0F, 0x29])?;
        self.send(cmd::BOOSTER, &[0x0D, 0x12, 0x30, 0x20, 0x19, 0x2A, 0x22])?;
        self.send(cmd::VCOM_DATA, &[0x37])?;
        let (w, h) = (self.config.width, self.config.height);
        self.send(
            cmd::RESOLUTION,
            &[(w >> 8) as u8, w as u8, (h >> 8) as u8, h as u8],
        )?;
        self.send(cmd::UNDOCUMENTED_E9, &[0x01])?;
        self.send(cmd::PLL, &[0x08])?;
        self.send(cmd::POWER_ON, &[])?;
        self.bus.wait_busy_low()
    }

    /// Writes a picture of two-bit codes (four pixels to a byte) and refreshes, then powers the panel off.
    ///
    /// # Errors
    /// The bus error.
    pub fn show(&mut self, codes: &[u8]) -> Result<(), B::Error> {
        self.wake()?;
        self.send(cmd::DATA, codes)?;
        self.send(cmd::REFRESH, &[0x00])?;
        self.bus.wait_busy_low()?;
        self.power_down()
    }

    /// Powers off and deep-sleeps: the picture stays, with no power.
    fn power_down(&mut self) -> Result<(), B::Error> {
        self.send(cmd::POWER_OFF, &[0x00])?;
        self.bus.wait_busy_low()?;
        self.send(cmd::DEEP_SLEEP, &[0xA5])
    }
}

/// One byte of a one-bit picture (eight pixels, a set bit white) as two bytes of codes: a clear bit is black (0), a set one white (1).
const fn widen(byte: u8) -> [u8; 2] {
    let mut out = [0u8; 2];
    let mut i = 0;
    while i < 8 {
        let bit = (byte >> (7 - i)) & 1;
        out[i / 4] |= bit << (6 - 2 * (i % 4));
        i += 1;
    }
    out
}

impl<B: Bus> Panel for Epd4g<B> {
    type Error = B::Error;

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), B::Error> {
        let mut codes = alloc::vec::Vec::with_capacity(frame.len() * 2);
        for &b in frame {
            codes.extend_from_slice(&widen(b));
        }
        self.show(&codes)
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        // The panel cannot refresh part of itself. A driver made for it ([`new_color4`](super::EinkDriver::new_color4)) never asks;
        // asked anyway, there is no whole picture here to show, so nothing is sent.
        let _ = (region, data, row_bytes(region));
        Ok(())
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        // Already asleep after every refresh.
        Ok(())
    }

    fn gray4(&self) -> bool {
        true
    }

    fn full_gray4(&mut self, levels: &[u8]) -> Result<(), B::Error> {
        // The codes as the driver packed them for this panel's colours ([`Panel::colors4`]).
        self.show(levels)
    }

    fn colors4(&self) -> Option<[u16; 4]> {
        Some(COLORS)
    }

    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        // Several seconds to a refresh; two bits a pixel is what the app can count on (four inks).
        fusuma::Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Micro)
            .with_glass(fusuma::DisplayKind::Reflective, 2, 15_000)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::super::bus::{Call, Recorder};
    use super::*;

    fn commands(log: &[Call]) -> Vec<u8> {
        log.iter()
            .filter_map(|c| match c {
                Call::Command(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_picture_is_init_data_refresh_and_sleep() {
        let mut p = Epd4g::new(Recorder::default(), Config::EPD_1IN54G);
        p.show(&[0x55; 4]).unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(log[0], Call::Reset);
        assert_eq!(
            commands(log),
            [
                0x4D, 0x00, 0x06, 0x50, 0x61, 0xE9, 0x30, 0x04, 0x10, 0x12, 0x02, 0x07
            ]
        );
        // The resolution is 200 x 200, big-endian; the data is the picture as given; the busy line is waited for after
        // power-on, the refresh and the power-off.
        assert!(log.contains(&Call::Data(vec![0, 200, 0, 200])));
        assert!(log.contains(&Call::Data(vec![0x55; 4])));
        assert_eq!(log.iter().filter(|c| **c == Call::BusyLow).count(), 3);
        assert_eq!(log.last(), Some(&Call::Data(vec![0xA5])));
    }

    #[test]
    fn every_picture_wakes_the_panel_again() {
        let mut p = Epd4g::new(Recorder::default(), Config::EPD_1IN54G);
        p.show(&[0; 4]).unwrap();
        p.show(&[0; 4]).unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(log.iter().filter(|c| **c == Call::Reset).count(), 2);
    }

    #[test]
    fn a_one_bit_picture_is_black_and_white_codes() {
        // 8 pixels: white, black, black, white | white, white, black, black -> codes 01 00 00 01 | 01 01 00 00.
        assert_eq!(widen(0b1001_1100), [0b01_00_00_01, 0b01_01_00_00]);
        assert_eq!(widen(0xFF), [0x55, 0x55]);
        assert_eq!(widen(0x00), [0, 0]);
        let mut p = Epd4g::new(Recorder::default(), Config::EPD_1IN54G);
        p.full(&[0xFF, 0x00]).unwrap();
        assert!(
            p.bus_mut()
                .log
                .contains(&Call::Data(vec![0x55, 0x55, 0, 0]))
        );
    }

    #[test]
    fn the_panel_says_it_has_four_inks_and_cannot_refresh_a_part() {
        let mut p = Epd4g::new(Recorder::default(), Config::EPD_1IN54G);
        assert!(p.gray4());
        assert_eq!(p.colors4(), Some(COLORS));
        p.partial(Rect::new(0, 0, 8, 1), &[0]).unwrap();
        assert!(p.bus_mut().log.is_empty());
        let s = p.screen();
        assert_eq!((s.depth_bits, s.refresh_ms), (2, 15_000));
    }
}
