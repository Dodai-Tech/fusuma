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
  one, sharing one implementation of the `Renderer` trait (`fusuma-render-vello-common`). The
  `tiny-skia` backend was dropped; `fusuma-render-skia` 0.1.0 stays on crates.io, unmaintained.
* **Microcontrollers keep the integer renderer** (`fusuma-render-embedded`). `vello_cpu` builds for
  the ESP32-S3 (Xtensa) and the C6 (RISC-V) with `libm` and no `std`; whether it runs well there is
  an experiment (see below), not a promise.
* **Native only.** No webview. A browser target is a later, optional platform crate that draws
  into a canvas from WebAssembly.
* **Animation is built in**, in fixed-point maths so that it behaves the same on a chip without an
  FPU as on a desktop.
* **Effects and shaders are optional capabilities.** A backend reports what it can do; widgets use
  an effect when it exists and fall back to a plain fill when it does not.
* **Licence: MIT.** Credit is kept by the licence notice; a trademark, not the licence, protects
  the names.
* **A renderer crate never depends on a toolkit crate**; CI enforces it (`scripts/ci/layers.nu`).

## Crate structure

| Crate | What | `no_std` |
|---|---|---|
| `fusuma-render` | `Renderer` trait, geometry, colors, paths, conformance suite; later the optional `Effects` and shader contract (feature-gated, unstable) | yes |
| `fusuma-render-embedded` | integer drawing for small displays | yes |
| `fusuma-render-vello-common` | the `Renderer` on a Vello scene, generic over CPU and GPU (internal) | no |
| `fusuma-render-vello-cpu` | `vello_cpu` backend | no |
| `fusuma-render-vello` | `vello_gpu` backend | no |
| `fusuma-anim` | easing, tweens, keyframes, springs, fling decay | yes |
| `fusuma-shaders` | shader contract implementation and engine (WGSL, prebuilt rust-gpu kernels, CPU path) | no |
| `fusuma-text` | text on Parley: fonts, shaping, line breaking, bidi, selection; optional, desktop and phone only | no |
| `fusuma` | **the toolkit**: widget tree, layout, theme, events, animation, paint, and the widgets as modules; `no_std`, no allocator by default; a `std` feature adds the desktop extras (run-time GPU or CPU choice). Apps depend on this one crate | yes |
| `fusuma-platform-mcu` | microcontroller displays and touch | yes |
| `fusuma-platform-desktop` | `winit` window, pixel buffer or GPU surface | no |
| `fusuma-platform-android`, `-ios` | native windows and touch | no |
| `fusuma-platform-tui` | semantic rendering of the widget tree as text cells (ratatui) | no |
| `fusuma-platform-web` | later, optional: WebAssembly into a canvas | no |

## Phase 1: renderer
- [x] `fusuma-render`: `Renderer` trait, conformance suite, fuzzed
- [x] `fusuma-render-embedded`: integer backend, runs on the ESP32-S3
- [x] `fusuma-render-vello-cpu` on `vello_cpu`, passing the conformance suite and matching the integer backend pixel for pixel; fuzzed
- [x] `fusuma-render-vello-common`: one implementation for both Vello backends
- [x] Published 0.1.0 of `fusuma-render`, `-embedded` (and the dropped `-skia`)
- [ ] Publish `fusuma-render-vello-common` and `-vello-cpu`, move `hibari-os` to them
- [x] `fusuma-render-vello` on `vello_gpu` and `wgpu` 30: headless device, renders into an `Rgba8Unorm` texture and reads it back; passes the conformance suite on a real GPU (AMD RX 7800 XT, Vulkan) and matches the CPU backend; tests skip on machines without a GPU. Images go to the GPU as external textures through a `Scene::image_source` hook
- [ ] `fusuma-render-vello`: render into a host-owned texture or window surface with no read-back; `gpu_available()` is there for the toolkit's GPU-first choice
- [x] Benchmark of a UI frame (`cargo run --release -p fusuma-render-vello --example bench`, read-back included): at 410x502 `vello_cpu` 0.18 ms and `vello_gpu` 0.61 ms; at 1920x1080 1.75 ms and 3.56 ms. A simple scene: the CPU wins while the picture has to come back to the CPU
- [ ] Benchmark more: a complex scene (many paths, large blurs), the GPU drawing to a surface with no read-back, and a shader chain with read-back versus as a texture
- [ ] Experiment: run `vello_cpu` on the S3 (it builds with `libm`) and measure frame time and memory
- [ ] Text: Vello glyph runs with a Rust text stack replace the bitmap fonts on the CPU and GPU backends

## Phase 2: animation (out of the box)
- [x] `fusuma-anim`: `Fx` (Q16.16), easing (quad, cubic, quart, smooth, back, bounce, CSS Bezier), `Tween` with delay, stagger and retarget, `Keyframes`
- [x] Physics: `Spring` (stiffness, damping, mass, or response time and damping ratio, presets), `Decay` (fling with friction and projected end), interruptible without a jump
- [x] 60 unit tests and a fuzz target; builds for RISC-V, ARM and Xtensa
- [x] `Animator<N>`: a fixed set of running tweens, springs and flings on one frame clock; `step` says whether another frame is needed; stale-safe handles
- [x] Drag helpers: `rubber_band` (resisting overscroll) and `snap_page` (where a flick settles, at most one page)
- [ ] Gesture-driven motion in a widget: a pager that follows the finger, lets go into a spring, and a scroll list with fling (needs the widget tree)
- [ ] Shared and layout animations: a card that grows into a page, list reorder and insert, stagger helpers
- [ ] More physics: gravity and bounce, a spring chain for trailing motion, velocity from touch history
- [ ] Vector animation like Rive or Lottie: a state machine over timelines, playing `velato` (Lottie) and `vello_svg` content; a design-tool file format is out of scope
- [ ] Move `hibari-os-core::anim` onto `fusuma-anim` (one animation crate for the OS and the toolkit)

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
- [ ] `fusuma`: a `Theme` trait and a default light and dark theme (colours, spacing, radii, text sizes, motion timings), and widgets that read only from it
- [ ] `dodai-fusuma` in the `dodai-ui` repository: Dodai themes as a Fusuma theme, generated from the component spec where possible (a Fusuma target for `dodai-gen`), plus Dodai-only widgets
- [ ] Theme switching at run time, and density scaling
- [ ] **Animation overlap:** `fusuma-anim` (fixed point, `no_std`, runs without an FPU) and `dodai-core::motion` (`f32`, `std`, richer: reactions, glide, mixins) do the same job. Keep both for now, because Hibari on a microcontroller cannot take the `f32`/`std` one; map the theme's motion tokens (duration, easing, spring response) onto `fusuma-anim`; consider moving shared curves and presets to one source later, and do not add features to both

