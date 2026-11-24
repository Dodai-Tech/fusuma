//! The board: the DSI panel's frame buffer, backlight and the GT911 touch, through the C shim in
//! `components/wt_display` (copied from the WeatherTrack P4 firmware).

use esp_idf_svc::sys::wt_display as ffi;

pub const WIDTH: usize = ffi::WT_LCD_W as usize;
pub const HEIGHT: usize = ffi::WT_LCD_H as usize;

/// Bring up panel, backlight and touch.
pub(crate) fn init() -> anyhow::Result<()> {
    // SAFETY: called once at startup, before anything else touches the display.
    let err = unsafe { ffi::wt_display_init() };
    anyhow::ensure!(err == 0, "wt_display_init failed: esp_err {err}");
    Ok(())
}

/// The RGB565 frame buffer the DSI DMA scans out (`WIDTH * HEIGHT` pixels).
fn framebuffer() -> *mut u16 {
    // SAFETY: plain getter; the buffer lives as long as the display driver (the whole program).
    unsafe { ffi::wt_display_framebuffer() }
}

/// Write CPU-cached rows `[y0, y1)` back to PSRAM so the DMA shows them.
fn flush_rows(y0: u32, y1: u32) {
    // SAFETY: rows are clamped inside the shim.
    unsafe {
        ffi::wt_display_flush_rows(y0, y1);
    }
}

/// Panel backlight, 0..=100.
pub(crate) fn set_backlight(percent: i32) {
    // SAFETY: plain setter.
    unsafe { ffi::wt_display_backlight(percent) }
}

/// Samples the GT911: where the finger is, or `None` when there is none. Turning samples into pointer events
/// is `fusuma::TouchTracker`, shared with every platform.
pub(crate) fn touch_sample() -> Option<fusuma::render::Point> {
    let (mut x, mut y) = (0u16, 0u16);
    // SAFETY: out-pointers are valid for the call.
    let pressed = unsafe { ffi::wt_touch_read(&mut x, &mut y) };
    pressed.then(|| fusuma::render::Point::new(i32::from(x), i32::from(y)))
}

/// The Waveshare ESP32-P4 7" board as a `fusuma_platform_embedded::Board`: the DSI panel's frame buffer, which the
/// DMA scans out, and the GT911 touch panel.
pub(crate) struct P4Board {
    screen: &'static mut [u16],
}

impl P4Board {
    /// Brings the panel up and takes the frame buffer.
    pub(crate) fn new() -> anyhow::Result<Self> {
        init()?;
        let fb = framebuffer();
        anyhow::ensure!(!fb.is_null(), "no frame buffer");
        // SAFETY: the DSI driver owns WIDTH * HEIGHT RGB565 pixels at `fb` for the whole program, and nothing
        // else writes to it; this is the only reference.
        let screen = unsafe { std::slice::from_raw_parts_mut(fb, WIDTH * HEIGHT) };
        Ok(Self { screen })
    }
}

impl fusuma_platform_embedded::Board for P4Board {
    type Error = core::convert::Infallible;

    fn size(&self) -> (u32, u32) {
        (WIDTH as u32, HEIGHT as u32)
    }

    fn touch(&mut self) -> Result<Option<fusuma::render::Point>, Self::Error> {
        Ok(touch_sample())
    }

    fn present(
        &mut self,
        region: fusuma::render::Rect,
        pixels: &mut dyn fusuma_platform_embedded::Pixels,
    ) -> Result<(), Self::Error> {
        let (x0, x1) = (region.x.max(0) as usize, (region.right().max(0) as usize).min(WIDTH));
        let (y0, y1) = (region.y.max(0) as usize, (region.bottom().max(0) as usize).min(HEIGHT));
        for y in y0..y1 {
            pixels.row(y as u32, x0 as u32, &mut self.screen[y * WIDTH + x0..y * WIDTH + x1]);
        }
        flush_rows(y0 as u32, y1 as u32);
        Ok(())
    }
}
