//! The backlight the board file names (the build's `fusuma_backlight` cfg), or none.

/// Brings it up; nothing when the board has none.
pub(crate) fn init() {
    #[cfg(fusuma_backlight = "ledc")]
    {
        use crate::board::{self, BACKLIGHT_INVERT, BACKLIGHT_OPTIONS, BACKLIGHT_PINS};
        let freq = u32::try_from(board::option(BACKLIGHT_OPTIONS, "freq_hz")).unwrap_or(5000);
        // SAFETY: called once at startup.
        let err = unsafe {
            esp_idf_svc::sys::fusuma_esp::fusuma_ledc_backlight_init(
                board::pin(BACKLIGHT_PINS, "pwm"),
                BACKLIGHT_INVERT,
                freq,
            )
        };
        if err != 0 {
            log::warn!("backlight init failed: esp_err {err}");
        }
    }
}

/// Sets it, 0 to 100; false when the board has none.
pub(crate) fn set(percent: i32) -> bool {
    #[cfg(fusuma_backlight = "ledc")]
    {
        // SAFETY: plain setter.
        unsafe { esp_idf_svc::sys::fusuma_esp::fusuma_ledc_backlight_set(percent) };
        true
    }
    #[cfg(not(fusuma_backlight = "ledc"))]
    {
        let _ = percent;
        false
    }
}
