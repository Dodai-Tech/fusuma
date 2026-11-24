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
its hardware: Vello on the GPU, falling back to `vello_cpu`, where Vello has SIMD (desktops, phones, the web); the
integer renderer on microcontrollers, where Vello runs a slow scalar fallback. The
renderer crates stay separate, because each is useful alone.

| Crate | What | `no_std` | Status |
|---|---|---|---|
| [`fusuma-render`](crates/fusuma-render) | the `Renderer` trait, geometry, colors, paths, the conformance suite | yes | working |
| [`fusuma-render-embedded`](crates/fusuma-render-embedded) | integer drawing straight into RGB565: the default on every microcontroller (several times faster than Vello there); anti-aliased text and edges to come | yes | working |
| [`fusuma-render-vello-cpu`](crates/fusuma-render-vello-cpu) | anti-aliased CPU drawing on `vello_cpu` | with `libm` | working |
| [`fusuma-render-vello`](crates/fusuma-render-vello) | anti-aliased GPU drawing on `vello_gpu` and `wgpu` | no | working |
| [`fusuma-render-vello-common`](crates/fusuma-render-vello-common) | shared code of the two Vello backends (internal) | with `libm` | working |
| [`fusuma`](crates/fusuma) | **the crate to add:** a facade over the toolkit, `render`, `anim`, `platform` and the backends, each backend one feature (`renderer-integer`, `renderer-vello-cpu`, `renderer-vello-gpu`) | yes | working |
| [`fusuma-ui`](crates/fusuma-ui) | **the toolkit:** retained widget tree, layout, theme tokens, pointer and key events, focus, animation and paint passes; Label, Button, Checkbox, Radio, Switch, Slider, List item, Progress, a scrolling container, Row, Column, Spacer | yes | working |
| [`fusuma-anim`](crates/fusuma-anim) | easing, tweens, keyframes, springs, fling decay, an animator and drag helpers, in fixed point | yes | working (the animator and drag helpers are not in the published 0.2.0) |
| [`fusuma-platform`](crates/fusuma-platform) | **the out-of-the-box renderer choice:** which backend a machine draws with, from its screen and hardware (RAM, PSRAM, FPU, GPU), with overrides, a memory check and a reason for the answer | yes | working |
| [`fusuma-platform-embedded`](crates/fusuma-platform-embedded) | **the part every board shares**: a `Board` trait (screen size, touch sample, show a region as RGB565 rows), the renderer surface and the frame step; `no_std`, tested on a computer with a fake board. Builds for RISC-V, Cortex-M0+/M4F and bare-metal ARM64 | no | working; the P4 runs on it |
| `fusuma-platform-esp` | runs an `App` on an ESP-IDF board (display, touch, frame loop) and picks the renderer itself; today the Waveshare ESP32-P4 7" ([`crates/fusuma-platform-esp`](crates/fusuma-platform-esp)) | no | working |
| [`fusuma-platform-desktop`](crates/fusuma-platform-desktop) | runs an `App` in a window on Linux, macOS and Windows (winit, softbuffer) with mouse, touch and keyboard, and picks the renderer itself: Vello on the GPU if there is one, else on the CPU | no | working on Linux; macOS and Windows not tried |
| `fusuma-shaders` | shader contract and engine | no | planned |
| [`fusuma-text`](crates/fusuma-text) | real text: Parley layout and measuring over the bundled fonts, cached; `no_std` with an allocator, so it runs on a P4. Single-line so far (Parley wraps; no widget uses it yet) | no | working |
| [`fusuma-fonts`](crates/fusuma-fonts) | the bundled fonts as `&'static [u8]`: Inter, Inter Bold, JetBrains Mono (Latin subsets, SIL OFL), one cargo feature each | yes | working |
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
