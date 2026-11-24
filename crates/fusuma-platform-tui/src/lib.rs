#![doc = include_str!("../README.md")]

mod depth;
mod input;
mod terminal;

pub use depth::{depth_from, detect_depth};
pub use fusuma_cells as cells;
pub use input::{WHEEL_ROWS, map_event};
pub use terminal::{Frame, Terminal, blank};
