# Fusuma roadmap

Fusuma is a native Rust UI toolkit from microcontrollers to desktops and phones: no webview, no
HTML, no CSS. Status: `[x]` done, `[ ]` planned. Order matters: each phase leaves something that
works and is tested.

## Decisions

* **One dependency for apps.** Apps use `fusuma` (the toolkit, one crate, no separate core or
  widgets crate); the backend is chosen by target and, on a desktop, at run time (GPU first, CPU if
  there is no usable adapter). The renderer crates stay separate: each is useful alone, and a firmware
  that only draws must not pull in a toolkit.
* **Vello for everything with an operating system.** `vello_gpu` on a GPU, `vello_cpu` without
  one, sharing one implementation of the `Renderer` trait (`fusuma-render-vello-common`). On a chip, Vello
  whenever a frame fits (see the next two items); `tiny-skia` and the integer renderer are for chips where it does not.
* **Several renderers, one trait, a default by hardware.** The integer renderer (`fusuma-render-embedded`),
  `tiny-skia` (`fusuma-render-skia`), `vello_cpu` and the Vello GPU backend all pass the same conformance suite.
  A device uses the default that fits it and the others stay available: the GPU backend when there is a usable
  GPU, `vello_cpu` on a chip with PSRAM, `tiny-skia` or the integer renderer on a chip without. This replaces an
  earlier decision to use Vello only and to delete the integer renderer: the ESP32-C6 measurements
  (`tools/bench-c6` in `hibari-os`) showed `tiny-skia` 3 to 4 times faster and lighter than `vello_cpu` there, and
  the integer renderer another 3 to 4 times faster, with half the memory per pixel. On the ESP32-S3 `vello_cpu` is
  about 4 times slower than the integer renderer for a full frame but felt richer with partial redraws.
* **The automatic choice ranks by memory, not by speed, on purpose: the richest backend that fits.** Vello is the
  preferred look because it draws real typography (Inter and JetBrains Mono through Parley, shaped, with
  diacritics), which `tiny-skia` and the integer renderer cannot (they have only the ASCII bitmap fonts). So the rule
  is: the GPU, then `vello_cpu` when PSRAM holds a Vello frame twice over, then `tiny-skia` when its frame fits the
  RAM left after `min_free_kb`, then the integer renderer. The cost is speed: on the S3 the integer renderer is about
  4 times faster for a full frame. Where speed matters more, a board pins a backend in `fusuma.toml`; a
  `prefer = speed | quality` setting is not built.
* **Fonts beyond Latin are tiered, not bundled everywhere.** Bundled: Latin subsets only (213 KB for three fonts).
  Desktop and phone get other scripts (Greek, Cyrillic, CJK, emoji) from system fonts through Fontique with
  fallback: Fusuma's job is to wire that up, never to bundle CJK. A chip with PSRAM may get Greek and Cyrillic as an
  optional cargo feature (wider Inter and Mono subsets; the size is not measured) and an app can register its own
  font data for anything else. A chip without PSRAM stays Latin on bitmap fonts. Unknown, to check before promising
  it: whether `vello_cpu`'s glyph runs draw colour emoji (COLR or bitmap glyphs).
* **Right-to-left and complex scripts are later work, for desktop and phone only.** Parley shapes and reorders them,
  so a label probably renders right on Vello, but nothing tests it: the status is unknown, not done. The text field is
  known to be wrong for them (its caret and selection work on UTF-8 boundaries and measure byte prefixes, which breaks
  on RTL, combining marks and ligatures), and layout has no direction (rows run left to right, nothing mirrors). Needed:
  a caret and selection from Parley's cluster API, a layout direction with start and end instead of left and right,
  and tests with Arabic, Hebrew and Devanagari through system fonts. Chips stay left-to-right Latin.
* **Design systems plug in from outside, for pixels and for cells.** Fusuma ships a plain theme and a theme interface,
  and never imports a design system. Dodai maps onto both from `dodai-fusuma` in the `dodai-ui` repository; its text-UI
  side is a `tui` feature there (off by default, no fonts or renderer), not a separate crate, so one mapping keeps
  pixels and cells on the same brand. For a board the mapping runs at build time and the firmware carries only
  Fusuma's theme data (`dodai-core` is `f32` and `std`).
* **Native only.** No webview. A browser target is a later, optional platform crate that draws
  into a canvas from WebAssembly.
* **Animation is built in**, in fixed-point maths so that it behaves the same on a chip without an
  FPU as on a desktop.
* **Effects and shaders are optional capabilities.** A backend reports what it can do; widgets use
  an effect when it exists and fall back to a plain fill when it does not.
* **Licence: MIT.** Credit is kept by the licence notice; a trademark, not the licence, protects
  the names.
