//! The ST7305: the controller of Waveshare's 4.2 inch 400x300 reflective LCD (the ESP32-S3-RLCD-4.2), over SPI. A reflective LCD needs no
//! backlight and, unlike e-paper, refreshes at once and without a flash, so a picture can be sent whenever it changes.
//!
//! The sequences and the memory layout are the ones of Waveshare's Arduino example (`display_bsp.cpp`: `RLCD_Init`, `RLCD_Display` and the
//! landscape pixel table), read from its source; the tests run them against a recording bus, and they have **not** been checked on a
//! panel. What is odd about the chip:
//!
//! * its memory is not rows of pixels: it is organised in blocks of **2 columns by 4 rows**, a byte each, and a column of blocks goes
//!   down the glass before the next one starts. [`place`] is the pixel's byte and bit; the picture is sent whole every time (15 KB, a few
//!   milliseconds), so the driver keeps the picture in this layout and updates it as parts arrive;
//! * a set bit is white.

use alloc::vec;
use alloc::vec::Vec;

use fusuma::render::Rect;

use super::bus::Bus;
use super::{Panel, byte_region, row_bytes};

mod cmd {
    pub const SLEEP_OUT: u8 = 0x11;
    pub const INVERSION_ON: u8 = 0x21;
    pub const DISPLAY_ON: u8 = 0x29;
    pub const COLUMNS: u8 = 0x2A;
    pub const ROWS: u8 = 0x2B;
    pub const WRITE: u8 = 0x2C;
    pub const MEMORY_ACCESS: u8 = 0x36;
    pub const PIXEL_FORMAT: u8 = 0x3A;
    pub const TEARING_ON: u8 = 0x35;
    pub const HIGH_POWER: u8 = 0x38;
}

/// The panel: 400x300, landscape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

impl Config {
    /// Waveshare's 4.2 inch reflective LCD, 400x300.
    pub const RLCD_4IN2: Self = Self {
        width: 400,
        height: 300,
    };
}

/// Where pixel `(x, y)` of a `height` tall picture is in the chip's memory: the byte, and the bit's mask. Two columns by four rows to a byte,
/// the rows counted from the bottom, the column of blocks going up the glass (the vendor's `InitLandscapeLUT`).
#[must_use]
pub const fn place(x: u32, y: u32, height: u32) -> (usize, u8) {
    let inv_y = height - 1 - y;
    let index = (x / 2) as usize * (height / 4) as usize + (inv_y / 4) as usize;
    let bit = 7 - ((inv_y % 4) * 2 + x % 2);
    (index, 1 << bit)
}

/// A reflective LCD on a [`Bus`].
#[derive(Debug)]
pub struct St7305<B> {
    bus: B,
    config: Config,
    /// The picture in the chip's layout, white where a bit is set.
    memory: Vec<u8>,
    awake: bool,
}

impl<B: Bus> St7305<B> {
    /// A panel of `config`'s size on `bus`, showing paper.
    pub fn new(bus: B, config: Config) -> Self {
        let bytes = (config.width * config.height / 8) as usize;
        Self {
            bus,
            config,
            memory: vec![0xFF; bytes],
            awake: false,
        }
    }

    /// The bus, for a test to read what was sent.
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    /// The picture in the chip's layout.
    #[must_use]
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    fn send(&mut self, command: u8, data: &[u8]) -> Result<(), B::Error> {
        self.bus.command(command)?;
        if data.is_empty() {
            Ok(())
        } else {
            self.bus.data(data)
        }
    }

    /// Resets the chip and runs Waveshare's init, once.
    fn wake(&mut self) -> Result<(), B::Error> {
        if self.awake {
            return Ok(());
        }
        self.bus.reset()?;
        self.send(0xD6, &[0x17, 0x02])?;
        self.send(0xD1, &[0x01])?;
        self.send(0xC0, &[0x11, 0x04])?;
        self.send(0xC1, &[0x69; 4])?;
        self.send(0xC2, &[0x19; 4])?;
        self.send(0xC4, &[0x4B; 4])?;
        self.send(0xC5, &[0x19; 4])?;
        self.send(0xD8, &[0x80, 0xE9])?;
        self.send(0xB2, &[0x02])?;
        self.send(
            0xB3,
            &[0xE5, 0xF6, 0x05, 0x46, 0x77, 0x77, 0x77, 0x77, 0x76, 0x45],
        )?;
        self.send(0xB4, &[0x05, 0x46, 0x77, 0x77, 0x77, 0x77, 0x76, 0x45])?;
        self.send(0x62, &[0x32, 0x03, 0x1F])?;
        self.send(0xB7, &[0x13])?;
        self.send(0xB0, &[0x64])?;
        self.send(cmd::SLEEP_OUT, &[])?;
        self.bus.delay_ms(200)?;
        self.send(0xC9, &[0x00])?;
        self.send(cmd::MEMORY_ACCESS, &[0x48])?;
        self.send(cmd::PIXEL_FORMAT, &[0x11])?;
        self.send(0xB9, &[0x20])?;
        self.send(0xB8, &[0x29])?;
        self.send(cmd::INVERSION_ON, &[])?;
        self.send(cmd::COLUMNS, &[0x12, 0x2A])?;
        self.send(cmd::ROWS, &[0x00, 0xC7])?;
        self.send(cmd::TEARING_ON, &[0x00])?;
        self.send(0xD0, &[0xFF])?;
        self.send(cmd::HIGH_POWER, &[])?;
        self.send(cmd::DISPLAY_ON, &[])?;
        self.awake = true;
        Ok(())
    }

