# fusuma-render-vello-cpu

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The Fusuma renderer backend for machines without a usable GPU, and for screenshots and tests:
anti-aliased CPU drawing on [`vello_cpu`](https://crates.io/crates/vello_cpu), which uses SIMD (its `multithreading`
feature is not used here yet: it is not known whether the partial-redraw path would use the threads). It draws the same way as the GPU backend, `fusuma-render-vello`, because
both are Vello and share one implementation of Fusuma's `Renderer` trait.

```rust
use fusuma_render::{Color, Rect, Renderer};

let mut r = fusuma_render_vello_cpu::new(200, 100).expect("a surface of this size");
r.clear(Color::rgb(16, 18, 24)).unwrap();
r.fill_rect(Rect::new(8, 8, 120, 64), 12, Color::rgb(40, 46, 60)).unwrap();
let png = fusuma_render_vello_cpu::png(&r).unwrap();
assert_eq!(&png[1..4], b"PNG");
```

Text is vector (Parley layout, drawn as glyph runs) with the `text` feature and the families `fusuma-fonts` links (`set_fonts` picks them), else the same bitmap fonts as `fusuma-render-integer`. Vello's glyph runs
(real fonts, shaping) come with the text stack.

`no_std` with `alloc` for a microcontroller: `default-features = false, features = ["libm"]` (the PNG output needs `std`). Built for the ESP32-S3 and the ESP32-C6 targets, run on the S3.

It renders only what a frame will touch (`will_draw`, damage in bands through one 128 KB buffer) and can move a scrolled
area's pixels instead of redrawing them (`shift_area`); both are checked pixel for pixel against a full render. A picture read
back inside an open clip keeps the clip (the scene is emptied after each render, and the open clips are pushed again).

It passes the shared `fusuma_render::conformance` suite and is fuzzed.
