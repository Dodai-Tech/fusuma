//! What an e-paper controller needs from the chip: the same four things for every controller on SPI.

use alloc::vec::Vec;

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

    /// Reads one data byte from the controller (the data/command line high), for a chip that has a read-back (a
    /// temperature, an id). Most buses are write-only and a panel's data line is shared with its write line, so the
    /// host reads by letting go of the SPI peripheral and bit-banging the clock; a bus that cannot returns `None` (the
    /// default).
    ///
    /// # Errors
    /// The pin error.
    fn read_byte(&mut self) -> Result<Option<u8>, Self::Error> {
        Ok(None)
    }

    /// Waits for `ms` milliseconds.
    ///
    /// # Errors
    /// The timer error.
    fn delay_ms(&mut self, ms: u32) -> Result<(), Self::Error> {
        let _ = ms;
        Ok(())
    }

    /// Waits until the busy pin says the controller is done (it is high while busy).
    ///
    /// # Errors
    /// The pin error, or a timeout.
    fn wait_busy(&mut self) -> Result<(), Self::Error>;
}

/// A bus that records what it was sent, for tests of the sequences and of a driver on top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recorder {
    /// Every call, in order.
    pub log: Vec<Call>,
    /// What a [`read_byte`](Bus::read_byte) gives: `None` for a bus that cannot read.
    pub read: Option<u8>,
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
    /// A one byte read.
    Read,
    /// A delay in milliseconds.
    Delay(u32),
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

    fn read_byte(&mut self) -> Result<Option<u8>, Self::Error> {
        self.log.push(Call::Read);
        Ok(self.read)
    }

    fn delay_ms(&mut self, ms: u32) -> Result<(), Self::Error> {
        self.log.push(Call::Delay(ms));
        Ok(())
    }
}
