# Tokonoma

A small renderer abstraction for 2D user interfaces. A toolkit draws through one trait,
`Renderer`; backends implement it for the target.

| Crate | Backend | For |
|---|---|---|
| `tokonoma` | the trait, geometry, colors, paths, the conformance suite (`no_std`, no allocator) | everyone |
| `tokonoma-embedded` | integer drawing on `embedded-graphics` targets (`no_std`) | microcontroller displays |
| `tokonoma-skia` | anti-aliased CPU drawing on `tiny-skia` | desktop, phone, watch, screenshots |
| `tokonoma-vello` (planned) | GPU drawing on `vello` | desktop and phone with a GPU |

Tokonoma knows nothing about any operating system. Part of the Dodai family: Dodai OS is the
foundation, Fusuma the toolkit, Tokonoma the alcove where the picture is shown.

## Use

```rust
use tokonoma::{Color, Rect, Renderer};

fn card<R: Renderer>(r: &mut R) -> Result<(), R::Error> {
    r.clear(Color::rgb(16, 18, 24))?;
    r.fill_rect(Rect::new(8, 8, 120, 64), 12, Color::rgb(40, 46, 60))
}
```

## Development

`just check-all` runs the gate. Every backend runs `tokonoma::conformance::run_all` in its tests,
so they stay in step.

## License

MIT
