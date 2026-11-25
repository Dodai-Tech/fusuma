# Fusuma roadmap

Fusuma is a native Rust UI toolkit from microcontrollers to desktops and phones: no webview, no HTML, no CSS.
`[x]` is done (with the evidence), `[ ]` is not. Numbers are measured unless they say otherwise; the S3 watch is the
Waveshare ESP32-S3-Touch-AMOLED-2.06 (410x502), the P4 the Waveshare ESP32-P4 7" (1024x600).

## Order of work

1. **GUI renderers** (mostly done): the integer renderer gets Vello's look at its own speed on microcontrollers.
   Open: [smooth strokes and paths](#renderers), images with alpha.
2. **UI components and logic** (next): icons in widgets, tabs, menu, toast, gestures, layout of only what changed.
   See [Toolkit](#toolkit).
3. **TUI**: cells in the toolkit, then running apps in a terminal, a serial console and on a panel. See
   [Text UI](#text-ui); the `fusuma-cells` and `fusuma-platform-tui` scaffolds wait until then.

Where the watch stands, the same showcase on the S3 with ESP-IDF at 240 MHz:

| | First frame | A tap | A scroll step or page |
|---|---|---|---|
| Vello CPU with Inter | 558 ms | 47 ms | 311 ms |
| Integer renderer, bitmap text (before the work below) | 97 ms | 7 ms | 88 ms |
| Integer renderer, smooth text and shapes (now) | 92 ms | not measured | not measured |

## Decisions

* **Apps add one crate, `fusuma`**, a facade over the toolkit, the renderer choice and the backends (cargo features).
  The renderer crates stay separate: each is useful alone, and a firmware that only draws needs no toolkit.
* **Two renderers, chosen by whether Vello has SIMD.** Vello where it has SIMD (x86, x86_64, aarch64, WebAssembly: the
  levels `fearless_simd` has), on the GPU when there is one, else `vello_cpu`, else the integer renderer if Vello's
  frame does not fit. The integer renderer on every microcontroller (S3, C6, P4, Cortex-M), where Vello runs a scalar
  fallback; its look (smooth text and edges) comes from its own code, held to Vello by tests. A board can still pin
  Vello in `fusuma.toml`. Replaced: "Vello only", then "the richest backend that fits in memory", and `tiny-skia`.
* **Configuration is data.** `fusuma.toml` (`[render]`, `[fonts]`, `[theme]`), board files (`board.toml`) and the
  font catalog (`catalog.toml`), each checked on load with an error that names the key, and a committed JSON schema.
* **A component exists once.** Devices differ in layout (size classes) and theme, never in forked widgets. A platform
  crate builds no widgets, lays nothing out and draws nothing; a renderer crate never depends on the toolkit (CI:
  `scripts/ci/layers.nu`).
* **`fusuma-ui` and `fusuma-anim` are `no_std` with no allocator**: a fixed arena `Tree<N>`, text inline, fixed-point
  animation that behaves the same without an FPU.
* **Design systems plug in from outside**, for pixels and cells. Fusuma ships a plain theme, palettes and a theme
  interface and never imports a design system; Dodai maps onto it from `dodai-fusuma` (`dodai-ui` repository), its
  text-UI side a `tui` feature there. On a board the mapping runs at build time (`dodai-core` is `f32` and `std`).
* **Fonts beyond Latin are tiered.** Bundled: Latin and Romanian subsets. Desktop and phone get other scripts from
  system fonts through Fontique (to wire up; Fusuma never bundles CJK). A chip with PSRAM may get Greek and Cyrillic
  subsets as an option (size unmeasured). Unknown: whether `vello_cpu` draws colour emoji.
* **Right-to-left and complex scripts are later work, desktop and phone only.** A label probably renders right on
  Vello (Parley shapes and reorders), untested. The text field is known to be wrong for them (caret and selection on
  UTF-8 boundaries), and layout has no direction. Chips stay left-to-right Latin.
* **A text UI is a separate platform, not a renderer.** Through pixel primitives a checkbox is a box and a tick path;
  a cell grid would have to guess `[x]`. Tree, state, events, focus, scrolling, dialogs and the frame loop are shared;
  each widget's cell look is a second paint pass in `fusuma-ui` into a `CellSink` (decided: option A).
* **Effects and shaders are optional capabilities**: a backend reports what it can do, and widgets fall back to a
  plain fill.
* **Native only, MIT.** The web is a canvas drawn by Vello from WebAssembly, not a webview or DOM widgets. Fusuma is not published yet:
  no versions, no deprecations; breaking changes are fine.

## Crates

| Crate | What | `no_std` |
|---|---|---|
| `fusuma` | the facade an app adds: re-exports the toolkit and, by feature, `render`, `anim`, `platform`, `text`, `fonts` and the backends | with `default-features = false` |
| `fusuma-ui` | the toolkit: widget tree, layout, theme and palettes, events, focus, scrolling, dialogs, text input, animation, paint, `Shell`, `App` | yes, no allocator |
| `fusuma-anim` | fixed-point easing, tweens, keyframes, springs, fling decay, clips, an animator, drag helpers | yes |
| `fusuma-render` | the `Renderer` trait, geometry, colours, paths, text styles, the conformance suite | yes |
| `fusuma-render-embedded` | the integer renderer: RGB565, smooth text from glyph atlases, smooth shapes in integer maths | yes |
| `fusuma-render-vello-common`, `-vello-cpu`, `-vello` | the `Renderer` on Vello, on the CPU and on the GPU (`wgpu`) | CPU with `libm` |
| `fusuma-text` | Parley layout over the linked font families, cached | with alloc |
| `fusuma-fonts` | the font catalog (families, presets, icons), `FontSet`, glyph atlases made at build time | yes |
| `fusuma-platform` | the renderer choice, settings (`fusuma.toml`) and board files (`board.toml`) | yes (`toml` needs `std`) |
| `fusuma-platform-embedded` | what every board shares: `Board`, `Driver`, the renderer surface, touch tracking, timing | yes |
| `fusuma-platform-esp` | ESP-IDF runtime and drivers per controller; boards are files (not a workspace member) | no |
| `fusuma-platform-desktop` | a `winit` window, GPU Vello else CPU | no |
| `fusuma-cells`, `fusuma-platform-tui` | scaffolds of the text UI: cell grid, diff, ANSI; a `crossterm` terminal | yes / no |
| planned: `fusuma-shaders`, `fusuma-platform-mobile`, `fusuma-platform-web` | shader engine; Android and iOS; the web on Vello (WebGPU, else CPU in wasm) | no |

## Renderers

- [x] `Renderer` contract with a conformance suite every backend passes; fuzzed
- [x] Vello CPU and GPU through one adapter (`fusuma-render-vello-common`); the GPU one passes on a real GPU (AMD RX
  7800 XT, Vulkan) and matches the CPU one; tests skip without a GPU
- [x] Partial redraws: `vello_cpu` rasterises only the damaged area, in bands through one reused 128 KB buffer, pixel
  for pixel equal to a full render there (S3: a hover 127 -> 50 ms, a page about 450-1100 -> 265 ms with Vello)
- [x] Fixed: the Vello CPU scene piled up across partial frames, so the watch slowed down with use; each render now
  draws only new commands over the picture (a scroll step on the host 380-1670 us growing -> 120-160 us flat)
- [x] Pixel-shift scrolling (`shift_area`, `will_draw`): a container that only scrolled has its pixels moved and only
  the exposed strip drawn; 14 tests compare every scrolled frame with a full redraw. Vello CPU only
- [x] Renderer choice by SIMD with layered settings, a memory check and a logged reason (`fusuma-platform`)
- [x] **Smooth text on the integer renderer**: glyph atlases made at build time (4-bit coverage) blended into RGB565.
  Held to Vello: ink in the same place within a pixel, the amount within 11 %
- [x] **Smooth shapes on the integer renderer**: rounded rectangles, circles and borders from their coverage in integer
  maths, only the corner columns pixel by pixel; alpha fills, so the dialog scrim and the scrollbar fade show. Held to
  Vello: within 1.5 % of the ink pixel by pixel
- [x] Faster copy to the panel: a block of rows in one pass in the panel's byte order (S3: 16.8 -> 12.7 ms a full
  frame); per-stage timing in every driver step (`Timing`: input, draw, raster, present)
- [ ] Smooth strokes along paths (the checkbox tick, chevrons) and smooth filled paths on the integer renderer
- [ ] Images with alpha on the integer renderer (drawn on/off today)
- [ ] Pixel-shift scrolling on the integer renderer (a full redraw of the window today)
- [ ] Several damage rectangles, so a hover and a status line do not repaint everything between them
- [ ] The GPU backend drawing to a window surface with no read-back
- [ ] Vello multithreading on the desktop (`vello_cpu`'s `multithreading` feature; unknown whether the partial
  render path uses the threads)
- [ ] Benchmarks of a complex scene (many paths, blurs) and of the GPU without read-back

## Text and fonts

- [x] Parley layout (`fusuma-text`, one line, cached) drawn by Vello's glyph runs, on the desktop, the P4 and the S3
- [x] **Font families as data**: `fusuma-fonts/fonts/catalog.toml` lists ten OFL families (Inter, Roboto, Atkinson
  Hyperlegible Next, IBM Plex Sans, Noto Sans and their monos), each a cargo feature, and five presets (`fusuma`,
  `material`, `legible`, `plex`, `noto`). `[fonts]` in `fusuma.toml` picks among the linked ones (`FontSet::choose`);
  every renderer takes the same `FontSet`. `just fonts-fetch` fetches, instances and subsets a family
- [x] **Icons drawn like text**: Material Symbols Rounded (Apache 2.0), the catalog's 90 icons in 17 KB,
  `FontSet::icon("settings")`, `Tree::icon`, `TextRole::Icon`; an icon fills a square on both renderers
- [x] Atlas sizes from `[fonts] sizes` (read at build time through `FUSUMA_SETTINGS`); the watch firmware asks for its
  theme's 15, 20, 24 and 30
- [ ] Wrapped and multi-line labels, a multi-line text view
- [ ] Kerning in the atlases; sizes other than those built
- [ ] System fonts through Fontique on desktop and phone; Greek and Cyrillic subsets as an option
- [ ] Lucide and Phosphor icons (each a catalog entry); filled icons (the `FILL` axis)
- [ ] The text field: caret and selection from Parley's clusters; drag and word selection, undo, longer text
- [ ] The bitmap font (the fallback without `fonts`) is ASCII only: `é` and `ș` draw as `?` there

## Theming

- [x] `Theme` (colours, spacing, radii, text sizes, fonts, sizes, motion) with a dark and a light default; every
  widget reads only from it; `Theme::scaled(percent)` for the display scale
- [x] **Palettes and accents**: fusuma, forest, ocean, petrol, violet for dark and light, and any palette's main action
  on any other; `[theme]` in `fusuma.toml`, switched at run time. Every combination keeps the contrast rules
  (text 7:1, muted 4.5:1, text on the main action 4.5:1, the main action 3:1 on the page), checked by tests
- [x] Material-style scrollbar: hidden at rest, shown while content moves, faded after 300 ms over 250 ms
- [ ] Per-widget style hooks so a design system can restyle a widget beyond the tokens
- [ ] `dodai-fusuma` in `dodai-ui`: a first adapter exists on an unpushed local branch there; waits for that repo's
  facade work. A Fusuma target for `dodai-gen` would keep adapters in step
- [ ] One source for motion: `fusuma-anim` (fixed point) and `dodai-core::motion` (`f32`) do the same job; map the
  theme's motion tokens onto `fusuma-anim`

## Toolkit

Design (ideas from Masonry, our own names): a retained tree that keeps widget state, passes over it (events, update,
animation, paint), invalidation flags that travel to the root, pointer capture and bubbling, actions queued out of
the tree. On a chip the tree is a fixed arena with the widgets as a closed enum.

- [x] `Tree<N>` with generation-checked handles; layout (`Content`, `Fixed`, `Fill(weight)`, padding, gap, cross
  alignment); events, update, animation and paint passes; a 32-widget tree is about 4 KB; fuzzed
- [x] Widgets: label, icon, button, checkbox, radio, switch, slider, progress bar, list item, text field (one line,
  IME and clipboard on the desktop), row, column, spacer, scroll container, dialog
- [x] Focus and keyboard (Tab, arrows, Enter, Escape)
- [x] Dirty-region painting: changes record what they touch, a property test proves the damage covers every changed
  pixel and that redrawing it equals a full redraw; fuzzed
- [x] Scroll physics: rubber band, fling with friction, bounce, take-over by wheel, keys and code; in real time on
  slow frames
- [x] Dialog: modal, centred, a scrim where the renderer blends, focus moves in and back, `Dismissed` on a press
  outside or Escape; nested dialogs
- [x] Screen shape (`Rect`, `Rounded`, `Circle`) with safe insets; size classes in the showcase (watch, phone,
  tablet, desktop by logical width)
- [ ] Icons in buttons, list items and tabs
- [ ] Tabs, menu, toast
- [ ] Gestures: long press and swipe (tap, drag and fling exist inside widgets)
- [ ] Layout only the changed subtrees (today any change lays out the whole tree; unmeasured)
- [ ] A compose pass: scroll offsets without a layout
- [ ] Animation wired into widgets: `enter`, `exit`, node offset and opacity, page transitions, a dialog entrance,
  an animation budget per chip (`fusuma-anim` has the vocabulary: spring tokens and `Clip`)
- [ ] Size classes and shape handling moved from the showcase into the toolkit
- [ ] Virtualised lists (hundreds of rows with a fixed node capacity)
- [ ] A semantic tree (read-only kind, text, state, rectangle, focus, children): the text UI and accessibility need it
- [ ] Accessibility with `AccessKit`
- [ ] A `Widget` trait for custom widgets behind an `alloc` feature; a view layer diffed into the tree
- [ ] Port the Hibari OS launcher to Fusuma widgets

## Animation

- [x] `fusuma-anim`: `Fx` (Q16.16), easing (incl. CSS Bezier), `Tween` with delay, stagger and retarget, `Keyframes`,
  `Spring` (stiffness and damping, or response and damping ratio), `Decay`, `Animator<N>`, `rubber_band`,
  `snap_page`, six spring tokens and a composable `Clip` with presets; builds for RISC-V, ARM and Xtensa; Hibari uses it
- [ ] More physics: gravity and bounce, a spring chain, velocity from touch history
- [ ] Shared and layout animations: a card growing into a page, list reorder and insert
- [ ] Vector animation: a state machine over timelines, playing Lottie (`velato`) and SVG (`vello_svg`)

## Effects and shaders

- [ ] `Capabilities` gains `effects` and `shaders`
- [ ] `Effects` (gradients, drop shadow, blur, opacity and blend layers, masks), first on Vello; widgets fall back
- [ ] A shader contract (`ShaderSource`, uniforms, `draw_shader`) and `fusuma-shaders` (the engine of
  `dodai-shaders`, rendering to a texture on the host's `wgpu` device); a `ShaderView`, backdrop blur, transitions.
  `dodai-shaders` needs Rust 1.97, Fusuma is at 1.90: a feature or a raised MSRV

## Boards and platforms

- [x] Desktop (`winit`): mouse, touch, keys, wheel, IME, clipboard; GPU Vello else CPU; sleeps when idle (312
  wake-ups in 5 s -> 0); Linux run, macOS and Windows built in CI only
- [x] `fusuma-platform-embedded`: `Board`, `Driver`, touch tracking (a held finger survives a few failed reads),
  aligned regions, row copies in the panel's byte order, timing; `no_std`, tested with a fake board; builds for
  RISC-V, Cortex-M0+, Cortex-M4F and bare-metal ARM64
- [x] **Boards are files, drivers are per controller**: `crates/fusuma-platform-esp/boards/<name>/board.toml` with its
  sdkconfig and partition table; every controller, pin and option checked, errors name the key. Drivers: CO5300
  (QSPI AMOLED), EK79007 (DSI), FT3168, GT911, LEDC backlight. Hibari reads the same format
- [x] `just showcase-esp` asks the connected chip which board it is, builds for it and flashes `ota_0` (the factory
  app stays)
- [x] The S3 watch on ESP-IDF: touch, bands of even rows through two DMA buffers (the CO5300 takes only even
  rectangles)
- [x] The P4: built after the move to board files; **not flashed since** (no P4 at hand), so its GT911 on the new I2C
  driver and the LEDC backlight from the board file are untried
- [ ] The C6 (a board file; `esp-hal` as well as ESP-IDF), a Cortex-M board (RP2040, STM32, nRF), a Linux
  framebuffer or DRM board for kiosks
- [ ] Android and iOS (`fusuma-platform-mobile`), the platform picked by target
- [ ] **Web on Vello** (`fusuma-platform-web`, or a web module of the desktop crate: both are `winit` + Vello): one
  `<canvas>`, no DOM widgets, no CSS. Vello GPU through WebGPU, falling back to Vello CPU in wasm with SIMD128 (the
  renderer rule already counts wasm as SIMD); Parley and the same font catalog in wasm; `winit`'s web backend for
  pointer, touch, keys and resize; a hidden `<textarea>` that follows the focused field for typing, IME and the phone
  keyboard; later the semantic tree mirrored as hidden DOM nodes for screen readers. Unmeasured: the size of the
  `.wasm` with Vello and Parley. The text UI comes nearly free there: `fusuma-cells`' ANSI into xterm.js. Blitz
  (DioxusLabs) is the reference for this stack (Vello through `anyrender`, Parley, Taffy, AccessKit), without its
  HTML and CSS
- [ ] An `HtmlView` widget, optional and `std` only (desktop, phone, web): HTML or markdown content (help pages,
  documents, rich text) rendered by Blitz into a texture that Fusuma draws. Not the way to build Fusuma's own UI, and
  never on a chip (Stylo is large)

## Text UI

A separate platform with the same tree and widgets, each drawn in its cell form (`[x]`, `(•)`, `[███░░]`) in a
terminal, over SSH, on a chip's serial console or on a panel. Better than `ratatui` should be shown with numbers:
retained (focus, scrolling, dialogs built in), one app for pixels and cells, `no_std`, diffed output.

- [x] `fusuma-cells` scaffold (`no_std`, no allocator): cells, colour depth with fall-back, `Grid`, a diff into runs,
  an ANSI encoder that sends only changes, a byte-at-a-time key decoder; builds for RISC-V and Cortex-M0
- [x] `fusuma-platform-tui` scaffold: `Terminal` (raw mode, alternate screen, restored on drop), colour depth
  detection, `crossterm` events as `fusuma::Input`; an example (a 50x16 first frame 968 bytes, a key press 54)
- [ ] A `Measure` trait so layout needs no `Renderer`; a `CellSink` paint pass; a cell theme (glyph sets, colour
  depth, emphasis per state) loadable as data; text snapshot tests
- [ ] Running an app; then the serial console of a chip, then a panel (cells through the integer renderer, one region
  a frame)
- [ ] Mouse, wide characters and Alt in the decoder
- [ ] Measure `ratatui` with `mousefood` on the watch before claiming anything

## Out of scope

- A webview, HTML or CSS
- The C++ Skia
- A design-tool file format such as Rive's (playing Lottie and SVG is in scope)
