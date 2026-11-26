# fusuma-render

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

A small renderer abstraction for 2D user interfaces. A toolkit draws through one trait,
`Renderer`; backends implement it for the target.

| Crate | Backend | For |
|---|---|---|
| `fusuma-render` | the trait, geometry, colors, paths, the conformance suite (`no_std`, no allocator) | everyone |
| `fusuma-render-integer` | integer drawing on `embedded-graphics` targets (`no_std`) | microcontroller displays |
| `fusuma-render-vello-cpu` | anti-aliased CPU drawing on `vello_cpu` | desktop, phone, watch without a GPU; screenshots |
| `fusuma-render-vello` | GPU drawing on `vello_gpu` and `wgpu` (reads back, or presents to a surface) | desktop, phone and web with a GPU |
| `fusuma-render-vello-common` | the one mapping from `Renderer` to Vello that the two above share | (not used directly) |

The renderer layer of [Fusuma](https://github.com/Dodai-Tech/fusuma). It knows nothing about any
operating system and nothing about the toolkit above it.

## Use

```rust
use fusuma_render::{Color, Rect, Renderer};

fn card<R: Renderer>(r: &mut R) -> Result<(), R::Error> {
    r.clear(Color::rgb(16, 18, 24))?;
    r.fill_rect(Rect::new(8, 8, 120, 64), 12, Color::rgb(40, 46, 60))
}
```

## What a backend offers beyond the drawing calls

The trait has optional parts with a safe default, so a backend that cannot do them still conforms: `shift_area` (move the
pixels of a scrolled area instead of redrawing them; `false` means "draw it") and `will_draw` (tell a CPU renderer which part a
frame will touch, so it rasterises only that). `Capabilities` says whether alpha and anti-aliasing are real.

Not in the trait yet: gradients, shadows, blur, opacity layers and shaders (planned, see the roadmap's *Effects and shaders*).

## Development

`just check-all` runs the gate. Every backend runs `fusuma_render::conformance::run_all` in its tests, so they stay in step.
The one-bit target of `fusuma-render-integer` (`MonoFrame`) cannot: the suite compares colours, and one bit has two. Its tests
draw the same scenes on it and on the colour frame and compare after the threshold. All of them are fuzzed (`fuzz/draw`).

## License

MIT