    /// Sends the whole picture.
    fn show(&mut self) -> Result<(), B::Error> {
        self.wake()?;
        self.send(cmd::COLUMNS, &[0x12, 0x2A])?;
        self.send(cmd::ROWS, &[0x00, 0xC7])?;
        self.send(cmd::WRITE, &[])?;
        // The memory is a field of the panel; the bus takes a slice, so a copy is not needed: borrow it for the call.
        let memory = core::mem::take(&mut self.memory);
        let sent = self.bus.data(&memory);
        self.memory = memory;
        sent
    }

    /// Puts `rows` rows of `row_bytes(region)` bytes (a set bit white) at `region` in the memory.
    fn put(&mut self, region: Rect, data: &[u8]) {
        let per_row = row_bytes(region);
        for (row, line) in data.chunks(per_row).enumerate().take(region.h as usize) {
            let y = region.y as u32 + row as u32;
            for x in 0..region.w {
                let px = region.x as u32 + x;
                if px >= self.config.width || y >= self.config.height {
                    continue;
                }
                let white = (line[x as usize / 8] >> (7 - x % 8)) & 1 == 1;
                let (index, mask) = place(px, y, self.config.height);
                if white {
                    self.memory[index] |= mask;
                } else {
                    self.memory[index] &= !mask;
                }
            }
        }
    }
}

impl<B: Bus> Panel for St7305<B> {
    type Error = B::Error;

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), B::Error> {
        let whole = Rect::new(
            0,
            0,
            self.config.width.next_multiple_of(8),
            self.config.height,
        );
        self.put(whole, frame);
        self.show()
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        let Some(r) = byte_region(region, self.config.width, self.config.height) else {
            return Ok(());
        };
        if r != region || data.len() != row_bytes(r) * r.h as usize {
            return Ok(());
        }
        self.put(r, data);
        self.show()
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        // A reflective LCD keeps its picture on its own and draws almost nothing.
        Ok(())
    }

    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        // Reflective, one bit, and quick: no flash, a few milliseconds a refresh.
        fusuma::Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Micro)
            .with_glass(fusuma::DisplayKind::Reflective, 1, 30)
    }
}

#[cfg(test)]
mod tests {
    use super::super::bus::{Call, Recorder};
    use super::*;

    #[test]
    fn a_pixel_has_its_own_byte_and_bit_and_none_is_shared() {
        let h = 300;
        let mut seen = alloc::collections::BTreeSet::new();
        for x in 0..400 {
            for y in 0..h {
                let (i, m) = place(x, y, h);
                assert!(i < 400 * 300 / 8, "{x},{y}: {i}");
                assert_eq!(m.count_ones(), 1);
                assert!(seen.insert((i, m)), "{x},{y} collides");
            }
        }
        assert_eq!(seen.len(), 400 * 300);
        // The corners the vendor's table gives: the bottom-left pixel is the first byte's lowest rows.
        assert_eq!(place(0, 299, h), (0, 0b1000_0000));
        assert_eq!(place(1, 299, h), (0, 0b0100_0000));
        assert_eq!(place(0, 298, h), (0, 0b0010_0000));
        // Four rows up is the next block of the same column pair; two columns right is the next column of blocks.
        assert_eq!(place(0, 295, h), (1, 0b1000_0000));
        assert_eq!(place(2, 299, h), (75, 0b1000_0000));
    }

    #[test]
    fn the_init_runs_once_then_each_picture_is_the_window_and_the_whole_memory() {
        let mut p = St7305::new(Recorder::default(), Config::RLCD_4IN2);
        p.full(&vec![0xFF; 50 * 300]).unwrap();
        let log = p.bus_mut().log.clone();
        assert_eq!(log[0], Call::Reset);
        assert!(
            log.contains(&Call::Delay(200)),
            "the chip is given time to wake"
        );
        // The window and the write, then 15000 bytes of paper.
        let last = log.len();
        assert_eq!(log[last - 2], Call::Command(0x2C));
        assert_eq!(log[last - 1], Call::Data(vec![0xFF; 15_000]));
        // A second picture does not run the init again.
        let n = log.len();
        p.partial(Rect::new(0, 0, 8, 1), &[0x00]).unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(log[n], Call::Command(0x2A));
        assert_eq!(log.iter().filter(|c| **c == Call::Reset).count(), 1);
        // The first row's first eight pixels are now black in the chip's layout, and nothing else changed.
        let black: usize = p.memory().iter().map(|b| b.count_zeros() as usize).sum();
        assert_eq!(black, 8);
    }

    #[test]
    fn a_picture_comes_out_in_the_chips_layout_pixel_for_pixel() {
        let mut p = St7305::new(Recorder::default(), Config::RLCD_4IN2);
        // One black pixel at (3, 5): a frame of rows of 50 bytes.
        let mut frame = vec![0xFF; 50 * 300];
        frame[5 * 50] &= !(1 << (7 - 3));
        p.full(&frame).unwrap();
        let (index, mask) = place(3, 5, 300);
        for (i, b) in p.memory().iter().enumerate() {
            let want = if i == index { !mask } else { 0xFF };
            assert_eq!(*b, want, "byte {i}");
        }
    }
}