* **A renderer crate never depends on a toolkit crate**; CI enforces it (`scripts/ci/layers.nu`).
* **A text UI is a separate platform, not a renderer.** `fusuma-platform-tui` is our own (not ratatui) but it is
  not a `fusuma-render-cells` backend plugged into the `Renderer` trait. A terminal draws widgets in a completely
  different way: through pixel primitives a checkbox is a rounded box plus a tick path, a radio button two circles, a
  switch a pill and a circle, and a cell grid would have to guess that these are `[x]`, `(•)` and `[ ●]`. So the
  components, their state, events, focus, scrolling, dialogs and the frame loop stay shared, and only how each kind of
  widget looks (and how it is laid out, in cells) is the TUI's own. Nothing is built; see Phase 4 and Phase 5.

## Crate structure

| Crate | What | `no_std` |
|---|---|---|
| `fusuma-render` | `Renderer` trait, geometry, colors, paths, conformance suite; later the optional `Effects` and shader contract (feature-gated, unstable) | yes |
| `fusuma-render-skia` | `tiny-skia` backend, `no_std` with `alloc`; the automatic step between Vello and the integer renderer | partly |
| `fusuma-render-vello-common` | the `Renderer` on a Vello scene, generic over CPU and GPU (internal) | with `libm` |
| `fusuma-render-vello-cpu` | `vello_cpu` backend | with `libm` |
| `fusuma-render-vello` | `vello_gpu` backend | no |
| `fusuma-anim` | easing, tweens, keyframes, springs, fling decay | yes |
| `fusuma-shaders` | shader contract implementation and engine (WGSL, prebuilt rust-gpu kernels, CPU path) | no |
| `fusuma-text`, `fusuma-fonts` | Parley layout over bundled fonts (Inter, Inter Bold, JetBrains Mono); `no_std` with alloc, runs on a chip with PSRAM | partly |
| `fusuma` | **the facade, the one crate an app adds**: re-exports the toolkit and, by feature, `render`, `anim`, `platform`, `text` and the backends | with `default-features = false` |
| `fusuma-ui` | **the toolkit**: widget tree, layout, theme, events, focus, scrolling, dialogs, text input, animation and paint passes, `Shell` and `App`; `no_std`, no allocator | yes |
| `fusuma-cells` | **planned**: a cell grid, a diff of two frames into changed runs, an ANSI encoder to any byte sink and an ANSI key and mouse decoder; for every text-UI output | yes, no allocator |
| `fusuma-platform-embedded`, `fusuma-platform-esp` | shared board code (`Board`, `Driver`, the renderer surface) and the ESP-IDF P4 board; the S3 watch's board is in the Hibari repository for now | yes / no |
| `fusuma-platform-desktop` | `winit` window, pixel buffer or GPU surface | no |
| `fusuma-platform-android`, `-ios` | native windows and touch | no |
| `fusuma-platform-tui` | **planned, our own, a separate platform** (not a renderer): a terminal or an SSH session through `crossterm`; the serial console of a chip and a pixel panel use `fusuma-cells` directly | no |
| `fusuma-platform-web` | later, optional: WebAssembly into a canvas | no |

## Phase 1: renderer
- [x] `fusuma-render`: `Renderer` trait, conformance suite, fuzzed
- [x] `fusuma-render-embedded`: integer backend, runs on the ESP32-S3 (kept: the lightest backend, see Decisions)
- [x] `fusuma-render-vello-cpu` on `vello_cpu`, passing the conformance suite and matching the integer backend pixel for pixel; fuzzed
- [x] `fusuma-render-vello-common`: one implementation for both Vello backends
- [x] Published 0.1.0 of `fusuma-render`, `-embedded` (and the dropped `-skia`)
- [x] `fusuma-render-vello-common` and `-vello-cpu` published (0.2.0); `hibari-os` uses them
- [x] `fusuma-render-vello` on `vello_gpu` and `wgpu` 30: headless device, renders into an `Rgba8Unorm` texture and reads it back; passes the conformance suite on a real GPU (AMD RX 7800 XT, Vulkan) and matches the CPU backend; tests skip on machines without a GPU. Images go to the GPU as external textures through a `Scene::image_source` hook
- [ ] `fusuma-render-vello`: render into a host-owned texture or window surface with no read-back; `gpu_available()` is there for the toolkit's GPU-first choice
- [x] Benchmark of a UI frame (`cargo run --release -p fusuma-render-vello --example bench`, read-back included): at 410x502 `vello_cpu` 0.18 ms and `vello_gpu` 0.61 ms; at 1920x1080 1.75 ms and 3.56 ms. A simple scene: the CPU wins while the picture has to come back to the CPU
- [ ] Benchmark more: a complex scene (many paths, large blurs), the GPU drawing to a surface with no read-back, and a shader chain with read-back versus as a texture
- [x] Experiment: `vello_cpu` on the S3 measured, see Decisions and Phase 1b
- [x] Text: Parley layout (`fusuma-text`) and Vello glyph runs draw Inter, Inter Bold and JetBrains Mono (`fusuma-fonts`, Latin subsets, 213 KB for the three) on the CPU and GPU backends, measured and drawn at one size; the bitmap fonts remain the fallback and the integer and `tiny-skia` renderers' only font. `Font` is part of `TextStyle`, `Theme::fonts` maps roles to fonts. Not yet: wrapping and multi-line text in widgets, rich text spans, system fonts, Greek, Cyrillic, CJK or emoji, caret placement in a text field from Parley
- [x] Measured on the ESP32-P4: with fonts the first frame needs about 30 KB of internal RAM when allocations above 512 bytes go to PSRAM (`CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=512`), against about 240 KB at the default threshold; the first frame takes about 300 ms. Parley is not tried on the S3 or the C6 (no room on the C6)

