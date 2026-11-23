#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

// The toolkit at the top level: `fusuma::Tree`, `fusuma::Button...`.
pub use fusuma_ui::*;

/// The drawing layer: the `Renderer` trait, geometry, colour, text and images.
pub use fusuma_render as render;

/// Animation: easing, tweens, keyframes, springs and decay.
pub use fusuma_anim as anim;

/// The renderer choice: which backend a machine draws with, from its hardware, with overrides.
pub use fusuma_platform as platform;

/// The renderer backends linked by the `renderer-*` features.
pub mod renderers {
    /// The integer, `no_std` renderer for microcontroller displays (`renderer-integer`).
    #[cfg(feature = "renderer-integer")]
    pub use fusuma_render_embedded as embedded;
    /// The `tiny-skia` renderer (`renderer-skia`).
    #[cfg(feature = "renderer-skia")]
    pub use fusuma_render_skia as skia;
    /// Vello on a GPU (`renderer-vello-gpu`).
    #[cfg(feature = "renderer-vello-gpu")]
    pub use fusuma_render_vello as vello_gpu;
    /// Vello on the CPU (`renderer-vello-cpu`).
    #[cfg(feature = "renderer-vello-cpu")]
    pub use fusuma_render_vello_cpu as vello_cpu;
}
