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

Apps depend on one crate, `fusuma`, a facade: the toolkit plus the renderer choice and the backends behind cargo features (`renderer-vello-cpu` by default; `default-features = false` is `no_std`, so it fits an ESP).
The renderer is chosen for the target (a `std` feature will add the run-time choice between the GPU
and the CPU backend). Several renderers pass the same conformance suite and a device picks a default by
its hardware: the integer renderer (lightest), `tiny-skia` (anti-aliased, light), `vello_cpu` (richest) and the
Vello GPU backend. The
renderer crates stay separate, because each is useful alone.

| Crate | What | `no_std` | Status |
|---|---|---|---|
| [`fusuma-render`](crates/fusuma-render) | the `Renderer` trait, geometry, colors, paths, the conformance suite | yes | working |
| [`fusuma-render-embedded`](crates/fusuma-render-embedded) | integer drawing for small displays; kept: the lightest renderer and the default for chips that must keep RAM free | yes | working |
| [`fusuma-render-skia`](crates/fusuma-render-skia) | anti-aliased CPU drawing on `tiny-skia`; lighter and about 3-4x faster than `vello_cpu` on a chip without an FPU; `no_std` with `alloc` | partly | working, measured on the ESP32-C6 |
| [`fusuma-render-vello-cpu`](crates/fusuma-render-vello-cpu) | anti-aliased CPU drawing on `vello_cpu` | no | working |
| [`fusuma-render-vello`](crates/fusuma-render-vello) | anti-aliased GPU drawing on `vello_gpu` and `wgpu` | no | working |
| [`fusuma-render-vello-common`](crates/fusuma-render-vello-common) | shared code of the two Vello backends (internal) | no | working |
| [`fusuma`](crates/fusuma) | **the crate to add:** a facade over the toolkit, `render`, `anim`, `platform` and the backends, each backend one feature (`renderer-integer`, `renderer-skia`, `renderer-vello-cpu`, `renderer-vello-gpu`) | yes | working |
| [`fusuma-ui`](crates/fusuma-ui) | **the toolkit:** retained widget tree, layout, theme tokens, pointer and key events, focus, animation and paint passes; Label, Button, Checkbox, Radio, Switch, Slider, List item, Progress, a scrolling container, Row, Column, Spacer | yes | working |
| [`fusuma-anim`](crates/fusuma-anim) | easing, tweens, keyframes, springs, fling decay, an animator and drag helpers, in fixed point | yes | working (the animator and drag helpers are not in the published 0.2.0) |
| [`fusuma-platform`](crates/fusuma-platform) | **the out-of-the-box renderer choice:** which backend a machine draws with, from its screen and hardware (RAM, PSRAM, FPU, GPU), with overrides, a memory check and a reason for the answer | yes | working |
| `fusuma-platform-esp` | runs an `App` on an ESP-IDF board (display, touch, frame loop) and picks the renderer itself; today the Waveshare ESP32-P4 7" ([`crates/fusuma-platform-esp`](crates/fusuma-platform-esp)) | no | working |
| `fusuma-shaders` | shader contract and engine | no | planned |
| `fusuma-text` | real text: fonts, shaping, line breaking (on Parley) for desktop and phone; the bitmap fonts stay for microcontrollers | no | planned |
| `fusuma-platform-*` | microcontroller, desktop, Android, iOS, web, terminal | no | planned |

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
crates/fusuma-render-embedded      integer drawing
crates/fusuma-render-vello-common  the Renderer on a Vello scene, shared by the CPU and GPU backends
crates/fusuma-render-vello-cpu     anti-aliased drawing on vello_cpu
crates/fusuma-render-vello         anti-aliased drawing on vello_gpu (run the bench: cargo run --release -p fusuma-render-vello --example bench)
crates/fusuma                      the toolkit: retained widget tree, layout, theme, events, paint
crates/fusuma-anim                 easing, tweens, keyframes, springs, fling decay
docs/ROADMAP.md                    what is done and what comes next
fuzz/                              cargo-fuzz targets (geometry, draw, anim, ui)
scripts/                           Nushell scripts used by the justfile and CI
```

Part of the Dodai family: Hibari OS is a thin operating system for ESP32 boards, Fusuma draws the
interface, and Dodai UI is the design system.

## License

MIT
