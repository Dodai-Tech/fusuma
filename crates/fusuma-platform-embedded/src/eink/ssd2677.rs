//! The SSD2677: the controller of the newer ("3.97 V2") reTerminal Sticky panel, 800x480 like the SSD1677 one it replaces
//! but with a different, UC81xx-style command set: power on and off around every refresh, one RAM of two bits a pixel, and
//! a waveform picked from the panel's temperature.
//!
//! The two panels cannot be told apart by looking at the board: the chip is asked for the id in register `0x70`
//! ([`probe`], `0x07` is an SSD2677). The sequences follow what the OpenDisplay project's firmware documents for this
//! panel (its `panel_ssd2677.cpp`, GPL-3.0; nothing of it is copied, the register numbers and values are facts about the
//! chip) and Seeed's own driver. Like the rest of this module they are checked against a recording bus only, **not** on a
//! panel.
//!
//! What this driver does not have: a fast or partial refresh (nobody has published a waveform for one), so every refresh,
//! partial or not, is a full flashing one of the whole screen, and the four-gray mode. [`Ssd2677::partial`] therefore
//! keeps the picture itself (48 KB) and shows all of it again.

use alloc::vec;
use alloc::vec::Vec;

use fusuma::render::Rect;

use super::bus::Bus;
use super::{Panel, byte_region, row_bytes};

/// The id the chip answers to register `0x70` with.
pub const CHIP_ID: u8 = 0x07;

mod cmd {
    pub const PANEL_SETTING: u8 = 0x00;
    pub const POWER_OFF: u8 = 0x02;
    pub const POWER_ON: u8 = 0x04;
    pub const BOOSTER: u8 = 0x06;
    pub const DEEP_SLEEP: u8 = 0x07;
    pub const DATA: u8 = 0x10;
    pub const REFRESH: u8 = 0x12;
    pub const VCOM_DATA: u8 = 0x50;
    pub const RESOLUTION: u8 = 0x61;
    pub const TEMPERATURE: u8 = 0x40;
    pub const CHIP_ID: u8 = 0x70;
}

/// The chip's frame is 680 gate lines tall and 200 are hidden: the visible 480 are the last ones.
const HIDDEN_ROWS: u32 = 200;

/// The panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

impl Config {
    /// The Sticky's 3.97 inch panel.
    pub const STICKY: Self = Self {
        width: 800,
        height: 480,
    };
}

/// Asks the chip behind `bus` which it is: a reset, then register `0x70`. `Some(true)` for an SSD2677, `Some(false)` for
/// something else that answered (an SSD1677 has no such register and gives another byte or none), `None` when the bus
/// cannot read.
///
/// # Errors
/// The bus error.
pub fn probe<B: Bus>(bus: &mut B) -> Result<Option<bool>, B::Error> {
    // Twice, so that one noisy read does not decide.
    let mut ids = [0u8; 2];
    for slot in &mut ids {
        bus.reset()?;
        bus.command(cmd::CHIP_ID)?;
        let Some(id) = bus.read_byte()? else {
            return Ok(None);
        };
        *slot = id;
    }
    Ok(Some(ids == [CHIP_ID; 2]))
}

/// An SSD2677 panel on a [`Bus`].
#[derive(Debug)]
pub struct Ssd2677<B> {
    bus: B,
    config: Config,
    awake: bool,
    /// The picture as the app drew it, a bit a pixel, a set bit white: every refresh shows all of it.
    shown: Vec<u8>,
}

/// The two-bit pixels of one byte of one-bit ones, high nibble first: black `00`, white `11`.
const fn widen(byte: u8) -> [u8; 2] {
    const fn nibble(n: u8) -> u8 {
        let mut out = 0;
        let mut bit = 0;
        while bit < 4 {
            out |= (if n >> bit & 1 == 1 { 0b11 } else { 0 }) << (bit * 2);
            bit += 1;
        }
        out
    }
    [nibble(byte >> 4), nibble(byte & 0x0F)]
}

/// The waveform selector for a panel at `raw` degrees, as the chip reports it (a reading that makes no sense is the
/// coldest bucket).
#[must_use]
pub const fn waveform(raw: u8) -> u8 {
    match raw {
        0..=10 => 0xE8,
        11..=20 => 0xEB,
        21..=30 => 0xEE,
        31..=127 => 0xF1,
        _ => 0xE8,
    }
}

impl<B: Bus> Ssd2677<B> {
    /// A panel. Nothing is sent until the first refresh, which wakes it.
    pub fn new(bus: B, config: Config) -> Self {
        Self {
            bus,
            config,
            awake: false,
            shown: vec![0xFF; (config.width as usize).div_ceil(8) * config.height as usize],
        }
    }

    /// The bus, to read or change it.
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

