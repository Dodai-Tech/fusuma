//! The touch controller the board file names (the build's `fusuma_touch` cfg), or none. `read` gives one sample as
//! the controller reports it; the board's mirror and swap are applied by `EspBoard`.

/// Brings the controller up; false when it does not answer (the panel then works without touch).
pub(crate) fn init() -> bool {
    imp::init()
}

/// One sample: `Some((down, x, y))`, or `None` when the bus failed.
pub(crate) fn read() -> Option<(bool, u16, u16)> {
    imp::read()
}

#[cfg(any(
    fusuma_touch = "ft3168",
    fusuma_touch = "ft5x06",
    fusuma_touch = "cst816",
    fusuma_touch = "gt911",
    fusuma_touch = "cst92xx"
))]
mod imp {
    use esp_idf_svc::sys::fusuma_esp as ffi;

    use crate::board::{self, HEIGHT, TOUCH_PINS as PINS, WIDTH};

    pub(super) fn init() -> bool {
        let cfg = ffi::fusuma_touch_config_t {
            sda: board::pin(PINS, "sda"),
            scl: board::pin(PINS, "scl"),
            rst: board::pin(PINS, "rst"),
            irq: board::pin(PINS, "int"),
            hz: 400_000,
            width: WIDTH as u16,
            height: HEIGHT as u16,
        };
        // SAFETY: called once at startup; `cfg` lives for the call.
        let err = unsafe {
            #[cfg(fusuma_touch = "ft3168")]
            {
                ffi::fusuma_ft3168_init(&cfg)
            }
            #[cfg(fusuma_touch = "ft5x06")]
            {
                ffi::fusuma_ft5x06_init(&cfg)
            }
            #[cfg(fusuma_touch = "cst816")]
            {
                ffi::fusuma_cst816_init(&cfg)
            }
            #[cfg(fusuma_touch = "gt911")]
            {
                ffi::fusuma_gt911_init(&cfg)
            }
            #[cfg(fusuma_touch = "cst92xx")]
            {
                ffi::fusuma_cst92xx_init(&cfg)
            }
        };
        if err != 0 {
            log::warn!("touch init failed (esp_err {err}); the panel works without it");
        }
        err == 0
    }

    pub(super) fn read() -> Option<(bool, u16, u16)> {
        let (mut down, mut x, mut y) = (false, 0u16, 0u16);
        // SAFETY: out-pointers are valid for the call.
        let ok = unsafe {
            #[cfg(any(
                fusuma_touch = "ft3168",
                fusuma_touch = "ft5x06",
                fusuma_touch = "cst816"
            ))]
            {
                ffi::fusuma_ft3168_read(&mut down, &mut x, &mut y)
            }
            #[cfg(fusuma_touch = "gt911")]
            {
                ffi::fusuma_gt911_read(&mut down, &mut x, &mut y)
            }
            #[cfg(fusuma_touch = "cst92xx")]
            {
                ffi::fusuma_cst92xx_read(&mut down, &mut x, &mut y)
            }
        };
        ok.then_some((down, x, y))
    }
}

#[cfg(not(any(
    fusuma_touch = "ft3168",
    fusuma_touch = "ft5x06",
    fusuma_touch = "cst816",
    fusuma_touch = "gt911",
    fusuma_touch = "cst92xx"
)))]
mod imp {
    pub(super) fn init() -> bool {
        false
    }

    pub(super) fn read() -> Option<(bool, u16, u16)> {
        Some((false, 0, 0))
    }
}
