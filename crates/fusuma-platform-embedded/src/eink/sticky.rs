//! Seeed's reTerminal Sticky has shipped with two panels of the same size and pixel count: an SSD1677 and, newer, an
//! SSD2677 whose command set differs. [`Sticky::detect`] asks the chip which one it is and gives the matching panel.

use fusuma::render::Rect;

use super::Panel;
use super::bus::Bus;
use super::ssd1677::{self, Ssd1677};
use super::ssd2677::{self, Ssd2677};

/// Which panel is in the Sticky.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// The original panel: partial refresh, about half a second.
    Ssd1677,
    /// The newer panel: full refresh only, several seconds.
    Ssd2677,
}

/// The Sticky's panel, whichever it is.
#[derive(Debug)]
pub enum Sticky<B> {
    /// An SSD1677.
    Ssd1677(Ssd1677<B>),
    /// An SSD2677.
    Ssd2677(Ssd2677<B>),
}

impl<B: Bus> Sticky<B> {
    /// Probes the chip on `bus`: an SSD2677 answers register `0x70` with `0x07`; anything else, or a bus that cannot read,
    /// is taken for an SSD1677 (the first panel, and the one every host can drive with no read-back), wired mirrored in x
    /// if `mirror_x`.
    ///
    /// # Errors
    /// The bus error.
    pub fn detect(mut bus: B, mirror_x: bool) -> Result<Self, B::Error> {
        Ok(if ssd2677::probe(&mut bus)? == Some(true) {
            Self::Ssd2677(Ssd2677::new(bus, ssd2677::Config::STICKY))
        } else {
            Self::Ssd1677(Ssd1677::new(
                bus,
                ssd1677::Config {
                    mirror_x,
                    ..ssd1677::Config::STICKY
                },
            ))
        })
    }

    /// Which panel it is.
    #[must_use]
    pub const fn variant(&self) -> Variant {
        match self {
            Self::Ssd1677(_) => Variant::Ssd1677,
            Self::Ssd2677(_) => Variant::Ssd2677,
        }
    }
}

impl<B: Bus> Panel for Sticky<B> {
    type Error = B::Error;

    fn size(&self) -> (u32, u32) {
        (800, 480)
    }

    fn full(&mut self, frame: &[u8]) -> Result<(), B::Error> {
        match self {
            Self::Ssd1677(p) => p.full(frame),
            Self::Ssd2677(p) => p.full(frame),
        }
    }

    fn partial(&mut self, region: Rect, data: &[u8]) -> Result<(), B::Error> {
        match self {
            Self::Ssd1677(p) => p.partial(region, data),
            Self::Ssd2677(p) => p.partial(region, data),
        }
    }

    fn sleep(&mut self) -> Result<(), B::Error> {
        match self {
            Self::Ssd1677(p) => p.sleep(),
            Self::Ssd2677(p) => p.sleep(),
        }
    }

    fn screen(&self) -> fusuma::Screen {
        match self {
            Self::Ssd1677(p) => p.screen(),
            Self::Ssd2677(p) => p.screen(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::bus::Recorder;
    use super::*;

    #[test]
    fn the_id_picks_the_panel_and_a_bus_that_cannot_read_gets_the_first_one() {
        let new = Sticky::detect(
            Recorder {
                read: Some(ssd2677::CHIP_ID),
                ..Recorder::default()
            },
            true,
        )
        .unwrap();
        assert_eq!(new.variant(), Variant::Ssd2677);
        assert_eq!(new.screen().refresh_ms, 3000, "full refreshes only");
        for read in [Some(0x00), Some(0xFF), None] {
            let old = Sticky::detect(
                Recorder {
                    read,
                    ..Recorder::default()
                },
                true,
            )
            .unwrap();
            assert_eq!(old.variant(), Variant::Ssd1677, "{read:?}");
            assert_eq!(old.screen().refresh_ms, 600);
        }
        assert_eq!(new.size(), (800, 480));
    }
}
