//! An EK79007 MIPI DSI panel (ESP32-P4): a frame buffer the DSI DMA scans out, which the rows are copied into.

use esp_idf_svc::sys::fusuma_esp as ffi;
use fusuma::render::Rect;
use fusuma_platform_embedded::{Block, ByteOrder, Pixels, aligned};

use crate::board::{self, DISPLAY_OPTIONS as OPTIONS, DISPLAY_PINS as PINS, GRID, HEIGHT, WIDTH};

/// The panel's frame buffer.
pub(crate) struct Display {
    screen: &'static mut [u16],
}

impl Display {
    /// Brings the panel up and takes the frame buffer.
    pub(crate) fn open() -> anyhow::Result<Self> {
        let option = |name| board::option(OPTIONS, name);
        let cfg = ffi::fusuma_ek79007_config_t {
            rst: board::pin(PINS, "rst"),
            lanes: u8::try_from(option("lanes")).unwrap_or(2),
            lane_mbps: u16::try_from(option("lane_mbps")).unwrap_or(1000),
            phy_ldo_channel: i32::try_from(option("phy_ldo_channel")).unwrap_or(-1),
            phy_ldo_mv: u16::try_from(option("phy_ldo_mv")).unwrap_or(2500),
        };
        // SAFETY: called once at startup, before anything else touches the display; `cfg` lives for the call.
        let err = unsafe { ffi::fusuma_ek79007_init(&cfg) };
        anyhow::ensure!(err == 0, "ek79007 init failed: esp_err {err}");
        // SAFETY: plain getter; the buffer lives as long as the display driver (the whole program).
        let fb = unsafe { ffi::fusuma_ek79007_framebuffer() };
        anyhow::ensure!(!fb.is_null(), "no frame buffer");
        // SAFETY: the DSI driver owns WIDTH * HEIGHT RGB565 pixels at `fb` for the whole program, and nothing else
        // writes to it; this is the only reference.
        let screen = unsafe { std::slice::from_raw_parts_mut(fb, (WIDTH * HEIGHT) as usize) };
        Ok(Self { screen })
    }

    /// The panel has no brightness of its own: false, so a backlight is used.
    pub(crate) fn brightness(&mut self, _percent: i32) -> bool {
        false
    }

    /// Nothing to report: the frame buffer is scanned out by the DSI DMA.
    pub(crate) fn report(&mut self) -> Option<String> {
        None
    }

    /// Copies `region` into the frame buffer and writes it back from the cache.
    pub(crate) fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) {
        let Some(r) = aligned(region, WIDTH, HEIGHT, GRID) else {
            return;
        };
        let w = WIDTH as usize;
        let (x0, y0) = (r.x as usize, r.y as usize);
        let (x1, y1) = (r.right() as usize, r.bottom() as usize);
        // Every row of the region in one call, straight into the frame buffer (one row every WIDTH).
        let end = (y1 - 1) * w + x1;
        let block = Block {
            y: y0 as u32,
            x: x0 as u32,
            width: (x1 - x0) as u32,
            stride: w,
            order: ByteOrder::Native,
        };
        pixels.rows(block, &mut self.screen[y0 * w + x0..end]);
        // SAFETY: rows are clamped inside the driver.
        unsafe {
            ffi::fusuma_ek79007_flush_rows(y0 as u32, y1 as u32);
        }
    }
}
