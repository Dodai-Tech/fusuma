//! The UC8179 (and its twin the GD7965): the controller of the 7.5 inch 800x480 black and white panel, Waveshare's `epd7in5_V2` and
//! the many compatible modules, over SPI.
//!
//! The command sequences are the ones of Waveshare's own driver (`epd7in5_V2.py`: `init`, `init_part`, `display`,
//! `display_Partial` and `sleep`), read from its source; the tests run them against a recording bus, and they have **not** been
//! checked on a panel. What differs from the SSD1680 family:
//!
//! * the busy line is **low** while the chip works, and the driver asks its status (`0x71`) before reading it
//!   ([`Bus::wait_busy_low`]);
//! * a picture is sent twice, as "old" (`0x10`) and "new" (`0x13`) data, and the new one is the inverse of ours (a set bit is
//!   black in the chip's memory, white in ours);
//! * a partial refresh needs a reset and another init (`init_part`) and then a window (`0x90`) in whole bytes; the chip keeps the
//!   rest of the picture. A full refresh after it needs the full init again, which this driver does by itself.
//!
//! The four-gray mode (Waveshare's `init_4Gray` and `display_4Gray`, only on panels sold after October 2023) is here behind
//! [`Uc8179::with_gray4`]: the picture is two planes, and every refresh in it is a whole one. Not here: the 4.2 inch UC8176, which
//! needs a look-up table of waveforms.

use alloc::vec::Vec;

use fusuma::render::Rect;

use super::bus::Bus;
use super::{Panel, byte_region, row_bytes};

mod cmd {
    pub const PANEL_SETTING: u8 = 0x00;
    pub const POWER_SETTING: u8 = 0x01;
    pub const POWER_OFF: u8 = 0x02;
    pub const POWER_ON: u8 = 0x04;
    pub const BOOSTER: u8 = 0x06;
    pub const DEEP_SLEEP: u8 = 0x07;
    pub const OLD_DATA: u8 = 0x10;
    pub const REFRESH: u8 = 0x12;
    pub const NEW_DATA: u8 = 0x13;
    pub const DUAL_SPI: u8 = 0x15;
    pub const VCOM_DATA: u8 = 0x50;
    pub const TCON: u8 = 0x60;
    pub const RESOLUTION: u8 = 0x61;
    pub const STATUS: u8 = 0x71;
    pub const PARTIAL_WINDOW: u8 = 0x90;
    pub const PARTIAL_IN: u8 = 0x91;
    pub const CASCADE: u8 = 0xE0;
    pub const TEMPERATURE: u8 = 0xE5;
}

/// The two bits of a level in the two planes of the gray mode (`0x10` first, `0x13` second), from Waveshare's `display_4Gray`: black is
/// in both, dark gray only in the second, light gray only in the first, white in neither.
const fn planes(level: u8) -> (bool, bool) {
    match level {
        0 => (true, true),
        1 => (false, true),
        2 => (true, false),
        _ => (false, false),
    }
}

/// The panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

impl Config {
    /// The 7.5 inch panel, 800x480.
    pub const EPD_7IN5: Self = Self {
        width: 800,
        height: 480,
    };
}

/// Which init the chip is in: the full one (for a full refresh) or the partial one (a window at a time).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Asleep,
    Full,
    Partial,
    Gray,
}

/// A UC8179 panel on a [`Bus`].
#[derive(Debug)]
pub struct Uc8179<B> {
    bus: B,
    config: Config,
    mode: Mode,
    scratch: Vec<u8>,
    gray: bool,
}

impl<B: Bus> Uc8179<B> {
    /// A panel. Nothing is sent until the first refresh, which wakes it.
    pub const fn new(bus: B, config: Config) -> Self {
        Self {
            bus,
            config,
            mode: Mode::Asleep,
            scratch: Vec::new(),
            gray: false,
        }
    }

