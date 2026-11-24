//! The Waveshare ESP32-S3-Touch-AMOLED-2.06 watch: the 410x502 CO5300 AMOLED over QSPI and the FT3168 touch panel,
//! through the C shim in `components/s3_amoled`.
//!
//! The panel has no frame buffer the chip can write into: every update is sent as a rectangle. It goes out in bands
//! of rows through two buffers in internal RAM, so one band is converted while the one before it is still being sent
//! (the shim's next command waits for that transfer to end).

use std::convert::Infallible;
use std::time::Instant;

use esp_idf_svc::sys::{MALLOC_CAP_DMA, MALLOC_CAP_INTERNAL, heap_caps_malloc, s3_amoled as ffi};
use fusuma::render::{Point, Rect};
use fusuma_platform_embedded::{Board, Pixels, aligned, band_rows};

const WIDTH: u32 = ffi::S3_LCD_W;
const HEIGHT: u32 = ffi::S3_LCD_H;

/// Pixels in one band buffer: 32 KB, the shim's largest transfer.
const BAND_PX: usize = 16 * 1024;

/// The CO5300 takes rectangles that start and end on even columns and rows.
const GRID: u32 = 2;

/// The corner radius of the watch's glass, in pixels. **A guess, not a measurement** (set by eye in Hibari): the
/// panel's data sheet does not say.
const CORNER_RADIUS: u32 = 70;

/// The board's name, for the log.
pub(crate) const NAME: &str = "Waveshare ESP32-S3-Touch-AMOLED-2.06 (410x502 AMOLED, FT3168)";

/// Whether the CPU has a floating-point unit (the S3's Xtensa cores have a single-precision one).
pub(crate) const FPU: bool = true;

/// Brings the board up with the brightness at `percent` (0 to 100).
pub(crate) fn open(percent: i32) -> anyhow::Result<Panel> {
    // SAFETY: called once at startup, before anything else touches the panel or the touch bus.
    let err = unsafe { ffi::s3_amoled_init() };
    anyhow::ensure!(err == 0, "s3_amoled_init failed: esp_err {err}");
    let level = u8::try_from(percent.clamp(0, 100) * 255 / 100).unwrap_or(u8::MAX);
    // SAFETY: plain setter after init.
    unsafe { ffi::s3_amoled_brightness(level) };
    Ok(Panel {
        bands: [band()?, band()?],
        next: 0,
        held: None,
        touch_errors: 0,
        draw_errors: 0,
        last_error: 0,
        bands_sent: 0,
        convert_us: 0,
        wait_us: 0,
        presents: 0,
        pixels: 0,
    })
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

/// The watch as a `fusuma_platform_embedded::Board`.
pub(crate) struct Panel {
    bands: [&'static mut [u16]; 2],
    /// The band to fill next; the other one may still be in flight.
    next: usize,
    /// Where the finger is: a failed read keeps it (the controller answers NAK now and then).
    held: Option<Point>,
    touch_errors: u32,
    draw_errors: u32,
    /// The ESP-IDF error code of the last failed transfer, and how many bands were sent.
    last_error: i32,
    bands_sent: u32,
    /// Since the last report: time spent turning the picture into big-endian RGB565 rows, time spent waiting for the
    /// panel (the transfer the conversion did not hide), updates and pixels sent.
    convert_us: u64,
    wait_us: u64,
    presents: u32,
    pixels: u64,
}

impl Panel {
    /// Where `present`'s time went since the last report, then starts counting again.
    pub(crate) fn report(&mut self) -> Option<String> {
        let n = u64::from(self.presents.max(1));
        let line = format!(
            "per update: convert {} us, waiting for the panel {} us, {} px; touch bus errors {}, panel errors {} of {} bands (last esp_err {:#x})",
            self.convert_us / n,
            self.wait_us / n,
            self.pixels / n,
            self.touch_errors,
            self.draw_errors,
            self.bands_sent,
            self.last_error
        );
        (self.convert_us, self.wait_us, self.presents, self.pixels) = (0, 0, 0, 0);
        Some(line)
    }
}

impl Board for Panel {
    type Error = Infallible;

    fn size(&self) -> (u32, u32) {
        (WIDTH, HEIGHT)
    }

    fn shape(&self) -> fusuma::Shape {
        fusuma::Shape::Rounded(CORNER_RADIUS)
    }

    fn touch(&mut self) -> Result<Option<Point>, Infallible> {
        let (mut down, mut x, mut y) = (false, 0u16, 0u16);
        // SAFETY: out-pointers are valid for the call.
        if unsafe { ffi::s3_touch_read(&mut down, &mut x, &mut y) } {
            self.held = down.then(|| Point::new(i32::from(x), i32::from(y)));
        } else {
            self.touch_errors += 1;
        }
        Ok(self.held)
    }

    fn present(&mut self, region: Rect, pixels: &mut dyn Pixels) -> Result<(), Infallible> {
        let Some(r) = aligned(region, WIDTH, HEIGHT, GRID) else {
            return Ok(());
        };
        let (x0, y0, x1, y1) = (r.x as u32, r.y as u32, r.right() as u32, r.bottom() as u32);
        let width = x1 - x0;
        // Rows a multiple of the grid, so every band starts on an even row too.
        let rows = band_rows(width, BAND_PX as u32, GRID);
        let mut y = y0;
        while y < y1 {
            let h = rows.min(y1 - y);
            let len = (width * h) as usize;
            let band = &mut self.bands[self.next][..len];
            let t0 = Instant::now();
            for (i, row) in band.chunks_exact_mut(width as usize).enumerate() {
                pixels.row(y + i as u32, x0, row);
            }
            // The panel takes big-endian pixels.
            for p in band.iter_mut() {
                *p = p.to_be();
            }
            let t1 = Instant::now();
            // SAFETY: the band is DMA-capable and stays untouched until the shim's next call: the other band is
            // filled next, and its `s3_amoled_draw` waits for this transfer before it starts.
            let err = unsafe {
                ffi::s3_amoled_draw(
                    x0 as u16,
                    y as u16,
                    x1 as u16,
                    (y + h) as u16,
                    band.as_ptr(),
                )
            };
            self.bands_sent += 1;
            if err != 0 {
                self.draw_errors += 1;
                self.last_error = err;
            }
            self.convert_us += t1.duration_since(t0).as_micros() as u64;
            self.wait_us += t1.elapsed().as_micros() as u64;
            self.next ^= 1;
            y += h;
        }
        self.presents += 1;
        self.pixels += u64::from(width) * u64::from(y1 - y0);
        Ok(())
    }
}
