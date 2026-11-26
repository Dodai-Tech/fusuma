//! The SSD1677 (and the SSD2677, which Seeed's reTerminal Sticky also ships with and which takes the same commands): the
//! controller of the 3.97 inch 800x480 black and white panel of the Sticky and of Waveshare's 4.26 inch 800x480 one, over
//! SPI.
//!
//! Like [`Ssd1680`](super::ssd1680::Ssd1680) it works on a [`Bus`]; what differs is the command set: coordinates are
//! 16 bits (not bytes), the gate count is a 16-bit field, a partial refresh is update sequence `0xFF` and a full one
//! `0xF7`. The sequences follow ESPHome's `epaper_spi` model `seeed-reterminal-sticky` (init, windows, planes) and
//! have **not** been checked on a panel yet; the tests run them against a recording bus.
//!
//! The Sticky's panel is wired mirrored in x ([`Config::mirror_x`]): the picture is mirrored on its way out, so the app
//! and everything above this driver see an ordinary screen.

use alloc::vec::Vec;

use fusuma::render::Rect;

use super::bus::Bus;
use super::{Panel, byte_region, row_bytes};

mod cmd {
    pub const DRIVER_OUTPUT: u8 = 0x01;
    pub const DEEP_SLEEP: u8 = 0x10;
    pub const DATA_ENTRY: u8 = 0x11;
    pub const SOFT_RESET: u8 = 0x12;
    pub const TEMP_SENSOR: u8 = 0x18;
    pub const MASTER_ACTIVATE: u8 = 0x20;
    pub const UPDATE_CONTROL_2: u8 = 0x22;
    /// The new picture (black and white RAM).
    pub const WRITE_NEW: u8 = 0x24;
    /// The picture the panel shows (the comparison RAM, "red" on a colour panel).
    pub const WRITE_OLD: u8 = 0x26;
    pub const BORDER: u8 = 0x3C;
    pub const INRUSH: u8 = 0x0C;
    pub const RAM_X_RANGE: u8 = 0x44;
    pub const RAM_Y_RANGE: u8 = 0x45;
    pub const RAM_X_COUNTER: u8 = 0x4E;
    pub const RAM_Y_COUNTER: u8 = 0x4F;
}

/// The panel and the update sequences it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// Whether the panel is wired mirrored in x, so the picture is mirrored on its way out. Needs `width` to be a
    /// multiple of 8.
    pub mirror_x: bool,
    /// The border waveform (`0x3C`): 0x01 follows the black and white look-up table.
    pub border: u8,
    /// Update control 2 for a full refresh.
    pub full_sequence: u8,
    /// Update control 2 for a partial refresh.
    pub partial_sequence: u8,
}

impl Config {
    /// A panel of `width` x `height` with the sequences of the Sticky's 3.97 inch module.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mirror_x: false,
            border: 0x01,
            full_sequence: 0xF7,
            partial_sequence: 0xFF,
        }
    }

    /// The reTerminal Sticky's 3.97 inch panel, 800x480, wired mirrored in x.
    pub const STICKY: Self = Self {
        mirror_x: true,
        ..Self::new(800, 480)
    };

    /// Waveshare's 4.26 inch panel, 800x480, wired mirrored in x.
    pub const EPD_4IN26: Self = Self::STICKY;
}

/// An SSD1677 panel on a [`Bus`].
#[derive(Debug)]
pub struct Ssd1677<B> {
    bus: B,
    config: Config,
    awake: bool,
    scratch: Vec<u8>,
}

/// `byte` with its bits in the opposite order.
const fn flip(byte: u8) -> u8 {
    byte.reverse_bits()
}

/// `data` (rows of `row` bytes, a bit a pixel) mirrored in x into `out`: the rows' bytes in the opposite order, each byte
/// with its bits reversed.
fn mirror_rows(data: &[u8], row: usize, out: &mut Vec<u8>) {
    out.clear();
    if row == 0 {
        return;
    }
    out.reserve(data.len());
    for line in data.chunks(row) {
        out.extend(line.iter().rev().map(|&b| flip(b)));
    }
}