    /// Turns the four-gray mode on: only for a panel that has it (sold after October 2023); on an older one the picture comes out
    /// wrong. See [`Panel::full_gray4`].
    #[must_use]
    pub const fn with_gray4(mut self) -> Self {
        self.gray = true;
        self
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

    /// Asks the chip's status, then waits for its busy line to go high.
    fn busy(&mut self) -> Result<(), B::Error> {
        self.bus.command(cmd::STATUS)?;
        self.bus.wait_busy_low()
    }

    /// The init of a full refresh (Waveshare's `init`): booster, power, panel, resolution, intervals.
    fn init_full(&mut self) -> Result<(), B::Error> {
        self.bus.reset()?;
        self.send(cmd::BOOSTER, &[0x17, 0x17, 0x28, 0x17])?;
        self.send(cmd::POWER_SETTING, &[0x07, 0x07, 0x28, 0x17])?;
        self.send(cmd::POWER_ON, &[])?;
        self.bus.delay_ms(100)?;
        self.busy()?;
        self.send(cmd::PANEL_SETTING, &[0x1F])?;
        let (w, h) = (self.config.width, self.config.height);
        self.send(
            cmd::RESOLUTION,
            &[(w >> 8) as u8, w as u8, (h >> 8) as u8, h as u8],
        )?;
        self.send(cmd::DUAL_SPI, &[0x00])?;
        self.send(cmd::VCOM_DATA, &[0x10, 0x07])?;
        self.send(cmd::TCON, &[0x22])?;
        self.mode = Mode::Full;
        Ok(())
    }

    /// The init of a partial refresh (Waveshare's `init_part`): the chip is reset, so the picture on the glass stays but a full one
    /// must be sent again for a full refresh.
    fn init_partial(&mut self) -> Result<(), B::Error> {
        self.bus.reset()?;
        self.send(cmd::PANEL_SETTING, &[0x1F])?;
        self.send(cmd::POWER_ON, &[])?;
        self.bus.delay_ms(100)?;
        self.busy()?;
        self.send(cmd::CASCADE, &[0x02])?;
        self.send(cmd::TEMPERATURE, &[0x6E])?;
        self.mode = Mode::Partial;
        Ok(())
    }

    /// The init of the gray mode (Waveshare's `init_4Gray`): panel, intervals, power, booster, cascade, temperature.
    fn init_gray(&mut self) -> Result<(), B::Error> {
        self.bus.reset()?;
        self.send(cmd::PANEL_SETTING, &[0x1F])?;
        self.send(cmd::VCOM_DATA, &[0x10, 0x07])?;
        self.send(cmd::POWER_ON, &[])?;
        self.bus.delay_ms(100)?;
        self.busy()?;
        self.send(cmd::BOOSTER, &[0x27, 0x27, 0x18, 0x17])?;
        self.send(cmd::CASCADE, &[0x02])?;
        self.send(cmd::TEMPERATURE, &[0x5F])?;
        self.mode = Mode::Gray;
        Ok(())
    }

    fn refresh(&mut self) -> Result<(), B::Error> {
        self.send(cmd::REFRESH, &[])?;
        self.bus.delay_ms(100)?;
        self.busy()
    }
}

impl<B: Bus> Panel for Uc8179<B> {
    type Error = B::Error;

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), B::Error> {
        self.init_full()?;
        // "Old" data is the picture as ours (a set bit is white); the new data is its inverse (a set bit is black).
        self.send(cmd::OLD_DATA, frame)?;
        self.scratch.clear();
        self.scratch.extend(frame.iter().map(|b| !b));
        let inverse = core::mem::take(&mut self.scratch);
        let sent = self.send(cmd::NEW_DATA, &inverse);
        self.scratch = inverse;
        sent?;
        self.refresh()
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        let Some(r) = byte_region(region, self.config.width, self.config.height) else {
            return Ok(());
        };
        if r != region || data.len() != row_bytes(r) * r.h as usize {
            return Ok(());
        }
        if self.mode != Mode::Partial {
            self.init_partial()?;
        }
        let (x0, x1) = (r.x as u32, r.x as u32 + r.w - 1);
        let (y0, y1) = (r.y as u32, r.y as u32 + r.h - 1);
        self.send(cmd::VCOM_DATA, &[0xA9, 0x07])?;
        self.bus.command(cmd::PARTIAL_IN)?;
        self.send(
            cmd::PARTIAL_WINDOW,
            &[
                (x0 >> 8) as u8,
                x0 as u8,
                (x1 >> 8) as u8,
                x1 as u8,
                (y0 >> 8) as u8,
                y0 as u8,
                (y1 >> 8) as u8,
                y1 as u8,
                0x01,
            ],
        )?;
        // In a partial refresh the new data is sent as ours (Waveshare's `display_Partial` inverts the buffer it was given, which
        // is the inverse of ours).
        self.send(cmd::NEW_DATA, data)?;
        self.refresh()
    }

    fn gray4(&self) -> bool {
        self.gray
    }

