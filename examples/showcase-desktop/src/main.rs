//! The Fusuma components showcase in a window. The app is `fusuma-showcase`, the same code the tests draw and
//! the ESP32-P4 firmware runs; the window and the choice of renderer are `fusuma-platform-desktop`.
//!
//! `cargo run -p fusuma-showcase-desktop` picks the renderer from the hardware;
//! `-- --renderer vello-cpu` (or `vello-gpu`, or `auto`) overrides it. `RUST_LOG=info` shows the choice.

use fusuma_platform_desktop::{Backend, BackendSetting, Options, RenderLayer, run};
use fusuma_showcase::Showcase;

/// The `--renderer` flag as the build's layer of the choice.
fn layer(args: impl IntoIterator<Item = String>) -> anyhow::Result<RenderLayer> {
    let mut args = args.into_iter();
    let mut backend = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--renderer" => {
                let name = args.next().ok_or_else(|| {
                    anyhow::anyhow!("--renderer needs auto, vello-cpu or vello-gpu")
                })?;
                backend = Some(match name.as_str() {
                    "auto" => BackendSetting::Auto,
                    other => BackendSetting::Fixed(Backend::from_name(other).ok_or_else(|| {
                        anyhow::anyhow!("unknown renderer '{other}'; auto, vello-cpu or vello-gpu")
                    })?),
                });
            }
            other => anyhow::bail!("unknown argument '{other}'; only --renderer is known"),
        }
    }
    Ok(RenderLayer {
        backend,
        ..RenderLayer::EMPTY
    })
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    run(
        Showcase::new,
        Options {
            title: "Fusuma components",
            settings: Some(include_str!("../fusuma.toml")),
            build: layer(std::env::args().skip(1))?,
            ..Options::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn no_flag_says_nothing_and_a_flag_is_the_builds_layer() {
        assert_eq!(layer(args("")).unwrap(), RenderLayer::EMPTY);
        assert_eq!(
            layer(args("--renderer vello-cpu")).unwrap().backend,
            Some(BackendSetting::Fixed(Backend::VelloCpu))
        );
        assert_eq!(
            layer(args("--renderer auto")).unwrap().backend,
            Some(BackendSetting::Auto)
        );
    }

    #[test]
    fn mistakes_are_errors_that_name_the_problem() {
        let err = |s: &str| layer(args(s)).unwrap_err().to_string();
        assert!(err("--renderer vello").contains("unknown renderer 'vello'"));
        assert!(err("--renderer").contains("needs"));
        assert!(err("--bogus").contains("--bogus"));
    }
}