## Phase 1b: Vello on the ESP

- [x] `vello_cpu` builds for the S3 (Xtensa) and the C6 (RISC-V) with `libm`, no `std`
- [x] Benchmark and a touch demo on the S3 (`tools/bench-s3`, `tools/vello-demo-s3` in `hibari-os`)
- [x] **Tearing** was not a missing tearing-effect sync: the CO5300 needs an explicit memory-write command (0x2C) after each address window, and without it a partial update lands in the wrong place. Fixed in `hibari-drivers` with a regression test; update windows are also kept even-aligned
- [x] **`no_std` backend:** `fusuma-render-vello-common` and `fusuma-render-vello-cpu` are `no_std` with `alloc` behind a default `std` feature (`libm` needed without it; the PNG output stays `std`). The whole `Renderer` on `vello_cpu` builds for the S3 and the C6 and runs on the S3: 2 to 3 percent over raw `vello_cpu` (267 against 260 ms for a full frame). Not yet released
- [x] **Partial redraws in the API:** `Tree::paint_frame` draws only what changed and `Scene::set_damage`/`add_damage` make `vello_cpu` rasterise only that (several areas per frame, bands through one reused scratch buffer of at most 128 KB, pixel-identical to a full render inside the area; tested with shapes, text, clips and alpha). Measured on the S3 watch with fonts: a hover 127 to 50 ms, a page change 450 to 1100 ms down to about 265 ms. Limits: damage is one bounding rectangle per frame outside a scroll, and `pixels()` is only valid inside the hinted area, so a screenshot or PNG export must not use the hint
- [x] **Memory budget:** a `vello_cpu` surface is modelled at about 4.8 bytes a pixel (`fusuma-platform`, measured on the S3); a damaged region is rendered in bands through one reused scratch buffer of at most 128 KB, so a page change no longer allocates a second picture-sized buffer. The chooser refuses a board with no room for a frame. The extra memory is not yet counted in that estimate
- [ ] **Multithreading:** `vello_cpu` 0.4 has a `multithreading` feature (rayon, `std` only); Fusuma does not turn it on, though `fusuma-render-vello-cpu`'s description said "optionally multithreaded". For the desktop and perhaps the P4 (two cores, ESP-IDF has threads; rayon there is untried), not the S3 (`no_std`). Unknown: whether the partial `render_with` path, which partial redraws use, runs on the threads
- [ ] **Speed:** found so far on the S3 watch: Vello rasterised the whole surface for a 47x56 hover (fixed: 127 to 50 ms), and the RGB565 conversion is slow (252 ms of a 750 ms full frame, my own per-row code, not yet fixed). Still unprofiled: path flattening, text layout and glyph drawing, and layout, which together are most of a big update
- [x] **Text:** Parley builds with `libm` for the S3 (Xtensa) and the P4 and runs on both with `vello_cpu`'s glyph runs; the S3 firmware image with Vello, Parley and the three fonts is 2.9 MB. Shaping works (Romanian ă â î ș ț render); wrapping and bidi are not used by any widget yet
- [x] **P4:** the showcase runs on an ESP32-P4 7" with `vello_cpu` chosen automatically (first full frame about 210 ms without fonts, 300 ms with, a small update about 10 ms of drawing); the S3 benchmark crate itself was not run there
- [x] **A fair A/B on the board** (S3 watch, 410x502 first frame): integer 196 ms, Vello 523 ms (948 ms with Inter), `tiny-skia` 1883 ms. Not done: the integer renderer with anti-aliasing added

### The integer renderer is kept (the removal below is cancelled)

