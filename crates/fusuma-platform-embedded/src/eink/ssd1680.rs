//! The SSD1680 (and its relatives SSD1681, SSD1675): the controller of the common small black and white e-paper panels,
//! among them Waveshare's 1.54 inch (200x200), 2.13 inch (122x250) and 2.9 inch (128x296) modules, over SPI.
//!
//! The chip's HAL implements [`Bus`] (a command byte with the data/command pin low, data with it high, the reset and
//! busy pins, a delay); [`Ssd1680`] then is a [`Panel`](super::Panel). The command sequences follow Waveshare's V2
//! and V4 drivers; the tests run them against a recording bus, they have **not** been checked on a panel yet.

use alloc::vec::Vec;

use fusuma::render::Rect;

use super::{Panel, byte_region, row_bytes};

/// What the controller's bus needs: nothing about the chip, only what the pins do.
pub trait Bus {
    /// What goes wrong.
    type Error: core::fmt::Debug;

    /// Sends one command byte (the data/command pin low).
    ///
    /// # Errors
    /// The SPI or pin error.
    fn command(&mut self, command: u8) -> Result<(), Self::Error>;

    /// Sends data bytes (the data/command pin high).
    ///
    /// # Errors
    /// The SPI or pin error.
    fn data(&mut self, data: &[u8]) -> Result<(), Self::Error>;

    /// Pulses the reset pin (low for 10 ms, then high, then 10 ms more).
    ///
    /// # Errors
    /// The pin error.
    fn reset(&mut self) -> Result<(), Self::Error>;

    /// Waits until the busy pin says the controller is done (it is high while busy).
    ///
    /// # Errors
    /// The pin error, or a timeout.
    fn wait_busy(&mut self) -> Result<(), Self::Error>;
}

/// The commands of the chip this driver uses.
mod cmd {
    pub const DRIVER_OUTPUT: u8 = 0x01;
    pub const DEEP_SLEEP: u8 = 0x10;
    pub const DATA_ENTRY: u8 = 0x11;
    pub const SOFT_RESET: u8 = 0x12;
    pub const TEMP_SENSOR: u8 = 0x18;
    pub const MASTER_ACTIVATE: u8 = 0x20;
    pub const UPDATE_CONTROL_2: u8 = 0x22;
    pub const WRITE_BW: u8 = 0x24;
    pub const WRITE_PREVIOUS: u8 = 0x26;
    pub const BORDER: u8 = 0x3C;
    pub const RAM_X_RANGE: u8 = 0x44;
    pub const RAM_Y_RANGE: u8 = 0x45;
    pub const RAM_X_COUNTER: u8 = 0x4E;
    pub const RAM_Y_COUNTER: u8 = 0x4F;
}

/// The panel's size and the update sequences it takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Pixels across (the source lines).
    pub width: u32,
    /// Pixels down (the gate lines).
    pub height: u32,
    /// The value for "display update control 2" that runs a full refresh (the clock, the analog part, the temperature
    /// and the waveform, a full update).
    pub full_sequence: u8,
    /// The value for a partial refresh.
    pub partial_sequence: u8,
    /// The border waveform for a full refresh (white, so the edge does not stay grey).
    pub border_full: u8,
    /// The border waveform for a partial one (keeps what is there).
    pub border_partial: u8,
}

impl Config {
    /// A panel of `width` x `height` with the Waveshare V2/V4 sequences.
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            full_sequence: 0xF7,
            partial_sequence: 0x0F,
            border_full: 0x05,
            border_partial: 0x80,
        }
    }

    /// The 1.54 inch module, 200x200.
    pub const EPD_1IN54: Self = Self::new(200, 200);
    /// The 2.13 inch module, 122x250 (RAM is 128 wide).
    pub const EPD_2IN13: Self = Self::new(122, 250);
    /// The 2.9 inch module, 128x296.
    pub const EPD_2IN9: Self = Self::new(128, 296);
}

/// An SSD1680 panel on a [`Bus`].
#[derive(Debug)]
pub struct Ssd1680<B> {
    bus: B,
    config: Config,
    awake: bool,
}

