# fusuma-render-vello

[![Crates.io](https://img.shields.io/crates/v/fusuma-render-vello.svg)](https://crates.io/crates/fusuma-render-vello)
[![docs.rs](https://img.shields.io/docsrs/fusuma-render-vello)](https://docs.rs/fusuma-render-vello)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The Fusuma renderer backend for machines with a GPU: anti-aliased drawing on
[`vello_gpu`](https://crates.io/crates/vello_gpu) and [`wgpu`](https://crates.io/crates/wgpu)
(Vulkan, Metal, DirectX). It draws the same way as `fusuma-render-vello-cpu`, because both share one
implementation of Fusuma's `Renderer` trait.

```rust,no_run
use fusuma_render::{Color, Rect, Renderer};

// `None` when there is no usable GPU: use `fusuma-render-vello-cpu` then.
let mut r = fusuma_render_vello::new(200, 100).expect("a GPU");
r.clear(Color::rgb(16, 18, 24)).unwrap();
r.fill_rect(Rect::new(8, 8, 120, 64), 12, Color::rgb(40, 46, 60)).unwrap();
```

This crate renders into a texture of its own and reads it back, which is what screenshots, tests and
toolkits that composite on the CPU need. Rendering into a texture that stays on the GPU (a window
surface, or another `wgpu` pipeline) is planned. Text uses the built-in bitmap fonts for now.

It passes the shared `fusuma_render::conformance` suite on a real GPU; a machine without a GPU
cannot run those tests, and they skip there.