- [x] The bitmap fonts (`font_for`, `text_size`) live in `fusuma-render-vello-common`; `fusuma-render-embedded` keeps its own copy until it is deleted
- [x] Dirty-rectangle redraws in `fusuma`: every change records the area it touches (`take_damage`, `take_damage_in(screen)`), layout records the old and the new place of anything that moved, and `paint_region` draws only what overlaps. A random-change property test proves the damage covers every pixel that changed and that redrawing only the damage equals a full redraw; fuzzed (329k runs)
- [ ] Use it for the launcher: needs a tile widget (an icon, a label, a press state), a pager with slide transitions and a `compose` pass for offsets, then rebuild the Hibari launcher on them; the launcher still has its own view and the integer renderer
- [x] A region renderer that reuses its surface and a flush that sends just the region to the panel (the `Board::present(region, pixels)` contract of `fusuma-platform-embedded`)
- [x] Move the `fusuma` toolkit tests from `Embedded` to `vello_cpu` (done); the launcher and host tests in `hibari-os` still use `Embedded`
- [x] ~~Delete `fusuma-render-embedded`~~ cancelled: it stays, see Decisions
- [x] A choice of default backend by hardware: `fusuma-platform` picks by hardware facts (GPU, then PSRAM, then memory) with layered overrides (`[render]` in `fusuma.toml`, a cargo feature) and a memory check; the P4 and desktop use it. **The rule ranks by memory, not speed**: on the S3 it picks Vello although the integer renderer is about 4x faster for a full frame. A speed-aware `prefer = smooth/quality` setting is not built
- [x] Measured `tiny-skia` on the ESP32-S3: 1883 ms for a full 410x502 frame, the slowest of the three
- [x] **`tiny-skia`'s clip memory:** every `clip` made a surface-sized mask (1 byte a pixel) and every `save` copied it, so a UI frame (a clip per redrawn part, per scroll window and per label) held about three: some 7 bytes a pixel, not the 4 the chooser counted, which would have run a C6 at 172x320 out of its 320 KB heap (the C6 benchmark had no clips, so it did not show). Now a clip is kept as a rectangle, one mask is made only when a shape crosses a clip and is rewritten row by row when the clip changes, and rectangles and text never use it: 5 bytes a pixel, which `Backend::memory` now counts. Tested (no mask for shapes inside their clips, one reused mask for crossing ones, nested clips and restores), each test seen to fail against a broken copy. The new figure is worked out, not measured on a board. With it the C6's own 172x320 LCD gets the integer renderer and a 160x160 one gets `tiny-skia`
- [ ] A conformance run of the renderers on the C6
- [ ] Publish the 0.3.0 set (`fusuma`, `fusuma-ui`, `fusuma-platform`, `fusuma-fonts`, `fusuma-text`, the renderers; `fusuma-render-skia` on crates.io is the old `std` 0.1.0). Not done: it unblocks Hibari's and `dodai-fusuma`'s CI, which depend on Fusuma by a sibling path

### Scrolling, screen shape and responsive layout (done, measured on the S3 watch)

- [x] **Pixel-shift scrolling:** a container that only scrolled has its pixels moved (`Renderer::shift_area`, `will_draw`) and only the strip that came into view, the scrollbar column and the rounded-corner bands are drawn; the panel is still sent the whole window. Any other change inside the window cancels the shift, so the picture is never wrong, only sometimes slower. 14 tests compare every scrolled frame with a full redraw pixel by pixel (wheel, drag, fling, rubber band, padding, rounded corners); fuzzed. **The frame time on the watch is not measured yet** (a redraw of the list window cost about 180 ms a frame). Only `vello_cpu` implements it; the integer and GPU backends fall back to a full redraw
- [x] **Screen shape:** `Shape` (`Rect`, `Rounded(radius)`, `Circle`), `Insets` and `safe_insets` (0.293 x the radius on every side), `edge_at(y)`, `Board::shape`, `Input::Shape`, `App::shape`. The showcase lays out inside the corners (checked for radii 40 to 90 and a circle). The watch's 70 px radius is a guess made by eye
- [x] **Size classes in the showcase:** watch (tile grid, one small page with a back button), phone (Previous and Next row), tablet and desktop (sidebar), by logical width, with the display scale applied; widgets are the same everywhere, only the layout differs
- [x] **Small fixes found on the watch:** a slider drag no longer turns into a page scroll when the finger drifts; the slider marks only the stretch the thumb crossed; labels in a column keep their text width
- [ ] Move the size classes (`Class`) and the shape handling out of the showcase into the toolkit, so every app gets them
- [ ] Faster RGB565 conversion (252 ms of a 750 ms full frame on the watch is my own per-row, per-pixel code), a 240 MHz clock, hot buffers in internal RAM, the second core; **profile layout, text and rasterising first**: the larger part of a big update is still unprofiled
- [ ] Several damage rectangles outside scrolling, so a hover plus a status line does not repaint everything between them
- [ ] Virtualised lists (a list of hundreds of rows with a fixed node capacity)

