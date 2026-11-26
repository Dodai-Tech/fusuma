//! Waveshare's 7.3 inch six-colour e-paper (the "E" panel, in the ESP32-S3-PhotoPainter): 800x480 in black, white, yellow, red, blue
//! and green, four bits a pixel, over SPI with one chip select.
//!
//! The sequences are the ones of Waveshare's PhotoPainter firmware (`display_bsp.cpp`: `EPD_Init`, `EPD_Display`,
//! `EPD_TurnOnDisplay`), read from its source; the tests run them against a recording bus, and they have **not** been checked on a
//! panel. What this panel is:
//!
//! * a picture is one plane of four bits a pixel, two pixels to a byte with the first in the high nibble, written with `0x10`; the codes are
//!   `SPECTRA6`'s;
//! * a refresh takes tens of seconds and there is **no partial one**; the driver runs the whole init, the picture, the refresh and the
//!   power-off for every picture, as the vendor's firmware does;
//! * the busy line is **high** when the chip is idle ([`Bus::wait_busy_low`]).
//!
//! Drive it with [`EinkDriver::new_color`](super::EinkDriver::new_color); it needs the colour surface (768 KB), so PSRAM.

use alloc::vec::Vec;

use fusuma::render::Rect;

use super::bus::Bus;
use super::{Palette, Panel, SPECTRA6};

mod cmd {
    pub const POWER_SETTING: u8 = 0x01;
    pub const POWER_OFF: u8 = 0x02;
    pub const POWER_OFF_SEQUENCE: u8 = 0x03;
    pub const POWER_ON: u8 = 0x04;
    pub const BOOSTER_5: u8 = 0x05;
    pub const BOOSTER_6: u8 = 0x06;
    pub const BOOSTER_8: u8 = 0x08;
    pub const PANEL_SETTING: u8 = 0x00;
    pub const DATA: u8 = 0x10;
    pub const REFRESH: u8 = 0x12;
    pub const PLL: u8 = 0x30;
    pub const VCOM_DATA: u8 = 0x50;
    pub const TCON: u8 = 0x60;
    pub const RESOLUTION: u8 = 0x61;
    pub const COMMAND_84: u8 = 0x84;
    pub const COMMAND_AA: u8 = 0xAA;
    pub const COMMAND_E3: u8 = 0xE3;
}

/// The inks of this panel.
pub const INKS: Palette = SPECTRA6;

/// The panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

impl Config {
    /// Waveshare's 7.3 inch six-colour panel, 800x480.
    pub const EPD_7IN3E: Self = Self {
        width: 800,
        height: 480,
    };
}

/// A 7.3 inch six-colour panel on a [`Bus`].
#[derive(Debug)]
pub struct Epd7in3e<B> {
    bus: B,
    config: Config,
}

impl<B: Bus> Epd7in3e<B> {
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
        self.bus.wait_busy_low()?;
        self.bus.delay_ms(50)?;
        self.send(cmd::COMMAND_AA, &[0x49, 0x55, 0x20, 0x08, 0x09, 0x18])?;
        self.send(cmd::POWER_SETTING, &[0x3F])?;
        self.send(cmd::PANEL_SETTING, &[0x5F, 0x69])?;
        self.send(cmd::POWER_OFF_SEQUENCE, &[0x00, 0x54, 0x00, 0x44])?;
        self.send(cmd::BOOSTER_5, &[0x40, 0x1F, 0x1F, 0x2C])?;
        self.send(cmd::BOOSTER_6, &[0x6F, 0x1F, 0x17, 0x49])?;
        self.send(cmd::BOOSTER_8, &[0x6F, 0x1F, 0x1F, 0x22])?;
        self.send(cmd::PLL, &[0x03])?;
        self.send(cmd::VCOM_DATA, &[0x3F])?;
        self.send(cmd::TCON, &[0x02, 0x00])?;
        let (w, h) = (self.config.width, self.config.height);
        self.send(
            cmd::RESOLUTION,
            &[(w >> 8) as u8, w as u8, (h >> 8) as u8, h as u8],
        )?;
        self.send(cmd::COMMAND_84, &[0x01])?;
        self.send(cmd::COMMAND_E3, &[0x2F])?;
        self.send(cmd::POWER_ON, &[])?;
        self.bus.wait_busy_low()
    }

    /// Writes a picture of four-bit codes (two pixels to a byte) and refreshes.
    ///
    /// # Errors
    /// The bus error.
    pub fn show(&mut self, codes: &[u8]) -> Result<(), B::Error> {
        self.wake()?;
        self.send(cmd::DATA, codes)?;
        // `EPD_TurnOnDisplay`: power on, the booster again, refresh, power off.
        self.send(cmd::POWER_ON, &[])?;
        self.bus.wait_busy_low()?;
        self.send(cmd::BOOSTER_6, &[0x6F, 0x1F, 0x17, 0x49])?;
        self.send(cmd::REFRESH, &[0x00])?;
        self.bus.wait_busy_low()?;
        self.send(cmd::POWER_OFF, &[0x00])?;
        self.bus.wait_busy_low()
    }
}

