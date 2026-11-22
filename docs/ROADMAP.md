# Fusuma roadmap

Fusuma is a native Rust UI toolkit from microcontrollers to desktops and phones: no webview, no
HTML, no CSS. Status: `[x]` done, `[ ]` planned. Order matters: each phase leaves something that
works and is tested.

## Decisions

* **One dependency for apps.** Apps use `fusuma`; the backend is chosen by target and, on a
  desktop, at run time (GPU first, CPU if there is no usable adapter). The backend crates are
  published because crates.io needs them, but their READMEs say "internal".
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
| `fusuma-text` | text layout and fonts in pure Rust (`parley`, or `cosmic-text`) | no |
| `fusuma-core` | widget tree, layout (`taffy`), events, focus, state | partly |
| `fusuma-widgets` | label, button, toggle, slider, list, dialog, tabs, text field | partly |
| `fusuma-platform-mcu` | microcontroller displays and touch | yes |
| `fusuma-platform-desktop` | `winit` window, pixel buffer or GPU surface | no |
| `fusuma-platform-android`, `-ios` | native windows and touch | no |
| `fusuma-platform-tui` | semantic rendering of the widget tree as text cells (ratatui) | no |
| `fusuma-platform-web` | later, optional: WebAssembly into a canvas | no |
| `fusuma` | facade: one dependency, features and run-time backend choice | no |

## Phase 1: renderer
- [x] `fusuma-render`: `Renderer` trait, conformance suite, fuzzed
- [x] `fusuma-render-embedded`: integer backend, runs on the ESP32-S3
- [x] `fusuma-render-vello-cpu` on `vello_cpu`, passing the conformance suite and matching the integer backend pixel for pixel; fuzzed
- [x] `fusuma-render-vello-common`: one implementation for both Vello backends
- [x] Published 0.1.0 of `fusuma-render`, `-embedded` (and the dropped `-skia`)
- [ ] Publish `fusuma-render-vello-common` and `-vello-cpu`, move `dodai-os` to them
- [x] `fusuma-render-vello` on `vello_gpu` and `wgpu` 30: headless device, renders into an `Rgba8Unorm` texture and reads it back; passes the conformance suite on a real GPU (AMD RX 7800 XT, Vulkan) and matches the CPU backend; tests skip on machines without a GPU. Images go to the GPU as external textures through a `Scene::image_source` hook
- [ ] `fusuma-render-vello`: render into a host-owned texture or window surface with no read-back; `gpu_available()` is there for the facade's GPU-first choice
- [x] Benchmark of a UI frame (`cargo run --release -p fusuma-render-vello --example bench`, read-back included): at 410x502 `vello_cpu` 0.18 ms and `vello_gpu` 0.61 ms; at 1920x1080 1.75 ms and 3.56 ms. A simple scene: the CPU wins while the picture has to come back to the CPU
- [ ] Benchmark more: a complex scene (many paths, large blurs), the GPU drawing to a surface with no read-back, and a shader chain with read-back versus as a texture
- [ ] Experiment: run `vello_cpu` on the S3 (it builds with `libm`) and measure frame time and memory
- [ ] Text: Vello glyph runs with a Rust text stack replace the bitmap fonts on the CPU and GPU backends

## Phase 2: animation (out of the box)
- [x] `fusuma-anim`: `Fx` (Q16.16), easing (quad, cubic, quart, smooth, back, bounce, CSS Bezier), `Tween` with delay, stagger and retarget, `Keyframes`
- [x] Physics: `Spring` (stiffness, damping, mass, or response time and damping ratio, presets), `Decay` (fling with friction and projected end), interruptible without a jump
- [x] 43 unit tests and a fuzz target; builds for RISC-V, ARM and Xtensa
- [ ] Animator: one frame clock, a set of running animations, "needs a frame" for redraw on demand
- [ ] Gesture-driven motion: drag with rubber-band overscroll, snap to pages and items from `Decay::projected_end`, spring release
- [ ] Shared and layout animations: a card that grows into a page, list reorder and insert, stagger helpers
- [ ] More physics: gravity and bounce, a spring chain for trailing motion, velocity from touch history
- [ ] Vector animation like Rive or Lottie: a state machine over timelines, playing `velato` (Lottie) and `vello_svg` content; a design-tool file format is out of scope
- [ ] Move `dodai-os-core::anim` onto `fusuma-anim` (one animation crate for the OS and the toolkit)

## Phase 3: effects and shaders
- [ ] `Capabilities`: `effects` and `shaders` flags next to `alpha` and `antialiasing`
- [ ] `Effects` extension trait (feature-gated, unstable): gradients (linear, radial, sweep), drop shadow, blur, blurred rounded rectangle, opacity and blend layers, masks. First on `vello_cpu` (it has filter layers and blurs), then `vello_gpu`; the integer backend does not implement it and widgets fall back to a plain fill
- [ ] Shader contract: `ShaderSource` (WGSL text or a named prebuilt kernel), uniforms as bytes, a `draw_shader` call on GPU backends, one conformance shader
- [ ] `fusuma-shaders`: the engine of `dodai-ui-shaders` (GPU compute kernels from rust-gpu as embedded WGSL, plus the CPU path from the same Rust source), render to a texture on the host's `wgpu` device so there is no read-back; raw WGSL for custom shaders. Decide with `dodai-ui` where the line between engine and looks is
- [ ] Shader widgets: a `ShaderView`, backdrop blur, animated backgrounds, transitions
- [ ] MSRV: `dodai-ui-shaders` needs Rust 1.97 and Fusuma is at 1.90; keep shaders behind a feature or raise the MSRV
- [ ] Optimization pass with numbers: no read-back, skip invisible shaders, render small and scale, one frame clock, pipeline warm-up

## Phase 4: the toolkit
- [ ] `fusuma-core`: retained widget tree, layout on `taffy`, events, focus, theming from design tokens; a semantic tree so that other front ends (terminal, accessibility) can read it
- [ ] `fusuma-widgets`: label, button, toggle, slider, list with scrolling, dialog, tabs, text field
- [ ] Text input with an input method, and accessibility with `AccessKit`
- [ ] `fusuma-platform-mcu`, then `fusuma-platform-desktop` (`winit`)
- [ ] `fusuma` facade and run-time backend choice (GPU first, CPU fallback)
- [ ] Port the Dodai OS launcher to Fusuma widgets, and `dodai-ui-fusuma` (Dodai widgets and a `ShaderView`) in the `dodai-ui` repository

## Phase 5: more platforms
- [ ] `fusuma-platform-android` and `-ios`: native windows and touch, no web wrapper
- [ ] `fusuma-platform-tui`: render the semantic tree as text cells (terminal over SSH, serial console on a headless chip, character displays)
- [ ] `fusuma-platform-web` (optional): WebAssembly into a canvas, no DOM widgets

## Out of scope
- A webview, HTML or CSS
- The C++ Skia
- A design-tool file format such as Rive's (playing Lottie and SVG is in scope)
