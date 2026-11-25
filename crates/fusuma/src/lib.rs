#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

// The toolkit at the top level: `fusuma::Tree`, `fusuma::Button...`.
pub use fusuma_ui::*;

/// The drawing layer: the `Renderer` trait, geometry, colour, text and images.
pub use fusuma_render as render;

/// The text-cell output: a cell grid, a diff of two frames, an ANSI encoder and a key decoder, for a terminal, a
/// serial console or a character panel.
pub use fusuma_cells as cells;

/// Animation: easing, tweens, keyframes, springs and decay.
pub use fusuma_anim as anim;

/// The renderer choice: which backend a machine draws with, from its hardware, with overrides.
pub use fusuma_platform as platform;

/// Vector text: Parley layout over the bundled fonts (the `text` feature and a `font-*` one).
#[cfg(feature = "text")]
pub use fusuma_text as text;

/// The font families and presets the build linked, and the `FontSet` that picks among them (the `fonts` feature).
#[cfg(feature = "fonts")]
pub use fusuma_fonts as fonts;

/// The renderer backends linked by the `renderer-*` features.
pub mod renderers {
    /// The integer, `no_std` renderer for microcontroller displays (`renderer-integer`).
    #[cfg(feature = "renderer-integer")]
    pub use fusuma_render_embedded as embedded;
    /// Vello on a GPU (`renderer-vello-gpu`).
    #[cfg(feature = "renderer-vello-gpu")]
    pub use fusuma_render_vello as vello_gpu;
    /// Vello on the CPU (`renderer-vello-cpu`).
    #[cfg(feature = "renderer-vello-cpu")]
    pub use fusuma_render_vello_cpu as vello_cpu;
}