impl<B: Bus> Ssd1680<B> {
    /// A panel. Nothing is sent until the first refresh, which wakes it.
    pub const fn new(bus: B, config: Config) -> Self {
        Self {
            bus,
            config,
            awake: false,
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

    /// Reset and set the chip up: the number of gates, the order the memory fills in, the temperature sensor.
    fn wake(&mut self) -> Result<(), B::Error> {
        if self.awake {
            return Ok(());
        }
        self.bus.reset()?;
        self.bus.wait_busy()?;
        self.send(cmd::SOFT_RESET, &[])?;
        self.bus.wait_busy()?;
        let gates = self.config.height - 1;
        self.send(
            cmd::DRIVER_OUTPUT,
            &[(gates & 0xFF) as u8, (gates >> 8) as u8, 0],
        )?;
        // X and Y both count up, the X counter moves first.
        self.send(cmd::DATA_ENTRY, &[0x03])?;
        // The internal temperature sensor picks the waveform.
        self.send(cmd::TEMP_SENSOR, &[0x80])?;
        self.awake = true;
        Ok(())
    }

    /// Points the memory at `region` (x in whole bytes).
    fn window(&mut self, region: Rect) -> Result<(), B::Error> {
        let (x0, x1) = (region.x as u32 / 8, (region.x as u32 + region.w) / 8 - 1);
        let (y0, y1) = (region.y as u32, region.y as u32 + region.h - 1);
        self.send(cmd::RAM_X_RANGE, &[x0 as u8, x1 as u8])?;
        self.send(
            cmd::RAM_Y_RANGE,
            &[
                (y0 & 0xFF) as u8,
                (y0 >> 8) as u8,
                (y1 & 0xFF) as u8,
                (y1 >> 8) as u8,
            ],
        )?;
        self.send(cmd::RAM_X_COUNTER, &[x0 as u8])?;
        self.send(cmd::RAM_Y_COUNTER, &[(y0 & 0xFF) as u8, (y0 >> 8) as u8])
    }

    fn update(&mut self, sequence: u8) -> Result<(), B::Error> {
        self.send(cmd::UPDATE_CONTROL_2, &[sequence])?;
        self.send(cmd::MASTER_ACTIVATE, &[])?;
        self.bus.wait_busy()
    }
}

impl<B: Bus> Panel for Ssd1680<B> {
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
        self.send(cmd::BORDER, &[self.config.border_full])?;
        self.window(whole)?;
        self.send(cmd::WRITE_BW, frame)?;
        // The "previous" memory too, so the next partial refresh differs from this picture and not from nothing.
        self.window(whole)?;
        self.send(cmd::WRITE_PREVIOUS, frame)?;
        self.update(self.config.full_sequence)
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        self.wake()?;
        // Whatever was asked for, the memory takes whole bytes inside the panel.
        let Some(r) = byte_region(region, self.config.width, self.config.height) else {
            return Ok(());
        };
        if data.len() != row_bytes(r) * r.h as usize {
            return Ok(());
        }
        self.send(cmd::BORDER, &[self.config.border_partial])?;
        self.window(r)?;
        self.send(cmd::WRITE_BW, data)?;
        self.update(self.config.partial_sequence)?;
        // After a partial refresh the "previous" memory has to hold what is shown now, or the next one flashes.
        self.window(r)?;
        self.send(cmd::WRITE_PREVIOUS, data)
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        if self.awake {
            self.send(cmd::DEEP_SLEEP, &[0x01])?;
            self.awake = false;
        }
        Ok(())
    }
}

/// A bus that records what it was sent, for tests of the sequences and of a driver on top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recorder {
    /// Every call, in order.
    pub log: Vec<Call>,
}

/// One call on a [`Recorder`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    /// A command byte.
    Command(u8),
    /// Data bytes.
    Data(Vec<u8>),
    /// A reset.
    Reset,
    /// A wait for the busy pin.
    Busy,
}

impl Bus for Recorder {
    type Error = core::convert::Infallible;

    fn command(&mut self, command: u8) -> Result<(), Self::Error> {
        self.log.push(Call::Command(command));
        Ok(())
    }

    fn data(&mut self, data: &[u8]) -> Result<(), Self::Error> {
        self.log.push(Call::Data(data.to_vec()));
        Ok(())
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        self.log.push(Call::Reset);
        Ok(())
    }

    fn wait_busy(&mut self) -> Result<(), Self::Error> {
        self.log.push(Call::Busy);
        Ok(())
    }
}
