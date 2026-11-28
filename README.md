# Fusuma

[![CI](https://github.com/Dodai-Tech/fusuma/actions/workflows/ci.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/ci.yml)
[![Security](https://github.com/Dodai-Tech/fusuma/actions/workflows/security.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/security.yml)
[![Fuzz](https://github.com/Dodai-Tech/fusuma/actions/workflows/fuzz.yml/badge.svg)](https://github.com/Dodai-Tech/fusuma/actions/workflows/fuzz.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.90-blue.svg)](Cargo.toml)
[![no_std](https://img.shields.io/badge/no__std-yes-success.svg)](crates/fusuma-ui)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A native Rust UI toolkit for everything from a microcontroller to a desktop, with no webview, no HTML and no CSS. A
fusuma (襖) is the sliding door of a Japanese room.

One app runs in a window and on an ESP32 board: the same widgets, layout, focus, scrolling, dialogs and animation,
laid out by screen size. The renderer is picked for the device: Vello (GPU, else CPU) where it has SIMD, and on a
microcontroller an integer renderer with smooth text and shapes of its own. Fonts, icons, colours and boards are
data (`fusuma.toml`, a font catalog, `board.toml`), not code.

Not published yet: breaking changes are fine.

## Use

Apps add one crate, `fusuma`:

```rust
use fusuma::{ButtonVariant, Style, TextRole, Tree};

fn build(t: &mut Tree<64>) -> Result<(), fusuma::BuildError> {
    let root = t.column(None, Style::new())?;
    t.label(Some(root), "Hello", TextRole::Title)?;
    t.button(Some(root), "Press me", ButtonVariant::Primary)?;
    Ok(())
}
```

```toml
# fusuma.toml: every key is optional
[render]
backend = "auto"      # auto, integer, vello-cpu or vello-gpu

[fonts]
preset = "legible"    # fusuma, material, legible, plex or noto

[theme]
palette = "washi"     # 27: washi (default, vermilion on sumi and washi) and washi-indigo/-matcha/-kincha/-sakura, cyberdeck and cyberdeck-orange/-cyan/-lime/-magenta/-amber/-violet, fusuma (blue), forest, ocean, petrol, violet, ember, graphite, rose, amber, mint, sky, indigo, slate, crimson, lime
```

Two designs ship, each with its own palettes: **Fusuma** (named for the sliding paper door: sumi ink and washi paper, one
vermilion accent, small corners, a lattice line inside cards, a seal bar on the chosen row) and **Cyberdeck** (near-black,
mono text, outlines, ticks and hatch). The showcase switches both at run time. Design tokens, a spec and an OpenPencil/Figma file are generated from the theme: see [docs/DESIGN.md](docs/DESIGN.md).

`examples/showcase` is a complete app: seven pages, a dialog, a text field, icons, a palette switcher.

```
just showcase-desktop          # in a window
just showcase-esp /dev/ttyACM0 # on the connected board (detected), flashed to an OTA slot
just showcase-esp-restore      # boot the board's factory app again
```

## Crates

| Crate | What | `no_std` |
|---|---|---|
| [`fusuma`](crates/fusuma) | **the crate to add**: the toolkit, the renderer choice and the backends behind features (`renderer-*`, `fonts-*`, `icons`, `toml`) | with `default-features = false` |
| [`fusuma-ui`](crates/fusuma-ui) | the toolkit: widget tree, layout, theme and palettes, events, focus, scrolling, dialogs, text input, paint | yes, no allocator |
| [`fusuma-anim`](crates/fusuma-anim) | easing, tweens, keyframes, springs, fling decay, clips, in fixed point | yes |
| [`fusuma-render`](crates/fusuma-render) | the `Renderer` trait, geometry, colours, paths, the conformance suite every backend passes | yes |
| [`fusuma-render-integer`](crates/fusuma-render-integer) | the integer renderer for microcontrollers: RGB565, smooth text from glyph atlases, smooth shapes | yes |
| [`fusuma-render-vello-cpu`](crates/fusuma-render-vello-cpu), [`fusuma-render-vello`](crates/fusuma-render-vello) | Vello on the CPU and on the GPU (`wgpu`), sharing [`-vello-common`](crates/fusuma-render-vello-common) | CPU with `libm` |
| [`fusuma-text`](crates/fusuma-text) | Parley layout over the linked fonts | with alloc |
| [`fusuma-icons`](crates/fusuma-icons) | every [Lucide](https://lucide.dev/icons/) icon (2431 names) by name and as a constant, as an icon family for any renderer | yes |
| [`fusuma-fonts`](crates/fusuma-fonts) | the font catalog: ten OFL families, five presets, Material Symbols icons, one feature each; glyph atlases | yes |
| [`fusuma-platform`](crates/fusuma-platform) | the renderer choice, `fusuma.toml` and board files (catalog: [docs/BOARDS.md](docs/BOARDS.md)) | yes |
| [`fusuma-platform-desktop`](crates/fusuma-platform-desktop) | a window on Linux, macOS and Windows (`winit`) | no |
| [`fusuma-platform-embedded`](crates/fusuma-platform-embedded) | what every board shares: `Board`, `Driver`, the frame step | yes |
| [`fusuma-access`](crates/fusuma-access) | the semantic tree as an AccessKit tree, and what a screen reader asks as calls on the tree (unpublished, std) | no |
| [`fusuma-platform-esp`](crates/fusuma-platform-esp) | ESP-IDF boards described by files, drivers per controller (CO5300, SH8601, RM690B0, ST7789, GC9A01, EK79007, FT3168, FT5x06, CST816, CST92xx, GT911) | no |
| [`fusuma-cells`](crates/fusuma-cells), [`fusuma-platform-tui`](crates/fusuma-platform-tui) | scaffolds of the text UI: a cell grid with ANSI output; a terminal | yes / no |

How the crates are split, and why: [docs/CRATES.md](docs/CRATES.md).

What is built and what comes next: [the roadmap](docs/ROADMAP.md). A renderer crate never depends on the toolkit,
and a platform crate draws nothing itself; CI checks both.

## Development

`just check-all` runs the quality gate (fmt, clippy, tests, docs, unused dependencies, the layer rules, the Nushell
tests). `just fuzz-all` runs the fuzz targets (nightly). `just fonts-list` and `just fonts-fetch` manage the font
catalog. Scripts are Nushell, in `scripts/`, with their tests.

Part of the Dodai family: Hibari OS is a thin operating system for ESP32 boards, Fusuma draws the interface, and Dodai
UI is the design system.

## License

MIT. The bundled fonts are under the SIL Open Font License 1.1, Material Symbols under Apache 2.0; their texts are in
`crates/fusuma-fonts/fonts`.
