//! An AMOLED on QSPI: the CO5300, the SH8601 or the RM690B0, whichever the board file names. The panel has no frame buffer the
//! chip can write into: every update is sent as a rectangle, in bands (see `bands`).

use esp_idf_svc::sys::fusuma_esp as ffi;
use fusuma::render::Rect;
use fusuma_platform_embedded::Pixels;

use super::bands::{BAND_PX, Bands};
use crate::board::{self, DISPLAY_OPTIONS as OPTIONS, DISPLAY_PINS as PINS};

/// The controller, as the C driver numbers it.
#[cfg(fusuma_display = "co5300")]
const KIND: i32 = ffi::FUSUMA_AMOLED_CO5300 as i32;
#[cfg(fusuma_display = "sh8601")]
const KIND: i32 = ffi::FUSUMA_AMOLED_SH8601 as i32;
#[cfg(fusuma_display = "rm690b0")]
const KIND: i32 = ffi::FUSUMA_AMOLED_RM690B0 as i32;

#[cfg(fusuma_display = "co5300")]
const NAME: &str = "co5300";
#[cfg(fusuma_display = "sh8601")]
const NAME: &str = "sh8601";
#[cfg(fusuma_display = "rm690b0")]
const NAME: &str = "rm690b0";

/// The panel and its two band buffers.
pub(crate) struct Display {
    bands: Bands,
}

impl Display {
    /// Brings the panel up.
    pub(crate) fn open() -> anyhow::Result<Self> {
        let d = PINS
            .iter()
            .find(|(n, _)| *n == "d")
            .map_or(&[][..], |(_, g)| *g);
        anyhow::ensure!(d.len() == 4, "{NAME}: pins.d needs four GPIOs");
        let cfg = ffi::fusuma_amoled_config_t {
            kind: KIND,
            probe: i32::from(has_option("probe_id") && board::option(OPTIONS, "probe_id") == 1),
            sck: board::pin(PINS, "sck"),
            d0: d[0],
            d1: d[1],
            d2: d[2],
            d3: d[3],
            cs: board::pin(PINS, "cs"),
            rst: board::pin(PINS, "rst"),
            pclk_hz: u32::try_from(board::option(OPTIONS, "pclk_mhz") * 1_000_000)
                .unwrap_or(80_000_000),
            x_offset: board::OFFSET[0],
            y_offset: board::OFFSET[1],
            max_transfer: BAND_PX * 2,
        };
        // SAFETY: called once at startup, before anything else touches the panel; `cfg` lives for the call.
        let err = unsafe { ffi::fusuma_amoled_init(&cfg) };
        anyhow::ensure!(err == 0, "{NAME} init failed: esp_err {err}");
        Ok(Self {
            bands: Bands::new(NAME)?,
        })
    }

    /// Sets the brightness, 0 to 100; true, an AMOLED sets its own.
    pub(crate) fn brightness(&mut self, percent: i32) -> bool {
        let level = u8::try_from(percent.clamp(0, 100) * 255 / 100).unwrap_or(u8::MAX);
        // SAFETY: plain setter after init.
        unsafe { ffi::fusuma_amoled_brightness(level) };
        true
    }

    /// Where `present`'s time went since the last report, then starts counting again.
    pub(crate) fn report(&mut self) -> Option<String> {
        self.bands.report()
    }

    /// Sends `region` (cut to the screen and grown to the panel's grid).
    pub(crate) fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) {
        self.bands.present(region, pixels, |x0, y0, x1, y1, p| {
            // SAFETY: `p` is a DMA-capable band that stays untouched until the driver's next call.
            unsafe { ffi::fusuma_amoled_draw(x0, y0, x1, y1, p) }
        });
    }
}

/// Whether the board's option table has `name` (only the CO5300 has `probe_id`).
fn has_option(name: &str) -> bool {
    OPTIONS.iter().any(|(n, _)| *n == name)
}
