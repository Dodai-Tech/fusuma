//! The Fusuma components showcase on the Waveshare ESP32-P4 7" board.
//!
//! The app is `fusuma-showcase`, the same code the host tests draw. Everything else (display, touch, the
//! frame loop and which renderer to draw with) is `fusuma-platform-esp`. By default it picks the renderer
//! from the hardware; a cargo feature (`render-integer`, `render-skia`, `render-vello-cpu`) forces one.

use fusuma_platform_esp::{Backend, Options, run};
use fusuma_showcase::Showcase;

/// The renderer the build asks for, if any; the lightest when several features are on.
const fn requested() -> Option<Backend> {
    if cfg!(feature = "render-integer") {
        Some(Backend::Integer)
    } else if cfg!(feature = "render-skia") {
        Some(Backend::Skia)
    } else if cfg!(feature = "render-vello-cpu") {
        Some(Backend::VelloCpu)
    } else {
        None
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    run(
        Showcase::new,
        Options {
            backend: requested(),
            ..Options::default()
        },
    )
}
