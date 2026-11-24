#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

mod ansi;
mod cell;
mod decode;
mod diff;
mod grid;

pub use ansi::Encoder;
pub use cell::{Attrs, Cell, Color, Depth, Style};
pub use decode::{CellKey, Decoder};
pub use diff::{Changes, MERGE_GAP, Run, changes};
pub use grid::Grid;