## Text

Two tiers, because a microcontroller and a phone need different things.

* **Microcontrollers keep the built-in bitmap fonts.** They need no allocator, no font files and no
  Unicode tables. Today these are three monospace sizes; a proportional pixel font and a glyph atlas
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

Costs, honestly: ICU4X's compiled data is large (the `icu_segmenter` source alone is 23 MB), so a Parley
build is a megabyte-class dependency and out of the question for the S3's firmware. The `complex-scripts`
feature (dictionary breaking for CJK, Thai and others) adds more. Whether Parley builds for the ESP at all
with `libm` is unmeasured, and I would not try before the desktop path works.

- [ ] A `TextEngine` seam in `fusuma` so labels measure and draw through either tier: bitmap on a chip, Parley on a desktop
- [ ] `fusuma-text`: Parley layout (font selection, shaping, wrapping, bidi, alignment) behind the seam; system fonts through Fontique, with a bundled fallback font so tests do not depend on the machine
- [ ] An optional `draw_glyphs` on `Renderer`, implemented by `fusuma-render-vello-cpu` and `-vello` using their glyph runs; conformance test with a bundled font
- [ ] Text widgets: wrapped and multi-line labels, then an input field using Parley's selection and editing utilities (cursor, selection, IME) and a placeholder
- [ ] Measure: the Parley build size for a desktop binary, and whether a no-ICU path (the lightweight segmenter) is enough for Latin text
- [ ] Per-widget style: font family, size, weight and line height as theme tokens, so a design system can set them

## Phase 4: the toolkit

Design, from reading Masonry (Xilem's widget layer; ideas only, our own names and API): a **retained
tree** that keeps widget state, **passes** over it (events, update, animation, paint), **invalidation
flags** per node that travel to the root, **pointer capture** and **bubbling**, **actions** queued out
of the tree, and an optional view layer on top (as in Xilem) that rebuilds into the tree. For the
ESP the tree is a fixed arena with the widgets as a closed enum, text stored inline and no heap;
desktop-sized trees use the same code with a bigger `N`.

- [x] `fusuma` first slice: `Tree<N>` (generation-checked handles, remove, reuse), layout (`Content`, `Fixed`, `Fill(weight)`, padding, gap, cross alignment; measured through the renderer), theme tokens with default dark and light (contrast-tested), event pass (capture, hover, click actions, cancel on leave or disable), update pass (disabled flows down, layout on request, hover refreshed after layout), animation pass (button hover fade, only widgets that asked), paint pass. 53 tests, a fuzz target (646k runs), builds for RISC-V, ARM and Xtensa; a tree of 32 widgets is about 4 KB
- [ ] Show it on the ESP32-S3 board (a toolkit screen in `hibari-os`), measure frame time and memory
- [ ] Layout and paint only the changed subtrees (the flags are there; today the whole tree is redone when anything asks)
- [ ] A compose pass: offsets for scrolling without a layout
- [ ] Focus and keyboard, text input with an input method; accessibility with `AccessKit`
- [ ] More widgets: toggle, slider, progress, list with scrolling and fling (using `fusuma-anim`), dialog, tabs, text field; overlays and z-order
- [ ] A `Widget` trait for custom widgets behind an `alloc` feature (the built-in set stays a closed enum for the no-allocator case)
- [ ] A view layer: a lightweight description built from application state and diffed into the retained tree, with messages routed back
- [ ] Per-widget style hooks so that a design system can restyle a widget beyond the theme tokens
- [ ] `fusuma`: retained widget tree, layout on `taffy`, events, focus, theming from design tokens; a semantic tree so that other front ends (terminal, accessibility) can read it
- [ ] Widgets as modules of `fusuma`: toggle, slider, progress, list with scrolling, dialog, tabs, text field (split into a crate only if it grows large)
- [ ] Text input with an input method, and accessibility with `AccessKit`
- [ ] `fusuma-platform-mcu`, then `fusuma-platform-desktop` (`winit`)
- [ ] A `std` feature on `fusuma` with the run-time backend choice (GPU first, CPU fallback)
- [ ] Port the Hibari OS launcher to Fusuma widgets, and `dodai-fusuma` (Dodai widgets and a `ShaderView`) in the `dodai-ui` repository

## Phase 5: more platforms
- [ ] `fusuma-platform-android` and `-ios`: native windows and touch, no web wrapper
- [ ] `fusuma-platform-tui`: render the semantic tree as text cells (terminal over SSH, serial console on a headless chip, character displays)
- [ ] `fusuma-platform-web` (optional): WebAssembly into a canvas, no DOM widgets

## Out of scope
- A webview, HTML or CSS
- The C++ Skia
- A design-tool file format such as Rive's (playing Lottie and SVG is in scope)
