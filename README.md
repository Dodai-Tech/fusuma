# Fusuma

[![CI](https://github.com/Dodai-Tech/fusuma/actions/workflows/ci.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/ci.yml)
[![Security](https://github.com/Dodai-Tech/fusuma/actions/workflows/security.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/security.yml)
[![Fuzz](https://github.com/Dodai-Tech/fusuma/actions/workflows/fuzz.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/fuzz.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A native Rust UI toolkit for everything from a microcontroller to a desktop, with no webview, no
HTML and no CSS. Fusuma (襖) is the sliding door of a Japanese room; Tokonoma (床の間) is the
alcove where the picture is shown.

Two layers live in this repository, as separate crates with their own names:

| Layer | Crates | Status |
|---|---|---|
| Renderer: draws shapes, paths, text and images | [`tokonoma`](crates/tokonoma) (the `Renderer` trait, `no_std`), [`tokonoma-embedded`](crates/tokonoma-embedded) (integer, `no_std`, for small displays), [`tokonoma-skia`](crates/tokonoma-skia) (anti-aliased, `tiny-skia`); `tokonoma-vello` (GPU) is planned | working, fuzzed, one shared conformance suite |
| Toolkit: widgets, layout, events, animation | `fusuma-*` | planned |

A renderer crate never depends on a toolkit crate: a project that needs only drawing, a
microcontroller firmware for instance, depends on `tokonoma-embedded` and gets no toolkit code. CI
checks this rule.

## Use

```rust
use tokonoma::{Color, Rect, Renderer};

fn card<R: Renderer>(r: &mut R) -> Result<(), R::Error> {
    r.clear(Color::rgb(16, 18, 24))?;
    r.fill_rect(Rect::new(8, 8, 120, 64), 12, Color::rgb(40, 46, 60))
}
```

Every backend passes `tokonoma::conformance::run_all`, so the same scene draws the same picture on
each of them.

## Development

`just check-all` runs the quality gate: fmt, clippy, tests, docs, unused dependencies and the
Nushell tests. `just fuzz-all` runs the fuzz targets (nightly).

## Layout

```
crates/tokonoma            the Renderer trait, geometry, colors, paths, conformance suite
crates/tokonoma-embedded   integer drawing for embedded-graphics targets
crates/tokonoma-skia       anti-aliased drawing on tiny-skia
fuzz/                      cargo-fuzz targets (geometry, draw)
scripts/                   Nushell scripts used by the justfile and CI
```

Part of the Dodai family: Dodai OS is the foundation, Fusuma and Tokonoma draw the interface, and
Dodai UI is the design system.

## License

MIT
