//! The Fusuma components showcase on an ESP32 board: the one `FUSUMA_BOARD` names (see `scripts/showcase.nu`).
//!
//! The app is `fusuma-showcase`, the same code the host tests draw. Everything else (display, touch, the
//! frame loop and which renderer to draw with) is `fusuma-platform-esp`. The renderer is automatic; the
//! project's `fusuma.toml` can pin one, and a cargo feature (`render-integer`,
//! `render-vello-cpu`) overrides both.

use fusuma_platform_esp::{Backend, BackendSetting, Options, RenderLayer, run};
use fusuma_showcase::Showcase;

/// The renderer the build asks for, if any; the lightest when several features are on.
const fn requested() -> Option<Backend> {
    if cfg!(feature = "render-integer") {
        Some(Backend::Integer)
    } else if cfg!(feature = "render-vello-cpu") {
        Some(Backend::VelloCpu)
    } else {
        None
    }
}

/// How long a pass of the pages' views takes on this chip, logged once at start (the `demo` build only).
#[cfg(feature = "demo")]
fn bench_view(app: &mut Showcase) {
    use std::time::Instant;
    for page in ["Foundations", "Buttons", "List", "Navigation"] {
        app.set_start(Some(page), None, None, true, false);
        let n = 300u32;
        let t = Instant::now();
        for _ in 0..n {
            app.view_pass();
        }
        let same = t.elapsed() / n;
        let t = Instant::now();
        for _ in 0..n {
            app.view_pass_changed();
        }
        let changed = t.elapsed() / n;
        log::info!(
            "view pass {page}: {} nodes, nothing changed {} us, a value changed {} us",
            app.node_count(),
            same.as_micros(),
            changed.as_micros()
        );
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    let build = RenderLayer {
        backend: match requested() {
            Some(b) => Some(BackendSetting::Fixed(b)),
            None => None,
        },
        ..RenderLayer::EMPTY
    };
    run(
        |w, h| {
            #[cfg(feature = "demo")]
            {
                // The demo build waits a few seconds so a log being read after a flash catches the numbers, then times a
                // pass of the pages' views on an app of its own before the real one starts.
                std::thread::sleep(std::time::Duration::from_secs(12));
                bench_view(&mut Showcase::new(w, h));
            }
            Showcase::new(w, h)
        },
        Options {
            settings: Some(include_str!("../fusuma.toml")),
            build,
            ..Options::default()
        },
    )
}