    /// Reset and the init sequence, then the waveform for the temperature the chip reads.
    fn wake(&mut self) -> Result<(), B::Error> {
        if self.awake {
            return Ok(());
        }
        self.bus.reset()?;
        // Panel setting: the scan direction with UD cleared, as the Sticky is mounted.
        self.send(cmd::PANEL_SETTING, &[0x27, 0x0E])?;
        self.bus.wait_busy()?;
        self.send(cmd::BOOSTER, &[0x0F, 0x8B, 0x93, 0xC1])?;
        self.send(0xE7, &[0xC1])?;
        self.send(0x30, &[0x08])?;
        self.send(cmd::VCOM_DATA, &[0x77])?;
        self.send(0x62, &[0x76, 0x76, 0x76, 0x5A, 0x9D, 0x8A, 0x76, 0x62])?;
        // The controller's own size: 800 source lines and 680 gate lines, of which 480 are visible.
        let gates = self.config.height + HIDDEN_ROWS;
        self.send(
            cmd::RESOLUTION,
            &[
                (self.config.width >> 8) as u8,
                self.config.width as u8,
                (gates >> 8) as u8,
                gates as u8,
            ],
        )?;
        self.send(0xE0, &[0x10])?;
        self.send(0x65, &[0, 0, 0, 0])?;
        self.send(0xE9, &[0x01])?;
        self.bus.wait_busy()?;
        // The waveform follows the temperature: read it, then select the waveform for it.
        self.send(cmd::TEMPERATURE, &[])?;
        self.bus.wait_busy()?;
        let temperature = self.bus.read_byte()?.unwrap_or(25);
        self.send(0xE0, &[0x12])?;
        self.send(0xE6, &[waveform(temperature)])?;
        self.send(0xA5, &[])?;
        self.bus.wait_busy()?;
        self.bus.delay_ms(10)?;
        self.awake = true;
        Ok(())
    }

    /// Sends the whole picture and shows it.
    fn show(&mut self) -> Result<(), B::Error> {
        self.wake()?;
        self.bus.command(cmd::DATA)?;
        // The rows above the visible ones, white.
        let blank = vec![0xFFu8; 512];
        let mut hidden = (HIDDEN_ROWS * self.config.width / 4) as usize;
        while hidden > 0 {
            let n = hidden.min(blank.len());
            self.bus.data(&blank[..n])?;
            hidden -= n;
        }
        let row = (self.config.width as usize).div_ceil(8);
        let mut out = Vec::with_capacity(row * 2 * 8);
        for rows in self.shown.chunks(row * 8) {
            out.clear();
            for &b in rows {
                out.extend_from_slice(&widen(b));
            }
            self.bus.data(&out)?;
        }
        self.send(cmd::POWER_ON, &[])?;
        self.bus.wait_busy()?;
        self.send(cmd::REFRESH, &[0x00])?;
        self.bus.wait_busy()?;
        // Drop the high voltage again: the panel keeps the picture without power.
        self.send(cmd::POWER_OFF, &[0x00])?;
        self.bus.wait_busy()
    }
}

impl<B: Bus> Panel for Ssd2677<B> {
    type Error = B::Error;

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), B::Error> {
        if frame.len() == self.shown.len() {
            self.shown.copy_from_slice(frame);
        }
        self.show()
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        let Some(r) = byte_region(region, self.config.width, self.config.height) else {
            return Ok(());
        };
        if r != region || data.len() != row_bytes(r) * r.h as usize {
            return Ok(());
        }
        let row = (self.config.width as usize).div_ceil(8);
        let (x, w) = (r.x as usize / 8, row_bytes(r));
        for (i, line) in data.chunks(w).enumerate() {
            let at = (r.y as usize + i) * row + x;
            self.shown[at..at + w].copy_from_slice(line);
        }
        self.show()
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        if self.awake {
            self.send(cmd::DEEP_SLEEP, &[0xA5])?;
            self.awake = false;
        }
        Ok(())
    }

    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        // Every refresh is a full flashing one: several seconds.
        fusuma::Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Panel)
            .with_glass(fusuma::DisplayKind::Reflective, 1, 3000)
    }
}

#[cfg(test)]
mod tests {
    use super::super::bus::{Call, Recorder};
    use super::*;

    fn data_after(log: &[Call], c: u8) -> Vec<Vec<u8>> {
        log.windows(2)
            .filter_map(|w| match w {
                [Call::Command(x), Call::Data(d)] if *x == c => Some(d.clone()),
                _ => None,
            })
            .collect()
    }

    fn panel(temperature: Option<u8>) -> Ssd2677<Recorder> {
        Ssd2677::new(
            Recorder {
                read: temperature,
                ..Recorder::default()
            },
            Config::STICKY,
        )
    }

    #[test]
    fn a_byte_of_black_and_white_pixels_widens_to_two_bit_ones_high_nibble_first() {
        assert_eq!(widen(0x00), [0x00, 0x00]);
        assert_eq!(widen(0xFF), [0xFF, 0xFF]);
        // 1010_0101: pixels white, black, white, black | black, white, black, white.
        assert_eq!(widen(0b1010_0101), [0b1100_1100, 0b0011_0011]);
    }

