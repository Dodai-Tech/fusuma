//! The Fusuma components showcase on the Waveshare ESP32-P4 7" board.
//!
//! The app is `fusuma-showcase`, the same code the host tests draw. Everything else (display, touch, the
//! frame loop and which renderer to draw with) is `fusuma-platform-esp`.

use fusuma_platform_esp::{Options, run};
use fusuma_showcase::Showcase;

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    run(Showcase::new, Options::default())
}