    fn full_gray4(&mut self, levels: &[u8]) -> Result<(), B::Error> {
        // Two bits a pixel in; two planes of one bit a pixel out, eight pixels to a byte.
        let (mut a, mut b) = (
            Vec::with_capacity(levels.len() / 2),
            Vec::with_capacity(levels.len() / 2),
        );
        for pair in levels.as_chunks::<2>().0 {
            let (mut ba, mut bb) = (0u8, 0u8);
            for byte in pair {
                for k in (0..4).rev() {
                    let (pa, pb) = planes((byte >> (k * 2)) & 3);
                    ba = (ba << 1) | u8::from(pa);
                    bb = (bb << 1) | u8::from(pb);
                }
            }
            a.push(ba);
            b.push(bb);
        }
        self.init_gray()?;
        self.send(cmd::OLD_DATA, &a)?;
        self.send(cmd::NEW_DATA, &b)?;
        self.refresh()
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        if self.mode == Mode::Asleep {
            return Ok(());
        }
        self.send(cmd::VCOM_DATA, &[0xF7])?;
        self.send(cmd::POWER_OFF, &[])?;
        self.busy()?;
        self.send(cmd::DEEP_SLEEP, &[0xA5])?;
        self.mode = Mode::Asleep;
        Ok(())
    }

    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        // A full refresh is about four seconds and flashes; a partial one about 0.4.
        fusuma::Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Tablet)
            .with_glass(
                fusuma::DisplayKind::Reflective,
                if self.gray { 2 } else { 1 },
                if self.gray { 4000 } else { 500 },
            )
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

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

    fn commands(log: &[Call]) -> Vec<u8> {
        log.iter()
            .filter_map(|c| {
                if let Call::Command(c) = c {
                    Some(*c)
                } else {
                    None
                }
            })
            .collect()
    }

    fn panel() -> Uc8179<Recorder> {
        Uc8179::new(Recorder::default(), Config::EPD_7IN5)
    }

    #[test]
    fn a_full_refresh_follows_waveshares_init_sends_old_and_inverted_new_data_and_waits_on_a_low_busy_line()
     {
        let mut p = panel();
        let frame: Vec<u8> = (0..100 * 480).map(|i| (i % 251) as u8).collect();
        p.full(&frame).unwrap();
        let log = &p.bus_mut().log;
        // The init, in Waveshare's order: booster, power setting, power on, panel, resolution, dual SPI, intervals, tcon.
        assert_eq!(
            commands(log)[..11],
            [
                0x06, 0x01, 0x04, 0x71, 0x00, 0x61, 0x15, 0x50, 0x60, 0x10, 0x13
            ]
        );
        assert_eq!(data_after(log, 0x06)[0], [0x17, 0x17, 0x28, 0x17]);
        assert_eq!(data_after(log, 0x01)[0], [0x07, 0x07, 0x28, 0x17]);
        assert_eq!(
            data_after(log, 0x61)[0],
            [0x03, 0x20, 0x01, 0xE0],
            "800 x 480"
        );
        assert_eq!(data_after(log, 0x50)[0], [0x10, 0x07]);
        // The picture twice: the old data as it is, the new data its inverse.
        assert_eq!(data_after(log, 0x10)[0], frame);
        let inv: Vec<u8> = frame.iter().map(|b| !b).collect();
        assert_eq!(data_after(log, 0x13)[0], inv);
        // The chip is asked its status before each wait for the busy line, which is low while it works.
        let waits = log.iter().filter(|c| **c == Call::BusyLow).count();
        let asks = commands(log).iter().filter(|c| **c == 0x71).count();
        assert_eq!(
            (waits, asks),
            (2, 2),
            "after power on and after the refresh"
        );
        assert!(!log.contains(&Call::Busy), "never the high-busy wait");
        assert_eq!(*log.last().unwrap(), Call::BusyLow);
        assert!(log.contains(&Call::Delay(100)));
    }

    #[test]
    fn a_partial_refresh_resets_into_the_partial_init_once_sends_a_byte_window_and_the_new_data_as_it_is()
     {
        let mut p = panel();
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        let n = p.bus_mut().log.len();
        p.partial(Rect::new(8, 10, 16, 2), &[0x0F, 0xF0, 0x55, 0xAA])
            .unwrap();
        let log = p.bus_mut().log[n..].to_vec();
        assert_eq!(log[0], Call::Reset, "the partial init resets the chip");
        assert_eq!(
            commands(&log),
            [
                0x00, 0x04, 0x71, 0xE0, 0xE5, 0x50, 0x91, 0x90, 0x13, 0x12, 0x71
            ]
        );
        assert_eq!(data_after(&log, 0xE5)[0], [0x6E]);
        assert_eq!(data_after(&log, 0x50)[0], [0xA9, 0x07]);
        // x 8 to 23, y 10 to 11, then the "partial window" flag.
        assert_eq!(data_after(&log, 0x90)[0], [0, 8, 0, 23, 0, 10, 0, 11, 0x01]);
        assert_eq!(
            data_after(&log, 0x13)[0],
            [0x0F, 0xF0, 0x55, 0xAA],
            "as ours, not inverted"
        );
        // A second partial refresh does not reset again.
        let n = p.bus_mut().log.len();
        p.partial(Rect::new(0, 0, 8, 1), &[0x11]).unwrap();
        assert!(!p.bus_mut().log[n..].contains(&Call::Reset));
        assert_eq!(
            commands(&p.bus_mut().log[n..]),
            [0x50, 0x91, 0x90, 0x13, 0x12, 0x71]
        );
        // A full one after them runs the full init again (the chip was reset into the partial one).
        let n = p.bus_mut().log.len();
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        assert_eq!(p.bus_mut().log[n], Call::Reset);
        assert_eq!(commands(&p.bus_mut().log[n..])[0], 0x06);
    }

    #[test]
    fn crooked_requests_send_nothing_and_a_partial_as_the_first_thing_wakes_the_chip_in_the_partial_init()
     {
        let mut p = panel();
        p.partial(Rect::new(9, 10, 16, 2), &[0; 4]).unwrap();
        p.partial(Rect::new(8, 10, 16, 2), &[0; 3]).unwrap();
        p.partial(Rect::new(2000, 0, 8, 8), &[0; 8]).unwrap();
        assert!(p.bus_mut().log.is_empty());
        p.partial(Rect::new(8, 10, 16, 2), &[0; 4]).unwrap();
        assert_eq!(
            commands(&p.bus_mut().log)[0],
            0x00,
            "the partial init, not the full one"
        );
    }

    #[test]
    fn sleep_powers_off_waits_deep_sleeps_once_and_a_sleeping_chip_is_woken_by_the_next_refresh() {
        let mut p = panel();
        p.sleep().unwrap();
        assert!(p.bus_mut().log.is_empty(), "never woke");
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        let n = p.bus_mut().log.len();
        p.sleep().unwrap();
        p.sleep().unwrap();
        let log = p.bus_mut().log[n..].to_vec();
        assert_eq!(commands(&log), [0x50, 0x02, 0x71, 0x07]);
        assert_eq!(data_after(&log, 0x50)[0], [0xF7]);
        assert_eq!(data_after(&log, 0x07)[0], [0xA5]);
        let n = p.bus_mut().log.len();
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        assert_eq!(p.bus_mut().log[n], Call::Reset, "woken again");
        let s = Panel::screen(&p);
        assert!(s.is_reflective());
        assert_eq!((s.width, s.height, s.depth_bits), (800, 480, 1));
    }

    #[test]
    fn the_four_gray_mode_follows_waveshares_init_and_sends_two_planes_from_two_bit_pixels() {
        let mut p = panel().with_gray4();
        assert!(Panel::gray4(&p));
        assert!(!Panel::gray4(&panel()), "off unless asked for");
        // One row of 8 pixels: the four levels twice (black, dark, light, white), packed two bits a pixel, the first in the top bits.
        let levels = [0b00_01_10_11, 0b00_01_10_11];
        p.full_gray4(&levels).unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(
            commands(log),
            [
                0x00, 0x50, 0x04, 0x71, 0x06, 0xE0, 0xE5, 0x10, 0x13, 0x12, 0x71
            ]
        );
        assert_eq!(data_after(log, 0x06)[0], [0x27, 0x27, 0x18, 0x17]);
        assert_eq!(data_after(log, 0xE5)[0], [0x5F]);
        // Black is in both planes, dark gray in the second, light gray in the first, white in neither. Levels 0,1,2,3,0,1,2,3 give
        // the first plane 1,0,1,0,1,0,1,0 and the second 1,1,0,0,1,1,0,0.
        assert_eq!(data_after(log, 0x10)[0], [0b1010_1010]);
        assert_eq!(data_after(log, 0x13)[0], [0b1100_1100]);
        assert_eq!(*log.last().unwrap(), Call::BusyLow);
        // A partial refresh after it starts from a reset again, and the screen says it has four grays.
        let n = p.bus_mut().log.len();
        p.partial(Rect::new(0, 0, 8, 1), &[0xFF]).unwrap();
        assert_eq!(p.bus_mut().log[n], Call::Reset);
        let s = Panel::screen(&p);
        assert_eq!(s.depth_bits, 2);
    }
}
