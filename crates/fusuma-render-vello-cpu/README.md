# fusuma-render-vello-cpu

[![Crates.io](https://img.shields.io/crates/v/fusuma-render-vello-cpu.svg)](https://crates.io/crates/fusuma-render-vello-cpu)
[![docs.rs](https://img.shields.io/docsrs/fusuma-render-vello-cpu)](https://docs.rs/fusuma-render-vello-cpu)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The Fusuma renderer backend for machines without a usable GPU, and for screenshots and tests:
anti-aliased CPU drawing on [`vello_cpu`](https://crates.io/crates/vello_cpu), which uses SIMD and
can use several threads. It draws the same way as the GPU backend, `fusuma-render-vello`, because
both are Vello and share one implementation of Fusuma's `Renderer` trait.

```rust
use fusuma_render::{Color, Rect, Renderer};

let mut r = fusuma_render_vello_cpu::new(200, 100).expect("a surface of this size");
r.clear(Color::rgb(16, 18, 24)).unwrap();
r.fill_rect(Rect::new(8, 8, 120, 64), 12, Color::rgb(40, 46, 60)).unwrap();
let png = fusuma_render_vello_cpu::png(&r).unwrap();
assert_eq!(&png[1..4], b"PNG");
```

Text is vector (Parley layout, drawn as glyph runs) when built with a font feature (`inter`, `inter-bold`, `jetbrains-mono`, which imply `text`), else the same bitmap fonts as `fusuma-render-embedded`. Vello's glyph runs
(real fonts, shaping) come with the text stack.

`no_std` with `alloc` for a microcontroller: `default-features = false, features = ["libm"]` (the PNG output needs `std`). Built for the ESP32-S3 and the ESP32-C6 targets, run on the S3.

It passes the shared `fusuma_render::conformance` suite.
