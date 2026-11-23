# fusuma-render

[![Crates.io](https://img.shields.io/crates/v/fusuma-render.svg)](https://crates.io/crates/fusuma-render)
[![docs.rs](https://img.shields.io/docsrs/fusuma-render)](https://docs.rs/fusuma-render)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

A small renderer abstraction for 2D user interfaces. A toolkit draws through one trait,
`Renderer`; backends implement it for the target.

| Crate | Backend | For |
|---|---|---|
| `fusuma-render` | the trait, geometry, colors, paths, the conformance suite (`no_std`, no allocator) | everyone |
| `fusuma-render-vello-cpu` | anti-aliased CPU drawing on `vello_cpu` | desktop, phone, watch without a GPU; screenshots |
| `fusuma-render-vello` (planned) | GPU drawing on `vello_gpu` | desktop and phone with a GPU |

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

## Development

`just check-all` runs the gate. Every backend runs `fusuma_render::conformance::run_all` in its tests,
so they stay in step.

## License

MIT