## Phase 2: animation (out of the box)
- [x] `fusuma-anim`: `Fx` (Q16.16), easing (quad, cubic, quart, smooth, back, bounce, CSS Bezier), `Tween` with delay, stagger and retarget, `Keyframes`
- [x] Physics: `Spring` (stiffness, damping, mass, or response time and damping ratio, presets), `Decay` (fling with friction and projected end), interruptible without a jump
- [x] 60 unit tests and a fuzz target; builds for RISC-V, ARM and Xtensa
- [x] `Animator<N>`: a fixed set of running tweens, springs and flings on one frame clock; `step` says whether another frame is needed; stale-safe handles
- [x] Drag helpers: `rubber_band` (resisting overscroll) and `snap_page` (where a flick settles, at most one page)
- [x] Scroll physics in the toolkit (`scrollphys`): a drag past an end resists like a rubber band and never stretches past the window, a flick goes on with friction (`Decay`) and slows to a stop, hitting an end at speed stretches a little and springs back with `SpringConfig::SPATIAL`, touching a moving list stops it and the lift is not a click, the wheel, `scroll_to` and the keyboard take over from a fling, slow frames run in 100 ms steps so a watch moves in real time. Tested, with mutation checks. Not yet: a pager that follows the finger, horizontal scrolling
- [x] Animation vocabulary in `fusuma-anim`: six spring tokens (spatial and effects, fast, default, slow; modelled on Material 3's motion scheme from memory) and a composable, allocation-free `Clip` (`then`, `delay`, `repeat`, `forever`, `ping_pong`, `reverse`, `speed`, `stagger`; presets `fade`, `slide`, `pop`, `shake`). Kept small on purpose: no closures, at most 8 pieces
- [ ] Wire the vocabulary into widgets: `animate`, `enter` and `exit` for node offset and opacity (opacity is approximate on the integer renderer), page transitions (`Push`, fade-through), a dialog entrance, an animation budget per chip so a watch can skip them
- [ ] Shared and layout animations: a card that grows into a page, list reorder and insert, stagger helpers
- [ ] More physics: gravity and bounce, a spring chain for trailing motion, velocity from touch history
- [ ] Vector animation like Rive or Lottie: a state machine over timelines, playing `velato` (Lottie) and `vello_svg` content; a design-tool file format is out of scope
- [x] Hibari OS uses `fusuma-anim` directly (its own animation module is gone)

## Phase 3: effects and shaders
- [ ] `Capabilities`: `effects` and `shaders` flags next to `alpha` and `antialiasing`
- [ ] `Effects` extension trait (feature-gated, unstable): gradients (linear, radial, sweep), drop shadow, blur, blurred rounded rectangle, opacity and blend layers, masks. First on `vello_cpu` (it has filter layers and blurs), then `vello_gpu`; the integer backend does not implement it and widgets fall back to a plain fill
- [ ] Shader contract: `ShaderSource` (WGSL text or a named prebuilt kernel), uniforms as bytes, a `draw_shader` call on GPU backends, one conformance shader
- [ ] `fusuma-shaders`: the engine of `dodai-shaders` (GPU compute kernels from rust-gpu as embedded WGSL, plus the CPU path from the same Rust source), render to a texture on the host's `wgpu` device so there is no read-back; raw WGSL for custom shaders. Decide with `dodai-ui` where the line between engine and looks is
- [ ] Shader widgets: a `ShaderView`, backdrop blur, animated backgrounds, transitions
- [ ] MSRV: `dodai-shaders` needs Rust 1.97 and Fusuma is at 1.90; keep shaders behind a feature or raise the MSRV
- [ ] Optimization pass with numbers: no read-back, skip invisible shaders, render small and scale, one frame clock, pipeline warm-up

## Theming: the Dodai look must be easy to apply

Fusuma ships a plain default theme (light and dark) and a theme interface that every widget reads;
design systems plug in from outside, as with iced's per-widget styles. Fusuma never imports
`dodai-ui` (the design-system repository); `dodai-fusuma` maps the Dodai themes onto the interface.

What `dodai-core` looks like (read before designing the interface):
* A `Theme` is a palette of semantic colour roles (surfaces, borders, text, brand, status tones) plus
  typography, spacing, shape (corner radii, a shape system), effects, motion, density and a panel
  border. It is `f32` and `std`.
* Components resolve to token structs through `Theme` methods (`ButtonTokens`, `KitStyle`, ...), and
  a `ComponentSpec` (derived with `TokenSpec`) describes them; `dodai-gen` writes the per-toolkit
  glue from that spec. A Fusuma target for the generator is the way to keep the adapters in step.
* `dodai-core` already has a motion module: easing, springs, decay, rubber band, page snap,
  tweens, an animator, entry effects and per-personality reactions, all `f32`.

Plan:
- [ ] Read `dodai-core`'s token model and the `dodai-slint` and `dodai-iced` adapters; shape the Fusuma theme interface so the Dodai themes map onto it without loss (per-widget tokens and states, shape system, motion, effects)
- [x] `fusuma`: a `Theme` (a struct, not a trait) with a neutral default light and dark theme (colours, spacing, radii, text sizes, fonts, control and indicator sizes, motion timings); every widget reads only from it
- [ ] `dodai-fusuma` in the `dodai-ui` repository: a first adapter exists on a local branch, **unpushed** (colours, spacing, radii, text sizes and sizes from `dodai-core::Theme`, translucent surfaces flattened for renderers without alpha; fonts left at Fusuma's default; seven tests). Not done: per-widget tokens, a Fusuma target for `dodai-gen`, Dodai-only widgets. Another agent is working on a dodai-ui facade, so it waits for that. Its Fusuma font features stay off there: Parley 0.13 conflicts with the Parley 0.11 in `dodai-slint`
- [x] Theme switching at run time (the showcase's theme button repaints everything) and density scaling (`Theme::scaled(percent)`, with the bitmap font magnified in whole steps)
- [ ] **Animation overlap:** `fusuma-anim` (fixed point, `no_std`, runs without an FPU) and `dodai-core::motion` (`f32`, `std`, richer: reactions, glide, mixins) do the same job. Keep both for now, because Hibari on a microcontroller cannot take the `f32`/`std` one; map the theme's motion tokens (duration, easing, spring response) onto `fusuma-anim`; consider moving shared curves and presets to one source later, and do not add features to both

## Text

Two tiers, because a microcontroller and a phone need different things.

* **Microcontrollers keep the built-in bitmap fonts.** They need no allocator, no font files and no
  Unicode tables. Today these are three monospace sizes, and **ASCII only** (`embedded-graphics`' `ascii` sets): on
  the integer and `tiny-skia` renderers `é` or `ș` draw as the replacement glyph, so "Romanian works" holds only on
  Vello with fonts. Latin-1 sets (`iso_8859_1`) would add `é` and `ü`; `ș` and `ț` need `iso_8859_2` or a font of our
  own; a proportional pixel font and a glyph atlas
  for a larger character set are the way to grow them.
* **Desktop and phone use Parley** (the Linebender text stack, which `/home/sorin/Projects/linebender/parley`
  is a checkout of). Parley is layout: it needs `alloc`, and its dependencies are Fontique (font lookup and
  fallback), HarfRust (shaping), Skrifa (glyph outlines) and ICU4X (bidi, line and word breaking,
  normalisation). It ships `std`, `libm` and `system` features, and published 0.13 needs Rust 1.88 and is
  Apache-2.0 or MIT, so it fits our 1.90 and our licence.
* **Drawing the glyphs.** Parley gives positioned glyph runs; the draw side is `vello_cpu`'s and
  `vello_gpu`'s glyph runs (the `text` feature, built on `glifo`), so text and shapes go through the same
  engine. Fusuma's `Renderer` trait has no glyph call today: it needs a small, optional `draw_glyphs`
  that the Vello backends implement and the integer backend does not (it keeps its bitmap fonts).

Costs, measured: the S3 watch firmware with Vello, Parley and the three bundled fonts (213 KB of the total) is a 2.9 MB image, so Parley is affordable on a chip with PSRAM, though not on one without (the C6 has no room for it). ICU4X's data is large (the `icu_segmenter` source alone is 23 MB) and was **not** the problem it was feared to be with the `libm` feature. The `complex-scripts`
feature (dictionary breaking for CJK, Thai and others) adds more. It builds and runs on the S3 and the P4 with `libm` (not `std`: that pulls `memmap2`, which has no ESP-IDF support). On the P4 it needs `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=512` and a 128 KB app stack, or it fills internal RAM (see the `esp-boards-rust` skill).

- [x] A text seam: `TextStyle` carries a `Font` (`Sans`, `SansBold`, `Mono`, `Bitmap`); a backend with vector fonts lays out and draws with them, any other (or a font that is not bundled) draws its bitmap font, so asking is always safe. Measuring and drawing use the same size, which the conformance suite caught
- [x] `fusuma-text` (Parley layout, one line, cached; `no_std` with alloc) and `fusuma-fonts` (Inter, Inter Bold, JetBrains Mono, Latin subsets under the OFL, one cargo feature each). Not done: system fonts through Fontique (the desktop does not use them yet), and no Greek, Cyrillic, CJK or emoji
- [x] Glyph drawing: not a `Renderer` call but `Scene::draw_glyphs` in `fusuma-render-vello-common`, implemented by the CPU and GPU scenes through `glyph_run`; tests check the GPU picture against the CPU one and that nothing is drawn outside the measured box. The integer and `tiny-skia` backends keep the bitmap font
- [ ] Text widgets: a single-line text field exists (own cursor and selection code, not Parley's editing utilities; up to 64 bytes; IME and clipboard through the desktop platform; no placeholder, no drag selection, no undo). Not done: wrapped and multi-line labels (Parley can wrap, no widget asks it to) and a multi-line text view
- [ ] Measure: Parley's own code size (only the whole image is known: 2.9 MB on the S3), and whether the lightweight segmenter is enough for Latin text
- [ ] Per-widget style: font family, size, weight and line height as theme tokens. Done: `Theme::fonts` maps text roles (body, title, mono) to fonts and `TextRole::Code` uses mono; sizes are theme tokens. Not done: weight, line height and per-widget overrides

## Phase 4: the toolkit

Design, from reading Masonry (Xilem's widget layer; ideas only, our own names and API): a **retained
tree** that keeps widget state, **passes** over it (events, update, animation, paint), **invalidation
flags** per node that travel to the root, **pointer capture** and **bubbling**, **actions** queued out
of the tree, and an optional view layer on top (as in Xilem) that rebuilds into the tree. For the
ESP the tree is a fixed arena with the widgets as a closed enum, text stored inline and no heap;
desktop-sized trees use the same code with a bigger `N`.

- [x] `fusuma` first slice: `Tree<N>` (generation-checked handles, remove, reuse), layout (`Content`, `Fixed`, `Fill(weight)`, padding, gap, cross alignment; measured through the renderer), theme tokens with default dark and light (contrast-tested), event pass (capture, hover, click actions, cancel on leave or disable), update pass (disabled flows down, layout on request, hover refreshed after layout), animation pass (button hover fade, only widgets that asked), paint pass. 53 tests, a fuzz target (646k runs), builds for RISC-V, ARM and Xtensa; a tree of 32 widgets is about 4 KB
- [x] Shown on the ESP32-S3 board: the showcase runs on the watch with the integer renderer (first frame 196 ms, against 61 ms for the old launcher's own code, which is not explained: layout and damage handling are the suspects, unmeasured), in `tools/fusuma-showcase-s3` of the Hibari repository
- [ ] Layout only the changed subtrees (paint is done by region, and a scroll avoids redrawing the window; layout still redoes the whole tree when anything asks, which is unmeasured but a suspect for the 196 ms frame on the watch)
- [ ] A compose pass: offsets for scrolling without a layout (a scroll still relayouts the whole tree; pixel-shift scrolling removed the redraw cost, not this)
- [x] Focus and keyboard (Tab, arrows, Enter, Escape), text input with an input method
- [ ] Accessibility with `AccessKit` (not started)
- [x] Widgets done: checkbox, radio, switch, slider, progress, selectable list, scroll container with fling, rubber band and bounce, text field, dialog (the modal overlay and z-order). See the lines below for what is still missing
- [ ] A `Widget` trait for custom widgets behind an `alloc` feature (the built-in set stays a closed enum for the no-allocator case)
- [ ] A view layer: a lightweight description built from application state and diffed into the retained tree, with messages routed back
- [ ] Per-widget style hooks so that a design system can restyle a widget beyond the theme tokens
- [ ] A semantic tree so that other front ends (terminal, accessibility) can read the widget tree: a read-only view of each node's kind, text, state, rectangle, focus and children. **The text-UI platform needs it** (and AccessKit would use it); layout may move to `taffy` on the `std` feature, while the no-allocator layout stays
- [x] Widgets in `fusuma-ui`: checkbox, radio, switch, slider, selectable list item, progress bar, with keyboard focus (Tab, arrows, Enter)
- [x] Scroll container (`Tree::scroll`): clips its children, scrolls by touch drag (with a slop so a tap still clicks), mouse wheel (`Pointer::Wheel`) and by moving the focus to a hidden widget; hidden widgets cannot be hit; a scrollbar shows the position. Inertia, rubber band and bounce came later (`scrollphys`, Phase 2). Not yet: horizontal scrolling
- [x] Text input (`Tree::text_input`): a single line of up to 64 bytes with cursor, selection, typing, edit commands, input-method composition and clipboard hooks; the desktop platform wires keys, IME and the system clipboard. Not yet: mouse-drag and word selection, caret blink, undo, longer text, a multi-line field
- [x] Animation vocabulary in `fusuma-anim`: six spring tokens (spatial and effects, fast, default, slow) and a composable, allocation-free `Clip` with presets (fade, slide, pop, shake). Not yet wired into the toolkit: scroll fling and rubber band, animated node offset and opacity (`animate`, `enter`, `exit`), page transitions (`Push`, fade-through), and an animation budget per chip
- [x] Dialog (`Tree::dialog`, `open_dialog`, `close_dialog`): centred at its content size, drawn on top, a dimming layer on renderers that blend (none on the integer one, which would black the screen out), modal (nothing behind answers, scrolls or takes the focus), focus moves in and comes back, a press outside or Cancel (Escape) sends `Dismissed` and the app decides. Nested dialogs work. Not yet: an entrance animation, a drop shadow, a dialog that is part of a scrolling page
- [ ] More widgets: multi-line text view, tabs, menu, toast
- [x] The platform crates: `fusuma-platform-embedded` (shared by every board), `fusuma-platform-esp` (the P4 on ESP-IDF) and `fusuma-platform-desktop` (winit); the watch's board is in the Hibari repository for now
- [x] `fusuma` is a facade crate (`fusuma-ui` is the toolkit): `renderer-*` features link backends, `toml` reads settings, `fusuma-platform` chooses
- [x] `fusuma::Shell`, `Input`, `TouchTracker`, `Clock`, `wheel_pixels` and `App::resized` in `fusuma-ui`: the frame step and input handling every platform shares; a CI rule keeps widgets and renderers out of `fusuma-platform-*`
- [x] `fusuma-platform-desktop`: a winit window, mouse, touch, keyboard, wheel and resize into the shared `Shell`; GPU Vello when an adapter exists, CPU otherwise, with the same settings layers; the showcase runs in it (`just showcase-desktop`). Theme scaling (`Theme::scaled`) and an idle loop that sleeps (312 wake-ups in 5 s became 0) are done. Not yet: presenting straight from the GPU, real fonts, macOS and Windows run by hand
- [x] `fusuma-platform-embedded`: the part every board shares (a `Board` trait: size, touch sample, show a region as RGB565 rows; the renderer surface; the frame step), tested on a computer with a fake board; builds `no_std` for RISC-V, Cortex-M0+ (no Vello: no atomics), Cortex-M4F and bare-metal ARM64 (`scripts/ci/embedded_matrix.nu`); the P4 crate is now just a `Board` on top of it
- [x] The ESP32-S3 watch (Waveshare ESP32-S3-Touch-AMOLED-2.06) runs the showcase on `esp-hal` through `fusuma-platform-embedded` (`tools/fusuma-showcase-s3` in the Hibari repository); the showcase has a compact layout for small screens. Measured first frame at 410x502: integer 196 ms, Vello 523 ms (948 ms with fonts), `tiny-skia` 1883 ms; touch not tried by me
- [ ] Boards beyond the P4 and the S3 watch: the C6 (the `Board` for their panels; `esp-hal` without ESP-IDF as well as ESP-IDF), a Cortex-M one (RP2040, STM32, nRF), a Linux framebuffer/DRM one for Raspberry Pi-class kiosks with no window system
- [ ] Mobile (`fusuma-platform-mobile`), Apple Silicon and Windows run by hand (CI builds them)
- [ ] Platform picked by target automatically (`fusuma-platform-desktop`, `-mobile`; the ESP one is a separate firmware-side crate)
- [ ] A gesture recogniser (tap, long press, swipe, drag with `fusuma-anim`): tap, drag and fling exist inside the widgets (the scroll container, the slider); long press and swipe do not, and Hibari OS's recogniser in `hibari-hal` has not moved here
- [ ] Port the Hibari OS launcher to Fusuma widgets, and `dodai-fusuma` (Dodai widgets and a `ShaderView`) in the `dodai-ui` repository

## Phase 5: more platforms
- [ ] `fusuma-platform-android` and `-ios`: native windows and touch, no web wrapper
- [ ] `fusuma-platform-tui`, our own, **a separate platform and not a renderer** (decided; nothing built). It draws each widget kind its own way (`[x] label`, `(•) label`, `[███░░]`, `[ OK ]`) in a grid of text cells, in a terminal, over SSH, on a serial console of a headless chip, or on a character display. Shared with the pixel platforms: the tree, state, events, focus, scrolling, dialogs, the animation clock, `Shell` and `Input`. Its own: layout in cells, the cell look of each widget, and the terminal input (mouse, keys, resize) and output (a diffed write of only the changed cells). What it needs first, none of it started:
  - [ ] a read-only semantic view of the tree (the item above)
  - [ ] layout that does not need a `Renderer`: `Tree::update` takes one only to measure text, so a small `Measure` trait that a renderer satisfies and a cell grid (one column per character) can too
  - [ ] a cell theme interface with a plain light and dark default: the same colour roles as the pixel `Theme`, how each degrades by colour depth (truecolor, 256, 16, monochrome with bold, reverse and underline), glyph sets (ASCII, light, rounded, heavy borders; checkbox, radio, switch and progress marks; scrollbar; spinner frames), an ASCII-only tier for serial consoles and the bitmap font, spacing in whole cells, emphasis per state, the focus marker, and motion in cell steps with an off switch for slow links. Pixel looks without a cell form get a stand-in (a radius becomes rounded border glyphs, a scrim dims the cells behind). Loadable as data (TOML), so a brand or a terminal can switch it at run time
  - [ ] the cell look of every widget kind. **Decided: option A**, in `fusuma-ui` as a second paint pass into a `CellSink` trait that the output implements: no allocation in the toolkit, the platform crates stay thin, and the layers rule holds as it is (option B, a crate reading the semantic view, would need a carve-out from "a platform draws nothing")
  - [ ] `fusuma-cells` (`no_std`, no allocator): the cell grid, the diff into changed runs, the ANSI encoder and decoder
  - [ ] text snapshot tests: a cell frame is text plus attributes, so every widget in every cell theme is a golden text file (cheaper and clearer than PNGs), and the snapshots make a gallery
  - [ ] outputs, in this order: (1) `crossterm` terminal and SSH, with colour depth detection, quickest to iterate and testable in CI; (2) the serial console of any chip (C6, RP2040, Cortex-M0), ANSI over a UART through `fusuma-cells`, a few KB of RAM; (3) a pixel panel: the cells drawn with the bitmap font through the integer renderer, only the changed cells, sent as one `Board::present` region
  - [ ] what is to make it better than `ratatui`, to be shown with numbers: retained (focus, scrolling, dialogs and animation built in, which `ratatui` leaves to the app), one app for pixels and cells, `no_std` with no allocator, a diffed output and one region transfer on a panel
  
  Where the effort should go on a board: `ratatui` with `mousefood` is the current answer for a text UI on a board (and `dodai-ratatui` styles it, but does not build `no_std` yet). From mousefood's source, not measured: without its `framebuffer` feature every cell is drawn straight to the display (one small window and transfer per cell, slow on a QSPI panel); with it, `flush` sends the whole screen, so a one-letter change sends every pixel; `ratatui` itself already diffs, so changed cells are all it draws. Our hypothesis is that one dirty-region transfer (our `Board::present`) beats both, by an unknown number of milliseconds on a 35x55 cell grid. **Measure a ratatui and mousefood screen on the watch first**; a terminal, SSH or serial console needs no such work
- [ ] `fusuma-platform-web` (optional): WebAssembly into a canvas, no DOM widgets

## Out of scope
- A webview, HTML or CSS
- The C++ Skia
- A design-tool file format such as Rive's (playing Lottie and SVG is in scope)
