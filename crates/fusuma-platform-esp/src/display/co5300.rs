//! A CO5300 AMOLED over QSPI. The panel has no frame buffer the chip can write into: every update is sent as a
//! rectangle, in bands of rows through two buffers in internal RAM, so one band is filled while the one before it is
//! still being sent (the driver's next command waits for that transfer to end).

use std::time::Instant;

use esp_idf_svc::sys::{MALLOC_CAP_DMA, MALLOC_CAP_INTERNAL, fusuma_esp as ffi, heap_caps_malloc};
use fusuma::render::Rect;
use fusuma_platform_embedded::{ByteOrder, Pixels, aligned, band_rows};

use crate::board::{self, DISPLAY_OPTIONS as OPTIONS, DISPLAY_PINS as PINS, GRID, HEIGHT, WIDTH};

/// Pixels in one band buffer: 32 KB, the driver's largest transfer.
const BAND_PX: usize = 16 * 1024;

/// The panel and its two band buffers.
pub(crate) struct Display {
    bands: [&'static mut [u16]; 2],
    /// The band to fill next; the other one may still be in flight.
    next: usize,
    errors: u32,
    /// The ESP-IDF error code of the last failed transfer, and how many bands were sent.
    last_error: i32,
    bands_sent: u32,
    /// Since the last report: filling the bands, waiting for the panel (the transfer the filling did not hide),
    /// updates and pixels sent.
    fill_us: u64,
    wait_us: u64,
    presents: u32,
    pixels: u64,
}

impl Display {
    /// Brings the panel up.
    pub(crate) fn open() -> anyhow::Result<Self> {
        let d = board::pins(PINS, "d");
        anyhow::ensure!(d.len() == 4, "co5300: pins.d needs four GPIOs");
        let cfg = ffi::fusuma_co5300_config_t {
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
        let err = unsafe { ffi::fusuma_co5300_init(&cfg) };
        anyhow::ensure!(err == 0, "co5300 init failed: esp_err {err}");
        Ok(Self {
            bands: [band()?, band()?],
            next: 0,
            errors: 0,
            last_error: 0,
            bands_sent: 0,
            fill_us: 0,
            wait_us: 0,
            presents: 0,
            pixels: 0,
        })
    }

    /// Sets the brightness, 0 to 100; true, an AMOLED sets its own.
    pub(crate) fn brightness(&mut self, percent: i32) -> bool {
        let level = u8::try_from(percent.clamp(0, 100) * 255 / 100).unwrap_or(u8::MAX);
        // SAFETY: plain setter after init.
        unsafe { ffi::fusuma_co5300_brightness(level) };
        true
    }

    /// Where `present`'s time went since the last report, then starts counting again.
    pub(crate) fn report(&mut self) -> Option<String> {
        let n = u64::from(self.presents.max(1));
        let line = format!(
            "co5300 per update: filling {} us, waiting for the panel {} us, {} px; panel errors {} of {} bands (last esp_err {:#x})",
            self.fill_us / n,
            self.wait_us / n,
            self.pixels / n,
            self.errors,
            self.bands_sent,
            self.last_error
        );
        (self.fill_us, self.wait_us, self.presents, self.pixels) = (0, 0, 0, 0);
        Some(line)
    }

    /// Sends `region` (cut to the screen and grown to the panel's grid).
    pub(crate) fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) {
        let Some(r) = aligned(region, WIDTH, HEIGHT, GRID) else {
            return;
        };
        let (x0, y0, x1, y1) = (r.x as u32, r.y as u32, r.right() as u32, r.bottom() as u32);
        let width = x1 - x0;
        // Rows a multiple of the grid, so every band starts on the grid too.
        let rows = band_rows(width, BAND_PX as u32, GRID);
        let mut y = y0;
        while y < y1 {
            let h = rows.min(y1 - y);
            let band = &mut self.bands[self.next][..(width * h) as usize];
            let t0 = Instant::now();
            // One pass: the rows, already in the panel's big-endian order.
            pixels.rows(y, x0, width, band, width as usize, ByteOrder::BigEndian);
            let t1 = Instant::now();
            // SAFETY: the band is DMA-capable and stays untouched until the driver's next call: the other band is
            // filled next, and its draw waits for this transfer before it starts.
            let err = unsafe {
                ffi::fusuma_co5300_draw(
                    x0 as u16,
                    y as u16,
                    x1 as u16,
                    (y + h) as u16,
                    band.as_ptr(),
                )
            };
            self.bands_sent += 1;
            if err != 0 {
                self.errors += 1;
                self.last_error = err;
            }
            self.fill_us += t1.duration_since(t0).as_micros() as u64;
            self.wait_us += t1.elapsed().as_micros() as u64;
            self.next ^= 1;
            y += h;
        }
        self.presents += 1;
        self.pixels += u64::from(width) * u64::from(y1 - y0);
    }
}

/// A band buffer in internal, DMA-capable RAM, kept for the whole program.
fn band() -> anyhow::Result<&'static mut [u16]> {
    // SAFETY: a plain allocation; the memory is zeroed before it is used as a slice, and never freed.
    unsafe {
        let ptr = heap_caps_malloc(BAND_PX * 2, MALLOC_CAP_DMA | MALLOC_CAP_INTERNAL).cast::<u16>();
        anyhow::ensure!(
            !ptr.is_null(),
            "no {} KB of DMA memory for a band",
            BAND_PX * 2 / 1024
        );
        ptr.write_bytes(0, BAND_PX);
        Ok(std::slice::from_raw_parts_mut(ptr, BAND_PX))
    }
}
