//! The boards this crate runs on, one module each, chosen by a cargo feature. A board module provides `NAME`, `FPU`,
//! `open(percent) -> Panel` and a `Panel` that is a `fusuma_platform_embedded::Board` with an infallible error and a
//! `report()` of its own numbers.

#[cfg(all(feature = "board-p4-7in", feature = "board-s3-amoled-206"))]
compile_error!("choose one board feature: board-p4-7in or board-s3-amoled-206");

#[cfg(not(any(feature = "board-p4-7in", feature = "board-s3-amoled-206")))]
compile_error!("choose a board feature: board-p4-7in or board-s3-amoled-206");

#[cfg(feature = "board-p4-7in")]
pub(crate) mod p4_7in;
#[cfg(feature = "board-p4-7in")]
pub(crate) use p4_7in as current;

#[cfg(feature = "board-s3-amoled-206")]
pub(crate) mod s3_amoled_206;
#[cfg(feature = "board-s3-amoled-206")]
pub(crate) use s3_amoled_206 as current;