    #[test]
    fn the_waveform_follows_the_temperature_and_a_nonsense_reading_is_the_coldest() {
        for (t, w) in [
            (0, 0xE8),
            (10, 0xE8),
            (11, 0xEB),
            (20, 0xEB),
            (25, 0xEE),
            (30, 0xEE),
            (31, 0xF1),
            (127, 0xF1),
            (128, 0xE8),
            (255, 0xE8),
        ] {
            assert_eq!(waveform(t), w, "{t}");
        }
    }

    #[test]
    fn the_chip_is_told_by_its_id_and_a_bus_that_cannot_read_says_so() {
        let mut yes = Recorder {
            read: Some(CHIP_ID),
            ..Recorder::default()
        };
        assert_eq!(probe(&mut yes), Ok(Some(true)));
        // Reset, the id register, a read; twice.
        assert_eq!(
            yes.log,
            [
                Call::Reset,
                Call::Command(0x70),
                Call::Read,
                Call::Reset,
                Call::Command(0x70),
                Call::Read
            ]
        );
        let mut other = Recorder {
            read: Some(0x00),
            ..Recorder::default()
        };
        assert_eq!(probe(&mut other), Ok(Some(false)));
        assert_eq!(probe(&mut Recorder::default()), Ok(None));
    }

    #[test]
    fn a_refresh_wakes_the_chip_streams_two_bit_rows_below_the_hidden_ones_and_powers_down_after() {
        let mut p = panel(Some(25));
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        let log = &p.bus_mut().log;
        let at = |c: u8| log.iter().position(|x| *x == Call::Command(c)).unwrap();
        // Order: panel setting ... temperature ... data ... power on, refresh, power off.
        assert!(
            at(0x00) < at(0x40)
                && at(0x40) < at(0x10)
                && at(0x10) < at(0x04)
                && at(0x04) < at(0x12)
                && at(0x12) < at(0x02)
        );
        // 800 source lines, 680 gate lines (0x02A8).
        assert_eq!(data_after(log, 0x61), [vec![0x03, 0x20, 0x02, 0xA8]]);
        // 25 degrees: the 21 to 30 waveform.
        assert_eq!(data_after(log, 0xE6), [vec![0xEE]]);
        // The data command is followed by only data until power on: 200 hidden rows then 480 rows, 2 bits a pixel.
        let start = log.iter().position(|x| *x == Call::Command(0x10)).unwrap();
        let end = log.iter().position(|x| *x == Call::Command(0x04)).unwrap();
        let total: usize = log[start..end]
            .iter()
            .map(|c| if let Call::Data(d) = c { d.len() } else { 0 })
            .sum();
        assert_eq!(total, (200 + 480) * 800 / 4);
        assert_eq!(*log.last().unwrap(), Call::Busy);
        assert_eq!(data_after(log, 0x12), [vec![0x00]]);
        assert_eq!(data_after(log, 0x02), [vec![0x00]]);
    }

    #[test]
    fn a_partial_keeps_the_picture_and_shows_all_of_it_again() {
        let mut p = panel(None);
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        // A black byte at x = 8 (the second byte of row 2), then show: every refresh is a whole one.
        p.partial(Rect::new(8, 2, 8, 1), &[0x00]).unwrap();
        assert_eq!(p.shown[2 * 100 + 1], 0x00);
        assert_eq!(p.shown[2 * 100], 0xFF);
        let log = &p.bus_mut().log;
        assert_eq!(log.iter().filter(|c| **c == Call::Command(0x12)).count(), 2);
        // Woken once only, and no temperature from a bus that cannot read means room temperature.
        assert_eq!(log.iter().filter(|c| **c == Call::Reset).count(), 1);
        assert_eq!(data_after(log, 0xE6), [vec![0xEE]]);
        // Crooked requests change nothing.
        let n = p.bus_mut().log.len();
        p.partial(Rect::new(9, 2, 8, 1), &[0]).unwrap();
        p.partial(Rect::new(8, 2, 8, 1), &[0, 0]).unwrap();
        assert_eq!(p.bus_mut().log.len(), n);
    }

    #[test]
    fn sleep_is_once_and_the_next_refresh_wakes_again() {
        let mut p = panel(None);
        p.sleep().unwrap();
        assert!(p.bus_mut().log.is_empty(), "never woke");
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        p.sleep().unwrap();
        p.sleep().unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(data_after(log, 0x07), [vec![0xA5]]);
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        assert_eq!(
            p.bus_mut()
                .log
                .iter()
                .filter(|c| **c == Call::Reset)
                .count(),
            2
        );
        assert!(Panel::screen(&p).is_reflective());
    }
}
