//! Sending a rectangle to a panel that has no frame buffer the chip writes: in bands of rows through two buffers in
//! internal RAM, so one band is filled while the one before it is still being sent (the driver's next command waits for
//! that transfer to end). The QSPI AMOLEDs and the plain SPI LCDs both work this way; only the call that sends a band
//! differs.

use std::time::Instant;

use esp_idf_svc::sys::{MALLOC_CAP_DMA, MALLOC_CAP_INTERNAL, heap_caps_malloc};
use fusuma::render::Rect;
use fusuma_platform_embedded::{Block, ByteOrder, Pixels, aligned, band_rows};

use crate::board::{GRID, HEIGHT, WIDTH};

/// Pixels in one band buffer: 32 KB, the driver's largest transfer.
pub(super) const BAND_PX: usize = 16 * 1024;

/// The two band buffers and what `present` counted.
pub(super) struct Bands {
    name: &'static str,
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

impl Bands {
    /// Allocates the buffers; `name` is the controller, for the report.
    pub(super) fn new(name: &'static str) -> anyhow::Result<Self> {
        Ok(Self {
            name,
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

    /// Where `present`'s time went since the last report, then starts counting again.
    pub(super) fn report(&mut self) -> Option<String> {
        let n = u64::from(self.presents.max(1));
        let line = format!(
            "{} per update: filling {} us, waiting for the panel {} us, {} px; panel errors {} of {} bands (last esp_err {:#x})",
            self.name,
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

    /// Sends `region` (cut to the screen and grown to the panel's grid); `draw(x0, y0, x1, y1, pixels)` sends one band
    /// and returns the ESP-IDF error.
    pub(super) fn present(
        &mut self,
        region: Rect,
        pixels: &mut dyn Pixels,
        draw: impl Fn(u16, u16, u16, u16, *const u16) -> i32,
    ) {
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
            pixels.rows(Block::band(x0, y, width, ByteOrder::BigEndian), band);
            let t1 = Instant::now();
            // The band is DMA-capable and stays untouched until the driver's next call: the other band is filled next,
            // and its draw waits for this transfer before it starts.
            let err = draw(
                x0 as u16,
                y as u16,
                x1 as u16,
                (y + h) as u16,
                band.as_ptr(),
            );
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
