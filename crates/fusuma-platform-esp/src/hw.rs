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
pub fn framebuffer() -> *mut u16 {
    // SAFETY: plain getter; the buffer lives as long as the display driver (the whole program).
    unsafe { ffi::wt_display_framebuffer() }
}

/// Write CPU-cached rows `[y0, y1)` back to PSRAM so the DMA shows them.
pub fn flush_rows(y0: u32, y1: u32) {
    // SAFETY: rows are clamped inside the shim.
    unsafe {
        ffi::wt_display_flush_rows(y0, y1);
    }
}

/// Panel backlight, 0..=100.
pub fn set_backlight(percent: i32) {
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
