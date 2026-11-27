# fusuma-render-vello

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

`GpuScene` keeps the picture in a texture of its own: a frame draws only what was drawn since the last one, over it, and a
scroll moves the rows by a texture copy. It reads back the part that changed, which is what screenshots, tests and
toolkits that composite on the CPU need. `GpuScene::present` and `from_device` copy the whole picture into a surface or
into another `wgpu` pipeline with no read-back (the web build does), so a swap chain needs no history. Text is vector (Inter and friends through `fusuma-text`) when built with `inter` or another font feature, else the built-in bitmap fonts.

It passes the shared `fusuma_render::conformance` suite on a real GPU; a machine without a GPU
cannot run those tests, and they skip there.
