//! The display controller the board file names: one module per controller, compiled in by the build's
//! `fusuma_display` cfg. Each provides `Display` with `open()`, `present`, `brightness` and `report`.

#[cfg(fusuma_display = "co5300")]
mod co5300;
#[cfg(fusuma_display = "co5300")]
pub(crate) use co5300::Display;

#[cfg(fusuma_display = "ek79007")]
mod ek79007;
#[cfg(fusuma_display = "ek79007")]
pub(crate) use ek79007::Display;
