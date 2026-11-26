//! The display controller the board file names: one module per controller family, compiled in by the build's
//! `fusuma_display` cfg. Each provides `Display` with `open()`, `present`, `brightness` and `report`.

#[cfg(any(
    fusuma_display = "co5300",
    fusuma_display = "sh8601",
    fusuma_display = "rm690b0",
    fusuma_display = "st7789",
    fusuma_display = "gc9a01"
))]
mod bands;

#[cfg(any(
    fusuma_display = "co5300",
    fusuma_display = "sh8601",
    fusuma_display = "rm690b0"
))]
mod amoled;
#[cfg(any(
    fusuma_display = "co5300",
    fusuma_display = "sh8601",
    fusuma_display = "rm690b0"
))]
pub(crate) use amoled::Display;

#[cfg(any(fusuma_display = "st7789", fusuma_display = "gc9a01"))]
mod lcd;
#[cfg(any(fusuma_display = "st7789", fusuma_display = "gc9a01"))]
pub(crate) use lcd::Display;

#[cfg(fusuma_display = "ek79007")]
mod ek79007;
#[cfg(fusuma_display = "ek79007")]
pub(crate) use ek79007::Display;
