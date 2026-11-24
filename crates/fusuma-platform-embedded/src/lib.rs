#![doc = include_str!("../README.md")]
#![no_std]

extern crate alloc;

mod board;
mod driver;
mod surface;
mod target;

pub use board::{
    Board, ByteOrder, HELD_THROUGH_ERRORS, HeldTouch, Pixels, aligned, band_rows, orient, rgb565,
};
pub use driver::{Driver, DriverError, Step, Timing};
pub use surface::{DrawError, LINKED, Surface};
pub use target::{Arch, Chip, esp_hardware, hardware, has_fpu, has_simd, vello_has_simd};