/// One byte of a one-bit picture (eight pixels, a set bit white) as four bytes of codes: a clear bit is black (0), a set one white (1).
fn widen(byte: u8) -> [u8; 4] {
    let mut out = [0u8; 4];
    for i in 0..8 {
        let bit = (byte >> (7 - i)) & 1;
        out[i / 2] |= bit << (4 * (1 - i % 2));
    }
    out
}

impl<B: Bus> Panel for Epd7in3e<B> {
    type Error = B::Error;

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), B::Error> {
        let mut codes = Vec::with_capacity(frame.len() * 4);
        for &b in frame {
            codes.extend_from_slice(&widen(b));
        }
        self.show(&codes)
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        // No partial refresh; a driver made for a colour panel never asks.
        let _ = (region, data);
        Ok(())
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        // Powered off after every refresh.
        Ok(())
    }

    fn palette(&self) -> Option<Palette> {
        Some(INKS)
    }

    fn full_color(&mut self, packed: &[u8]) -> Result<(), B::Error> {
        self.show(packed)
    }

    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        fusuma::Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Micro)
            .with_glass(fusuma::DisplayKind::Reflective, 2, 30_000)
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
    fn a_picture_is_init_data_and_the_turn_on_sequence() {
        let mut p = Epd7in3e::new(Recorder::default(), Config::EPD_7IN3E);
        p.show(&[0x01, 0x56]).unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(&log[..3], [Call::Reset, Call::BusyLow, Call::Delay(50)]);
        assert_eq!(
            commands(log),
            [
                0xAA, 0x01, 0x00, 0x03, 0x05, 0x06, 0x08, 0x30, 0x50, 0x60, 0x61, 0x84, 0xE3,
                0x04, // init
                0x10, // the picture
                0x04, 0x06, 0x12, 0x02, // turn on
            ]
        );
        // 800 x 480 as the resolution; the picture as given; the busy line after the init's power-on, the turn-on's, the refresh and the power-off.
        assert!(log.contains(&Call::Data(vec![0x03, 0x20, 0x01, 0xE0])));
        assert!(log.contains(&Call::Data(vec![0x01, 0x56])));
        assert_eq!(log.iter().filter(|c| **c == Call::BusyLow).count(), 5);
    }

    #[test]
    fn a_one_bit_picture_is_black_and_white_codes() {
        assert_eq!(widen(0b1001_1100), [0x10, 0x01, 0x11, 0x00]);
        let mut p = Epd7in3e::new(Recorder::default(), Config::EPD_7IN3E);
        p.full(&[0xFF]).unwrap();
        assert!(p.bus_mut().log.contains(&Call::Data(vec![0x11; 4])));
    }

    #[test]
    fn the_panel_says_it_has_six_inks() {
        let p = Epd7in3e::new(Recorder::default(), Config::EPD_7IN3E);
        assert_eq!(p.palette(), Some(SPECTRA6));
        assert_eq!(p.size(), (800, 480));
    }
}
