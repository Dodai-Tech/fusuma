# fusuma

A native Rust UI toolkit for screens from a microcontroller to a desktop. This is the one crate to add; the
rest is behind features.

```toml
fusuma = "0.3"                                                   # toolkit, plus Vello on the CPU
fusuma = { version = "0.3", default-features = false }           # no_std: the toolkit and the renderer trait only
fusuma = { version = "0.3", features = ["renderer-integer", "toml"] }
```

| Path | What |
|---|---|
| `fusuma::Tree`, `Button`, ... | the widget tree, layout, theme, events (crate `fusuma-ui`) |
| `fusuma::render` | the `Renderer` trait, geometry, colour, text (crate `fusuma-render`) |
| `fusuma::anim` | easing, tweens, springs (crate `fusuma-anim`) |
| `fusuma::text` | vector text over the bundled fonts, with a cache (crate `fusuma-text`; features `font-inter`, `font-inter-bold`, `font-jetbrains-mono`) |
| `fusuma::platform` | which renderer a machine draws with, and the settings that override it |
| `fusuma::renderers::*` | the backends, one feature each: `renderer-integer`, `renderer-vello-cpu`, `renderer-vello-gpu` |

Features are additive: if two crates in a build ask for different renderers, all of them are linked. The
smaller crates stay usable alone for a firmware that only needs drawing.
