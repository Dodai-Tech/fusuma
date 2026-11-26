//! The built-in bitmap fonts live in `fusuma_render::bitmap`, shared with the integer renderer: three monospace sizes, Latin-1
//! (and with `bitmap-extended` Romanian, Central European, Cyrillic and Greek), needing no allocator and no font file.

pub use fusuma_render::bitmap::{
    MAX_SCALE, font_for, font_for_char, runs, scaled_text_size, text_scale, text_size,
};
