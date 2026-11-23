//! Fusuma render: a small renderer abstraction for 2D user interfaces.
//!
//! A toolkit draws through the [`Renderer`] trait: filled and stroked rectangles, circles and
//! paths, text, images, a clip and a translation. Backends implement it: Vello on the CPU
//! (`fusuma-render-vello-cpu`) and on a GPU (`fusuma-render-vello`).
//! The crate is `no_std`, needs no allocator and knows nothing about any operating system.
//!
//! With the `conformance` feature, [`conformance::run_all`] is the test-suite a backend runs to
//! show that it draws the same scene as the others.
#![no_std]

pub mod color;
pub mod geometry;
pub mod image;
pub mod path;
pub mod renderer;
pub mod text;

#[cfg(feature = "conformance")]
pub mod conformance;

pub use color::Color;
pub use geometry::{Point, Rect, Size};
pub use image::Image;
pub use path::{PathCmd, triangle};
pub use renderer::{Capabilities, Error, MIN_STATE_DEPTH, Probe, Renderer};
pub use text::{Align, TextStyle, left_edge};
