#![doc = include_str!("../README.md")]
#![cfg_attr(not(feature = "std"), no_std)]

// The toolkit at the top level: `fusuma::Tree`, `fusuma::Button...`.
pub use fusuma_ui::*;

/// The drawing layer: the `Renderer` trait, geometry, colour, text and images.
pub use fusuma_render as render;

/// The text-cell output: a cell grid, a diff of two frames, an ANSI encoder and a key decoder, for a terminal, a
/// serial console or a character panel.
pub use fusuma_cells as cells;

mod elm;

// The Elm architecture at the top level: `fusuma::Program`, `fusuma::button`, `fusuma::column`. An explicit export wins over
// the glob of the toolkit below it, so a name both have (`Stepper`) is the Elm view here and the widget under `fusuma::ui`.
pub use elm::*;
// A name both layers have: here it is the Elm view; the toolkit's widget is `fusuma::ui::Stepper`.
pub use elm::Stepper;

/// The toolkit under the Elm layer (the retained tree, layout, widgets, themes), for what the views do not cover.
pub use fusuma_ui as ui;

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

/// Every Lucide icon as a typed [`Icon`], and the font and atlases that draw them. The font and
/// the atlases are linked by the `icons` feature.
pub use fusuma_icons as icons;
pub use fusuma_icons::Icon;

/// The renderer backends linked by the `renderer-*` features.
pub mod renderers {
    /// The integer, `no_std` renderer for microcontroller displays (`renderer-integer`).
    #[cfg(feature = "renderer-integer")]
    pub use fusuma_render_integer as embedded;
    /// Vello on a GPU (`renderer-vello-gpu`).
    #[cfg(feature = "renderer-vello-gpu")]
    pub use fusuma_render_vello as vello_gpu;
    /// Vello on the CPU (`renderer-vello-cpu`).
    #[cfg(feature = "renderer-vello-cpu")]
    pub use fusuma_render_vello_cpu as vello_cpu;
}
