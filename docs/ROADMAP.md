# Fusuma roadmap

Fusuma is a native Rust UI toolkit from microcontrollers to desktops and phones: no webview, no HTML, no CSS.
`[x]` is done (with the evidence), `[ ]` is not. Numbers are measured unless they say otherwise; the S3 watch is the
Waveshare ESP32-S3-Touch-AMOLED-2.06 (410x502), the P4 the Waveshare ESP32-P4 7" (1024x600).

## Order of work

1. **GUI renderers** (done): the integer renderer has Vello's look at its own speed on microcontrollers.
2. **UI components and logic** (done, see [Toolkit](#toolkit)).
3. **TUI** (done, see [Text UI](#text-ui)): the showcase runs in a terminal, over a serial console and on a panel.
4. **Web** (done, see [Boards and platforms](#boards-and-platforms)).

Where the watch stands, the same showcase on the S3 with ESP-IDF at 240 MHz:

| | First frame | A tap | A scroll step or page |
|---|---|---|---|
| Vello CPU with Inter | 558 ms | 47 ms | 311 ms |
| Integer renderer, bitmap text (before the work below) | 97 ms | 7 ms | 88 ms |
| Integer renderer, smooth text, shapes and paths, pixel-shift scrolling, the toolkit of this round (now) | 92 ms | 12 ms (small updates, average of 91 in a scripted run, worst 13) | 36 ms (a scroll step, average of 92, worst 42; 32 ms of it drawing: moving 232 KB of PSRAM, then the strip) |

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
| `fusuma-cells` | the text UI's cells: grid, diff, ANSI encoder, key and mouse decoder, wide characters | yes |
| `fusuma-platform-tui`, `fusuma-platform-console` | the text UI in a terminal (`crossterm`) and over a serial console (any `fmt::Write`) | no / yes |
| `fusuma-cells-panel` | the text UI on a pixel panel through any renderer, one region a frame | yes |
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
- [x] **Smooth strokes and filled paths on the integer renderer** (`fill.rs`, integer maths, no allocation): paths cut
  into 1/16 px edges, four samples a pixel row with exact horizontal coverage; strokes are quads, miter joins (Vello's
  limit of 4) and round caps, wound alike and filled non-zero so a translucent stroke is one layer. Held to Vello on a
  tick, a chevron, a slanted line, a triangle and a curve (ink within 4 %, pixels within 8 %)
- [x] **Images with alpha on the integer renderer**: each pixel mixed into the frame (`Blend::blend_colors`); held to
  Vello within 16 per channel (RGB565 keeps 5 or 6 bits)
- [x] **Pixel-shift scrolling on the integer renderer** (`Blend::shift_rows`, `Frame` moves the rows): the 14 scroll
  tests run on both renderers; on the S3 watch a scroll step went 88 -> 35 ms average (40 worst; the scripted `demo`
  firmware feature scrolls and taps with nobody touching)
- [x] **Several damage rectangles** (`Pieces`, up to four, joined when near or when full): `paint_frame` draws each
  apart, `App::frame_rects` and the embedded driver send each apart; Vello CPU is told each with `will_draw`. A test
  on both renderers and a driver test (two labels at the ends of a tall screen: 2 rectangles, under a quarter of the
  pixels of their bounding box)
- [x] The GPU backend drawing to a surface with no read-back (`GpuScene::present`, `from_device`): used by the web
  build; the desktop window still reads back
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
- [x] Wrapped labels and a multi-line text view (see Toolkit)
- [x] **Kerning in the atlases**: each face's GPOS `kern` pairs among the printable ASCII (Inter: about 1,050 pairs,
  6 KB a face) read at build time, applied when measuring and drawing; five lines of Inter at 10, 15 and 20 px are
  now exactly as wide as Vello's (2 to 5 px narrower than the sum of advances)
- [ ] Sizes other than those built
- [ ] System fonts through Fontique on desktop and phone; Greek and Cyrillic subsets as an option
- [x] **Lucide and Phosphor icons**, opt-in (`icons-lucide`, `icons-phosphor`; `[fonts] icons = "lucide"`): the same 90 names as
  Material Symbols through aliases in the catalog (`home=house`), 30 KB and 23 KB, name tables read from the three
  formats (`codepoints`, lucide's `info.json`, a stylesheet); Phosphor's dip below the baseline is lifted so every icon
  stays in its square; held to Vello for all three families on the integer renderer
- [ ] Filled icons (the `FILL` axis)
- [x] The text field: drag and word selection, undo, a placeholder, longer text (a feature)
- [ ] The text field: caret and selection from Parley's clusters (right-to-left and complex scripts)
- [ ] The bitmap font (the fallback without `fonts`) is ASCII only: `é` and `ș` draw as `?` there

## Theming

- [x] `Theme` (colours, spacing, radii, text sizes, fonts, sizes, motion) with a dark and a light default; every
  widget reads only from it; `Theme::scaled(percent)` for the display scale
- [x] **Palettes and accents**: fusuma, forest, ocean, petrol, violet for dark and light, and any palette's main action
  on any other; `[theme]` in `fusuma.toml`, switched at run time. Every combination keeps the contrast rules
  (text 7:1, muted 4.5:1, text on the main action 4.5:1, the main action 3:1 on the page), checked by tests
- [x] Material-style scrollbar: hidden at rest, shown while content moves, faded after 300 ms over 250 ms
- [x] Per-widget style hooks (`Hooks`)
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
- [x] **Icons in buttons, list items and tabs** (`button_with_icon`, `list_item_with_icon`, `tab`): the icon is sized and
  placed with the text; tests count the width it adds and look for its ink
- [x] **Tabs** (`tab_bar`, `tab`: a group like list items, `Selected`, a bar under the chosen one), **menus** (`menu`,
  `open_menu`: a popover that hangs below its button or above it, keeps on screen, closes itself on a row or a press
  outside, no dimming layer), **toasts** (`toast`, `show_toast`: out of the flow, over dialogs, fades and slides in and
  out; the picture after it is gone equals the page)
- [x] **Gestures**: `set_long_press` (500 ms, `LongPressed`) and `set_swipe` (`Swiped(dir)` at 40 px, mostly along
  one axis), opt in; the press then ends without a click and a scroll under it lets go
- [x] **Layout only what changed**: a node whose subtree did not change keeps its size (a test counts the measurements:
  one label changed, one measure), and a scroll only places its children (0 measures); a test compares 80 random
  changes with a layout from scratch. Watch, a scripted run: see the table above
- [x] **Wrapped text**: `paragraph`, `paragraph_static`, `text_view`; words, newlines and over-wide words, with the
  renderer's own measuring (Parley on Vello, atlases on the integer renderer, not Parley's own line breaking)
- [x] **Text field**: drag and double-click (word) selection, Ctrl+arrows by word, undo and redo (four steps, typing
  and deleting grouped), a placeholder, and 250 bytes with the `long-text` feature (64 otherwise: every node takes the
  room of the biggest)
- [x] **Animation in widgets**: dialogs fade the screen and slide in and out (`shown`), pages slide in and out
  (`slide_in`, `slide_out`, `Exited`, a node `shift` that only re-places), toasts; `MotionBudget` (`Full`,
  `Reduced`, `Off`) on the theme, and the showcase's watch uses `Reduced`
- [x] **Size classes and screen shape in the toolkit**: `Class`, `Screen` (size, scale, shape, `class()`,
  `safe_bounds()`, `theme()`)
- [x] **Virtual lists** (`virtual_list`, `virtual_window`): 300 rows in 11 nodes, the picture at any offset equal to
  the real list's, through drag and fling
- [x] **A semantic tree** (`semantics`, `semantic`: role, name, state, value, rectangle, depth, focus; closed dialogs
  and hidden toasts left out)
- [x] **Style hooks** on the theme (`Hooks`: button, surface, item, tab, panel), each a function that takes the place
  of the default; a test draws with all five
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
- [x] **Web on Vello** (`examples/showcase-web`, `just showcase-web`, a `web` job in CI): the showcase in one
  `<canvas>`, from WebAssembly (3.6 MB, unoptimised, with Inter, JetBrains Mono and the icons). Checked in a headless
  browser: every page, clicks, the sidebar's wheel, tabs, the menu, a toast, a swipe, typing and composition.
  **Vello on WebGPU** when the browser has it (`GpuScene::present` copies the frame into the canvas's texture, no
  read-back; the whole picture each frame), **Vello on the CPU** as the fallback or with `?renderer=cpu` (a changed
  rectangle at a time into a 2D canvas). A **hidden `<textarea>`** follows the focused field: typing, an input
  method's composition (preedit underlined at the caret), paste, copy and cut; the keys edit by word and undo. It is
  `web-sys` on a canvas, not `winit`'s web backend. Not done: the semantic tree mirrored as hidden DOM for screen
  readers, `.wasm` size work (`wasm-opt`, `strip = false`), clipboard read for Ctrl+V outside paste events
- [ ] An `HtmlView` widget, optional and `std` only (desktop, phone, web): HTML or markdown content (help pages,
  documents, rich text) rendered by Blitz into a texture that Fusuma draws. Not the way to build Fusuma's own UI, and
  never on a chip (Stylo is large)

## Text UI

A separate platform with the same tree and widgets, each drawn in its cell form (`[x]`, `(•)`, `[███░░]`) in a
terminal, over SSH, on a chip's serial console or on a panel. Better than `ratatui` should be shown with numbers:
retained (focus, scrolling, dialogs built in), one app for pixels and cells, `no_std`, diffed output.

- [x] `fusuma-cells` (`no_std`, no allocator): cells, colour depth with fall-back, `Grid`, a diff into runs, an ANSI
  encoder that sends only changes, a key decoder with Alt, SGR mouse reports and UTF-8, wide characters (two columns,
  a tail cell the encoder skips); builds for RISC-V and Cortex-M0
- [x] **A `Measure` trait** (`fusuma-render`): layout asks only for text sizes, so `CellMeasure` lays out in cells with
  no pixel renderer; `Theme::CELLS` is the cell-sized theme (brackets, markers and a switch's width counted)
- [x] **A `CellSink` paint pass** (`Tree::paint_cells`): every widget in its text form (`[x] Wi-Fi`, `(•) Medium`,
  `[ ON ]`, `███░░░`, `[ OK ]`, `▸ row`, `[ Home ]`, a field with caret and selection, a scrollbar, boxes for dialogs, the
  page dimmed behind one, a toast)
- [x] **A cell theme as data** (`CellTheme`): glyphs and the attributes each state adds, `UNICODE`, `ASCII` and `MONO`
  (no colour, every state by attributes); the `[cells]` table of `fusuma.toml` picks a preset and changes glyphs and
  emphasis by name, checked on load (a box must be 3 wide), with the schema
- [x] **Text snapshot tests**: the gallery (every widget, every state) and the overlays in each cell theme, against
  golden files in `crates/fusuma-ui/tests/golden` (`FUSUMA_UPDATE_GOLDEN=1` rewrites them)
- [x] **`just showcase-tui`**: the showcase in a terminal (mouse, keys, resize; `--dump 80x24 Page` prints a frame as
  text; `--ascii`, `--mono`, `--settings`), the same app as the window and the watch; run through a pty in a check
- [x] **A serial console** (`fusuma-platform-console`): frames to any `fmt::Write`, bytes in as `Input`; the showcase
  runs over it in a test (a click as bytes: the next frame is a fifth of the first)
- [x] **A panel** (`fusuma-cells-panel`): a grid drawn through any renderer (blocks, box drawing and bullets as
  shapes, bold, dim, reverse, underline), only the changed runs, one rectangle for the board to send. Not yet flashed
  to a board
- [ ] Measure `ratatui` with `mousefood` on the watch before claiming anything

## Out of scope

- A webview, HTML or CSS
- The C++ Skia
- A design-tool file format such as Rive's (playing Lottie and SVG is in scope)