impl<B: Bus> Ssd1677<B> {
    /// A panel. Nothing is sent until the first refresh, which wakes it.
    pub const fn new(bus: B, config: Config) -> Self {
        Self {
            bus,
            config,
            awake: false,
            scratch: Vec::new(),
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

    /// Reset and set the chip up: the temperature sensor, the inrush current, the gate count, the border and the order
    /// the memory fills in.
    fn wake(&mut self) -> Result<(), B::Error> {
        if self.awake {
            return Ok(());
        }
        self.bus.reset()?;
        self.bus.wait_busy()?;
        self.send(cmd::SOFT_RESET, &[])?;
        self.bus.wait_busy()?;
        self.send(cmd::TEMP_SENSOR, &[0x80])?;
        self.send(cmd::INRUSH, &[0xAE, 0xC7, 0xC3, 0xC0, 0x80])?;
        let gates = self.config.height - 1;
        self.send(
            cmd::DRIVER_OUTPUT,
            &[(gates & 0xFF) as u8, (gates >> 8) as u8, 0x02],
        )?;
        self.send(cmd::BORDER, &[self.config.border])?;
        // X and Y both count up, X first.
        self.send(cmd::DATA_ENTRY, &[0x03])?;
        self.awake = true;
        Ok(())
    }

    /// Points the memory at `region` of the panel (x in whole bytes, so a multiple of 8).
    fn window(&mut self, region: Rect) -> Result<(), B::Error> {
        let (x0, x1) = (region.x as u32, region.x as u32 + region.w - 1);
        let (y0, y1) = (region.y as u32, region.y as u32 + region.h - 1);
        self.send(
            cmd::RAM_X_RANGE,
            &[x0 as u8, (x0 >> 8) as u8, x1 as u8, (x1 >> 8) as u8],
        )?;
        self.send(cmd::RAM_X_COUNTER, &[x0 as u8, (x0 >> 8) as u8])?;
        self.send(
            cmd::RAM_Y_RANGE,
            &[y0 as u8, (y0 >> 8) as u8, y1 as u8, (y1 >> 8) as u8],
        )?;
        self.send(cmd::RAM_Y_COUNTER, &[y0 as u8, (y0 >> 8) as u8])
    }

    fn update(&mut self, sequence: u8) -> Result<(), B::Error> {
        self.send(cmd::UPDATE_CONTROL_2, &[sequence])?;
        self.send(cmd::MASTER_ACTIVATE, &[])?;
        self.bus.wait_busy()
    }

    /// Where `region` (the app's, byte-aligned) is on the panel, and `data` as the panel takes it.
    fn on_panel(&mut self, region: Rect, data: &[u8]) -> (Rect, Vec<u8>) {
        if self.config.mirror_x && self.config.width.is_multiple_of(8) {
            let mut out = core::mem::take(&mut self.scratch);
            mirror_rows(data, row_bytes(region), &mut out);
            let x = self.config.width - (region.x as u32 + region.w);
            (Rect::new(x as i32, region.y, region.w, region.h), out)
        } else {
            (region, data.to_vec())
        }
    }
}

impl<B: Bus> Panel for Ssd1677<B> {
    type Error = B::Error;

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), B::Error> {
        self.wake()?;
        let whole = Rect::new(
            0,
            0,
            self.config.width.next_multiple_of(8),
            self.config.height,
        );
        let (region, data) = self.on_panel(whole, frame);
        self.window(region)?;
        self.send(cmd::WRITE_NEW, &data)?;
        // The comparison memory too, so the next partial refresh differs from this picture and not from nothing.
        self.window(region)?;
        self.send(cmd::WRITE_OLD, &data)?;
        self.scratch = data;
        self.update(self.config.full_sequence)
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        let Some(r) = byte_region(region, self.config.width, self.config.height) else {
            return Ok(());
        };
        if r != region || data.len() != row_bytes(r) * r.h as usize {
            return Ok(());
        }
        self.wake()?;
        let (r, data) = self.on_panel(r, data);
        self.window(r)?;
        self.send(cmd::WRITE_NEW, &data)?;
        self.update(self.config.partial_sequence)?;
        // What is shown now is what the next partial refresh compares against.
        self.window(r)?;
        self.send(cmd::WRITE_OLD, &data)?;
        self.scratch = data;
        Ok(())
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        if self.awake {
            // Deep sleep 3: the memory is lost, which is why the next refresh after a sleep is a full one.
            self.send(cmd::DEEP_SLEEP, &[0x03])?;
            self.awake = false;
        }
        Ok(())
    }

    fn screen(&self) -> fusuma::Screen {
        let (w, h) = self.size();
        fusuma::Screen::new(w, h)
            .with_form_factor(fusuma::FormFactor::Panel)
            .with_glass(fusuma::DisplayKind::Reflective, 1, 600)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::super::bus::{Call, Recorder};
    use super::*;

    fn cmds(log: &[Call]) -> Vec<u8> {
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

    fn data_after(log: &[Call], c: u8) -> Vec<Vec<u8>> {
        log.windows(2)
            .filter_map(|w| match w {
                [Call::Command(x), Call::Data(d)] if *x == c => Some(d.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn bits_and_rows_are_mirrored_the_way_a_panel_wired_backwards_wants() {
        assert_eq!(flip(0b1000_0000), 0b0000_0001);
        assert_eq!(flip(0b1101_0010), 0b0100_1011);
        let mut out = Vec::new();
        // Two rows of three bytes: the bytes swap ends and each is reversed.
        mirror_rows(&[0x01, 0x02, 0x03, 0x80, 0x00, 0xF0], 3, &mut out);
        assert_eq!(out, [0xC0, 0x40, 0x80, 0x0F, 0x00, 0x01]);
        mirror_rows(&[1, 2, 3], 0, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn the_chip_is_woken_once_with_the_gate_count_and_the_border_of_its_panel() {
        let mut p = Ssd1677::new(Recorder::default(), Config::STICKY);
        assert_eq!(p.size(), (800, 480));
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(
            &log[..4],
            [Call::Reset, Call::Busy, Call::Command(0x12), Call::Busy]
        );
        // 480 gates: 479 = 0x01DF, then the scan direction.
        assert_eq!(data_after(log, 0x01), [vec![0xDF, 0x01, 0x02]]);
        assert_eq!(data_after(log, 0x3C), [vec![0x01]]);
        assert_eq!(data_after(log, 0x11), [vec![0x03]]);
        assert_eq!(data_after(log, 0x0C), [vec![0xAE, 0xC7, 0xC3, 0xC0, 0x80]]);
        // The whole panel, 16-bit coordinates: x 0..799 (0x031F), y 0..479 (0x01DF).
        assert_eq!(data_after(log, 0x44)[0], [0, 0, 0x1F, 0x03]);
        assert_eq!(data_after(log, 0x45)[0], [0, 0, 0xDF, 0x01]);
        // The new picture then the comparison one, then a full update.
        assert_eq!(data_after(log, 0x24)[0].len(), 100 * 480);
        assert_eq!(data_after(log, 0x26)[0].len(), 100 * 480);
        assert_eq!(data_after(log, 0x22), [vec![0xF7]]);
        assert_eq!(*cmds(log).last().unwrap(), 0x20);
        assert_eq!(*log.last().unwrap(), Call::Busy, "it waits for the refresh");
        // A second refresh does not wake it again.
        let n = p
            .bus_mut()
            .log
            .iter()
            .filter(|c| **c == Call::Reset)
            .count();
        p.full(&vec![0xFF; 100 * 480]).unwrap();
        assert_eq!(
            p.bus_mut()
                .log
                .iter()
                .filter(|c| **c == Call::Reset)
                .count(),
            n
        );
    }

    #[test]
    fn a_partial_refresh_goes_to_the_mirrored_place_with_mirrored_bits_and_keeps_the_comparison_memory()
     {
        let mut p = Ssd1677::new(Recorder::default(), Config::STICKY);
        // 16 x 2 pixels at x = 8, y = 10: two bytes a row.
        p.partial(Rect::new(8, 10, 16, 2), &[0x01, 0x80, 0xF0, 0x0F])
            .unwrap();
        let log = &p.bus_mut().log;
        // Mirrored: the region's left edge on the panel is 800 - (8 + 16) = 776 (0x0308), and its right edge 791 (0x0317).
        assert_eq!(data_after(log, 0x44)[0], [0x08, 0x03, 0x17, 0x03]);
        assert_eq!(data_after(log, 0x4E)[0], [0x08, 0x03]);
        assert_eq!(data_after(log, 0x45)[0], [10, 0, 11, 0]);
        // These bytes are their own mirror images; the next test uses ones that are not.
        assert_eq!(data_after(log, 0x24)[0], [0x01, 0x80, 0xF0, 0x0F]);
        // The partial sequence, and the same data in the comparison memory afterwards.
        assert_eq!(data_after(log, 0x22), [vec![0xFF]]);
        assert!(log.ends_with(&[
            Call::Command(0x26),
            Call::Data(vec![0x01, 0x80, 0xF0, 0x0F])
        ]));
        let at = |c| log.iter().position(|x| *x == Call::Command(c)).unwrap();
        assert!(
            at(0x24) < at(0x20)
                && at(0x20) < log.iter().rposition(|x| *x == Call::Command(0x26)).unwrap()
        );
    }

    #[test]
    fn a_mirrored_partial_is_not_a_palindrome_and_an_unmirrored_panel_sends_it_as_it_is() {
        let mut p = Ssd1677::new(Recorder::default(), Config::STICKY);
        p.partial(Rect::new(0, 0, 16, 1), &[0b1000_0000, 0b0000_0110])
            .unwrap();
        assert_eq!(
            data_after(&p.bus_mut().log, 0x24)[0],
            [0b0110_0000, 0b0000_0001]
        );
        assert_eq!(
            data_after(&p.bus_mut().log, 0x44)[0],
            [0x10, 0x03, 0x1F, 0x03],
            "the far right edge: x 784 to 799"
        );
        let mut q = Ssd1677::new(Recorder::default(), Config::new(800, 480));
        q.partial(Rect::new(0, 0, 16, 1), &[0b1000_0000, 0b0000_0110])
            .unwrap();
        assert_eq!(
            data_after(&q.bus_mut().log, 0x24)[0],
            [0b1000_0000, 0b0000_0110]
        );
        assert_eq!(data_after(&q.bus_mut().log, 0x44)[0], [0, 0, 0x0F, 0]);
    }

    #[test]
    fn crooked_or_off_panel_requests_send_nothing_and_sleep_is_once_and_makes_the_next_wake_again()
    {
        let mut p = Ssd1677::new(Recorder::default(), Config::STICKY);
        p.partial(Rect::new(8, 10, 16, 2), &[0; 3]).unwrap();
        p.partial(Rect::new(9, 10, 16, 2), &[0; 4]).unwrap();
        p.partial(Rect::new(2000, 10, 16, 2), &[0; 4]).unwrap();
        assert!(
            !p.bus_mut()
                .log
                .iter()
                .any(|c| matches!(c, Call::Command(0x24)))
        );
        p.sleep().unwrap();
        assert!(
            p.bus_mut()
                .log
                .iter()
                .all(|c| !matches!(c, Call::Command(0x10))),
            "never woke, nothing to sleep"
        );
        p.full(&vec![0; 100 * 480]).unwrap();
        p.sleep().unwrap();
        p.sleep().unwrap();
        let log = &p.bus_mut().log;
        assert_eq!(cmds(log).iter().filter(|&&c| c == 0x10).count(), 1);
        assert_eq!(data_after(log, 0x10), [vec![0x03]]);
        p.full(&vec![0; 100 * 480]).unwrap();
        assert_eq!(
            p.bus_mut()
                .log
                .iter()
                .filter(|c| **c == Call::Reset)
                .count(),
            2,
            "woken again"
        );
    }

    #[test]
    fn the_panel_tells_the_app_it_is_a_reflective_touch_board_and_the_sticky_is_a_panel_not_a_watch()
     {
        let p = Ssd1677::new(Recorder::default(), Config::STICKY);
        let s = Panel::screen(&p);
        assert!(s.is_reflective());
        assert_eq!(s.form_factor(), fusuma::FormFactor::Panel);
        assert_eq!((s.width, s.height, s.depth_bits), (800, 480, 1));
    }
}
