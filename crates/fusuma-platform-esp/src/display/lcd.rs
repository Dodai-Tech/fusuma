//! A plain SPI LCD with a data/command line: the ST7789 or the GC9A01, whichever the board file names. Like the AMOLEDs it is
//! sent rectangles in bands (see `bands`); the backlight is a separate LEDC pin.

use esp_idf_svc::sys::fusuma_esp as ffi;
use fusuma::render::Rect;
use fusuma_platform_embedded::Pixels;

use super::bands::{BAND_PX, Bands};
use crate::board::{self, DISPLAY_OPTIONS as OPTIONS, DISPLAY_PINS as PINS};

#[cfg(fusuma_display = "st7789")]
const KIND: i32 = ffi::FUSUMA_LCD_ST7789 as i32;
#[cfg(fusuma_display = "gc9a01")]
const KIND: i32 = ffi::FUSUMA_LCD_GC9A01 as i32;

#[cfg(fusuma_display = "st7789")]
const NAME: &str = "st7789";
#[cfg(fusuma_display = "gc9a01")]
const NAME: &str = "gc9a01";

/// The panel and its two band buffers.
pub(crate) struct Display {
    bands: Bands,
}

impl Display {
    /// Brings the panel up.
    pub(crate) fn open() -> anyhow::Result<Self> {
        let cfg = ffi::fusuma_lcd_config_t {
            kind: KIND,
            sck: board::pin(PINS, "sck"),
            mosi: board::pin(PINS, "mosi"),
            cs: board::pin(PINS, "cs"),
            dc: board::pin(PINS, "dc"),
            rst: board::pin(PINS, "rst"),
            pclk_hz: u32::try_from(board::option(OPTIONS, "pclk_mhz") * 1_000_000)
                .unwrap_or(40_000_000),
            x_offset: board::OFFSET[0],
            y_offset: board::OFFSET[1],
            madctl: u8::try_from(board::option(OPTIONS, "madctl")).unwrap_or(0),
            invert: i32::from(board::option(OPTIONS, "invert") == 1),
            tuned: i32::from(has_option("tuned") && board::option(OPTIONS, "tuned") == 1),
            max_transfer: BAND_PX * 2,
        };
        // SAFETY: called once at startup, before anything else touches the panel; `cfg` lives for the call.
        let err = unsafe { ffi::fusuma_lcd_init(&cfg) };
        anyhow::ensure!(err == 0, "{NAME} init failed: esp_err {err}");
        Ok(Self {
            bands: Bands::new(NAME)?,
        })
    }

    /// The LCD has no brightness register: the backlight pin does it.
    pub(crate) fn brightness(&mut self, _percent: i32) -> bool {
        false
    }

    /// Where `present`'s time went since the last report, then starts counting again.
    pub(crate) fn report(&mut self) -> Option<String> {
        self.bands.report()
    }

    /// Sends `region` (cut to the screen and grown to the panel's grid).
    pub(crate) fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) {
        self.bands.present(region, pixels, |x0, y0, x1, y1, p| {
            // SAFETY: `p` is a DMA-capable band that stays untouched until the driver's next call.
            unsafe { ffi::fusuma_lcd_draw(x0, y0, x1, y1, p) }
        });
    }
}

/// Whether the board's option table has `name` (only the ST7789 has `tuned`).
fn has_option(name: &str) -> bool {
    OPTIONS.iter().any(|(n, _)| *n == name)
}
