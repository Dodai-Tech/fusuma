# Fusuma

[![CI](https://github.com/Dodai-Tech/fusuma/actions/workflows/ci.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/ci.yml)
[![Security](https://github.com/Dodai-Tech/fusuma/actions/workflows/security.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/security.yml)
[![Fuzz](https://github.com/Dodai-Tech/fusuma/actions/workflows/fuzz.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/fuzz.yml)
[![Crates.io](https://img.shields.io/crates/v/fusuma-render.svg)](https://crates.io/crates/fusuma-render)
[![docs.rs](https://img.shields.io/docsrs/fusuma-render)](https://docs.rs/fusuma-render)
[![MSRV](https://img.shields.io/badge/MSRV-1.90-blue.svg)](Cargo.toml)
[![no_std](https://img.shields.io/badge/no__std-yes-success.svg)](crates/fusuma-render)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A native Rust UI toolkit for everything from a microcontroller to a desktop, with no webview, no
HTML and no CSS. A fusuma (襖) is the sliding door of a Japanese room.

Apps depend on one crate, `fusuma` (planned), and the right renderer is chosen for the target:
the integer one on a microcontroller, the GPU one on a desktop or phone, the CPU one when there is
no usable GPU. The crates below are the parts it is built from.

| Crate | What | `no_std` | Status |
|---|---|---|---|
| [`fusuma-render`](crates/fusuma-render) | the `Renderer` trait, geometry, colors, paths, the conformance suite | yes | working |
| [`fusuma-render-embedded`](crates/fusuma-render-embedded) | integer drawing for small displays | yes | working |
| [`fusuma-render-vello-cpu`](crates/fusuma-render-vello-cpu) | anti-aliased CPU drawing on `vello_cpu` | no | working |
| [`fusuma-render-vello`](crates/fusuma-render-vello) | anti-aliased GPU drawing on `vello_gpu` and `wgpu` | no | working |
| [`fusuma-render-vello-common`](crates/fusuma-render-vello-common) | shared code of the two Vello backends (internal) | no | working |
| [`fusuma-core`](crates/fusuma-core) | retained widget tree, layout, theme tokens, pointer events, animation and paint passes; Label, Button, Row, Column, Spacer | yes | working, first slice |
| [`fusuma-anim`](crates/fusuma-anim) | easing, tweens, keyframes, springs, fling decay, in fixed point | yes | working |
| `fusuma-shaders` | shader contract and engine | no | planned |
| `fusuma-widgets`, `fusuma-text` | more widgets (toggle, slider, list, text field), real text and fonts | partly | planned |
| `fusuma-platform-*` | microcontroller, desktop, Android, iOS, web, terminal | no | planned |
| `fusuma` | the facade: one dependency, the backend chosen by target | no | planned |

See the [roadmap](docs/ROADMAP.md). A renderer crate never depends on a toolkit crate: a firmware
that needs only drawing depends on `fusuma-render-embedded` and gets no toolkit code. CI checks
this rule.

## Use

```rust
use fusuma_render::{Color, Rect, Renderer};

fn card<R: Renderer>(r: &mut R) -> Result<(), R::Error> {
    r.clear(Color::rgb(16, 18, 24))?;
    r.fill_rect(Rect::new(8, 8, 120, 64), 12, Color::rgb(40, 46, 60))
}
```

Every backend passes `fusuma_render::conformance::run_all`, so the same scene draws the same picture on
each of them.

## Development

`just check-all` runs the quality gate: fmt, clippy, tests, docs, unused dependencies and the
Nushell tests. `just fuzz-all` runs the fuzz targets (nightly).

## Layout

```
crates/fusuma-render               the Renderer trait, geometry, colors, paths, conformance suite
crates/fusuma-render-embedded      integer drawing for embedded-graphics targets
crates/fusuma-render-vello-common  the Renderer on a Vello scene, shared by the CPU and GPU backends
crates/fusuma-render-vello-cpu     anti-aliased drawing on vello_cpu
crates/fusuma-render-vello         anti-aliased drawing on vello_gpu (run the bench: cargo run --release -p fusuma-render-vello --example bench)
crates/fusuma-core                 retained widget tree, layout, theme, events, animation and paint passes
crates/fusuma-anim                 easing, tweens, keyframes, springs, fling decay
docs/ROADMAP.md                    what is done and what comes next
fuzz/                              cargo-fuzz targets (geometry, draw, anim, ui)
scripts/                           Nushell scripts used by the justfile and CI
```

Part of the Dodai family: Hibari OS is a thin operating system for ESP32 boards, Fusuma draws the
interface, and Dodai UI is the design system.

## License

MIT
