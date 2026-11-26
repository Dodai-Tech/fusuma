# Fusuma roadmap

Fusuma is a native Rust UI toolkit from microcontrollers to desktops and phones: no webview, no HTML, no CSS.
`[x]` is done (with the evidence), `[ ]` is not. Numbers are measured unless they say otherwise; the S3 watch is the
Waveshare ESP32-S3-Touch-AMOLED-2.06 (410x502), the P4 the Waveshare ESP32-P4 7" (1024x600).

## Order of work

1. **GUI renderers** (done): the integer renderer has Vello's look at its own speed on microcontrollers.
2. **UI components and logic** (done, see [Toolkit](#toolkit)).
3. **TUI** (done, see [Text UI](#text-ui)): the showcase runs in a terminal, over a serial console and on a panel.
4. **Web** (done, see [Boards and platforms](#boards-and-platforms)).
5. **The Elm-style view layer** (in progress, see [View layer](#view-layer)): one program shape from an ESP32-C3 to the
   web, declarative, with no allocation on the chip.
6. **One screen description, phones and e-paper** (built, not run on hardware; see [Screens](#screens-one-description-on-every-platform),
   [E-paper](#e-paper-reflective-panels) and [Mobile](#mobile)): the Sticky and the phone hosts are next to test.

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
* **The app is Elm; the engine underneath is retained.** An app is a `State`, a `Msg` enum, `update` (the one place state
  changes), `view(&State)` and `subscription`. Events are values, not closures. Underneath, views are cheap typed
  values that make the retained `Tree<N>` match them and touch only what differs, so layout, damage and pixel-shift
  scrolling stay incremental. Taken from Iced: the program shape, `Task`/`Sub`, `Wake`, message log and replay,
  presets, the simulator with selectors. Taken from Xilem: the typed `build`/`rebuild`/`teardown` lifecycle, tuples
  as children, `memoize`, effects tied to a view, stashed (hidden) nodes. Left out: Iced's rebuild, relayout and
  redraw of everything per message and its `Box<dyn Widget>`; Xilem's state-mutating closures, id-path routing,
  `Environment` and `alloc`. One engine everywhere: only the capacities (`Tree<N>`), the renderer and the effect
  adapters differ per target.
* **Native only, MIT.** The web is a canvas drawn by Vello from WebAssembly, not a webview or DOM widgets. Fusuma is not published yet:
  no versions, no deprecations; breaking changes are fine.

## Crates

How the crates are layered (the same idea as Xilem over Masonry, or Iced over its widgets: the project-named crate is the API
apps use, the toolkit sits below it, and every host and every heavy dependency has a crate of its own):

```
fusuma                       the front door: the Elm API (Program, views, Task, Sub); the toolkit as fusuma::ui; feature switches
  fusuma-ui                  the complete toolkit: engine (tree, layout, damage, events), every widget, themes, hooks
    fusuma-render            the contract: the Renderer trait, geometry, colour, text types, conformance suite (no_std, no deps)
      fusuma-render-integer  the integer engine for chips (no_std, no allocator)
      fusuma-render-vello*   Vello on the CPU and the GPU
  fusuma-text, -fonts        shaping and bundled fonts (optional, by feature)
  fusuma-platform-*          hosts: desktop (winit), embedded (Board) + esp + eink, mobile (features android, ios, linux), tui (crossterm), console (any writer)
  fusuma-cells, -cells-panel the text UI
  fusuma-test, -palette      the simulator, palette generation
```

A crate is split from its neighbour only for one of three reasons: a heavy dependency (Vello, `winit`, `crossterm`,
`esp-idf-svc`) that a chip must never link, a different target (`no_std` against `std`), or a different audience (backend
authors, app authors, platform authors). Features choose what an app gets: `renderer-integer`, `renderer-vello-cpu`,
`renderer-vello-gpu`, `fonts-*`, `icons`; **a desktop app never compiles the integer renderer and a chip never compiles
Vello.** Not done: presets (`desktop`, `web`, `embedded`), a CI `cargo tree` check that proves those opt-outs, merging the
three Vello crates, folding `fusuma-platform` into `fusuma` (the ESP build script and the board crates use it on its own).

| Crate | What | `no_std` |
|---|---|---|
| `fusuma` | **the front door apps add**: the Elm architecture at the top (`Program`, `Runner`, `Task`, `Sub`, the views `button`, `column`, `card`, `each`, `memo`, `animate`, `class`), the toolkit as `fusuma::ui`, and by feature `render`, `anim`, `platform`, `text`, `fonts` and the renderer backends | with `default-features = false` |
| `fusuma-ui` | the toolkit: widget tree, layout (measures only what changed), theme, palettes and style hooks, events, focus, scrolling, dialogs, menus, toasts, tabs, text and wrapped text, gestures, slides, virtual lists, hidden nodes, app ids, the semantic tree, the cell paint pass, `Shell`, `App`, `Wake` | yes, no allocator |
| `fusuma-test` | the simulator: find, click, activate, type, time, messages, scripts, PNGs, on a program | no |
| `fusuma-palette` | palettes from a seed colour in OKLCH with readable text guaranteed | no |
| `fusuma-anim` | fixed-point easing, tweens, keyframes, springs, fling decay, clips, an animator, drag helpers | yes |
| `fusuma-render` | the `Renderer` trait, geometry, colours, paths, text styles, the conformance suite | yes |
| `fusuma-render-integer` | the integer renderer: RGB565, kerned smooth text from glyph atlases, smooth shapes, strokes, paths and images with alpha in integer maths, pixel-shift scrolling | yes |
| `fusuma-render-vello-common`, `-vello-cpu`, `-vello` | the `Renderer` on Vello, on the CPU and on the GPU (`wgpu`) | CPU with `libm` |
| `fusuma-text` | Parley layout over the linked font families, cached | with alloc |
| `fusuma-fonts` | the font catalog (families, presets, icons), `FontSet`, glyph atlases made at build time | yes |
| `fusuma-platform` | the renderer choice, settings (`fusuma.toml`) and board files (`board.toml`) | yes (`toml` needs `std`) |
| `fusuma-platform-embedded` | what every board shares: `Board`, `Driver`, the renderer surface, touch tracking, timing | yes |
| `fusuma-platform-eink` | e-paper over `fusuma-platform-embedded`: one-bit packing, batched refresh, SSD1680, SSD1677, SSD2677, the Sticky's panel detection (unpublished, not run on a panel) | yes |
| `fusuma-platform-esp` | ESP-IDF runtime and drivers per controller; boards are files (not a workspace member) | no |
| `fusuma-platform-desktop` | a `winit` window, GPU Vello else CPU | no |
| `fusuma-cells` | the text UI's cells: grid, diff, ANSI encoder, key and mouse decoder, wide characters | yes |
| `fusuma-platform-tui`, `fusuma-platform-console` | the text UI in a terminal (`crossterm`) and over a serial console (any `fmt::Write`) | no / yes |
| `fusuma-cells-panel` | the text UI on a pixel panel through any renderer, one region a frame | yes |
| planned: `fusuma-shaders` | shader engine | no |
| `fusuma-platform-mobile` | what every phone host shares: the surface lifecycle (a surface is made only with a window and the app in front, dropped the moment the system takes it), safe areas with the soft keyboard as an inset, the keyboard's show and hide once per change, back as cancel | yes |
| `fusuma-platform-mobile` | **scaffold** (not published): the shared phone lifecycle, insets and back, and per OS, behind the features `android`, `ios` and `linux`, touch and key translation, the shell and output rotation of Linux phones, and the `display(..)` conversions; no activity, view or window runs an app yet | yes |
| `examples/showcase`, `-desktop`, `-tui`, `-web`, `-esp` | the one app and its hosts: window, terminal, browser (wasm), boards | `showcase` yes |

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
- [x] **Audit of the renderer layer** (all five crates): line coverage 96.8 % (`cargo llvm-cov`: integer 96 to 99 %, vello-cpu 99 %,
  vello-common 90 %, the rest is the GPU-only paths), 0 missing docs, no TODOs, the READMEs brought up to date, **the draw fuzz
  target now also runs the one-bit frame** and found two real bugs, both fixed with a regression test that was seen to fail
  without the fix: a picture read inside an open clip left the Vello CPU scene without its clip, so the `restore` panicked
  (`open_clips`), and a path with points near the ends of `i32` overflowed the integer renderer's crossing maths (`crossing`, in
  `i128` when needed). `MonoFrame` is checked against the colour frame (exact for hard edges, within 8 to 15 % of the inked
  pixels for smooth ones), since the conformance suite compares colours and one bit has two
- [x] **Fuzzing covers the newer code**: `ui` now builds and drives flows, grids, charts, gauges, spinners, splitters, images, text areas,
  bottom sheets, drags and right-to-left; new targets `dates` (calendar and time arithmetic, the page window) and `eink` (the
  planner, byte regions, packing, one-bit drawing with its padding). They found five overflows in the first minutes (a bottom sheet
  at the edge of the range, a drag grab offset, a bar chart with more samples than pixels, and boxes near the ends of `i32` in the
  chart, gauge, spinner and splitter), all fixed with a test; each target ran clean for 8 more minutes afterwards
- [ ] **Known gaps in the layer:** no gradients, shadows, blur, opacity layers or shaders (see *Effects and shaders*); the integer
  renderer has no dashes, no image scaling and no clip other than a rectangle; the GPU backend is not fuzzed (no GPU in CI)
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
- [x] **The bitmap font is Latin-1** (`fusuma_render::bitmap`, shared by the integer and Vello backends: `é ü ñ ß £ ¿` draw as themselves,
  checked by the conformance suite); with `bitmap-extended` a character that is not in it comes from ISO 8859-16 (Romanian `ș ț ă`,
  Polish, Hungarian), 8859-2, 8859-5 (Cyrillic) or 8859-7 (Greek), **one character at a time** (a text is cut into runs of one font; every
  font of a size is as wide as the others, so a text is as wide as it has characters). About 100 KB of flash more, which is why it is a
  feature. Anything else (CJK, Arabic, Hindi) is still a `?` there: those need the vector text

## Theming

- [x] `Theme` (colours, spacing, radii, text sizes, fonts, sizes, motion) with a dark and a light default; every
  widget reads only from it; `Theme::scaled(percent)` for the display scale
- [x] **Palettes and accents**: fusuma, forest, ocean, petrol, violet, ember, graphite for dark and light, and any palette's main action
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
  alignment); events, update, animation and paint passes; a node is 184 bytes (a budget test keeps it from growing; the aim
  is 128), so a 32-widget tree is about 6 KB; fuzzed
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
- [x] **Building blocks** (`card`, `divider`, `section`, `badge`, `avatar`, `stepper`, in `blocks.rs`): arrangements of
  the basic widgets, no new node kinds; new `Surface::Border`, `Surface::Primary` (text on it is drawn in `on_primary`)
  and `Round::Full` (pill or circle). The showcase opens on a new **Overview** page made of them, and the palette tile
  on the watch gets a row of its own now that there are many pages
- [x] **Style hooks** on the theme (`Hooks`: button, surface, item, tab, panel), each a function that takes the place
  of the default; a test draws with all five
- [x] **Accessibility with AccessKit** (`fusuma-access`): `tree_update`, `app_update` and `update_of` turn the semantic tree into an AccessKit
  tree (window, roles, names, checked/selected/disabled, slider value and range, field value and hint, bounds, nesting, the focused
  widget, a modal dialog, a polite live toast); `perform` does what a reader asks (click, focus, increment and decrement, type,
  reveal) through `Tree::assist` / `App::assist`, refuses a stale handle (`Gone`) and what is behind a modal. The desktop crate's
  `accessibility` feature (winit adapter: AT-SPI on Linux, UIA on Windows, NSAccessibility on macOS) tells the reader after every
  frame; `--features accessibility` on the showcase. **Not tried with a real screen reader** (Orca, NVDA, VoiceOver): the tests check the
  tree and the requests. Still to do: the web (ARIA), Android and iOS adapters, text selection and caret for a reader, and live
  regions beyond the toast

- [x] **Tones** (`Tone`: info, success, warning, danger; `Colors::{success, warning, info}` in every palette and in the generated
  ones, found by the same contrast search as `danger`; `Surface::Tone` solid with black or white text that reads, `Surface::ToneSoft`
  a tinted card): every tone stands out of every page at 3:1 and its text reads at 4.5:1 in all palettes
- [x] **Components made of views** (`crates/fusuma/src/elm/components.rs`, no new node kinds, tested in `elm_components.rs`: what
  each builds, which message each part sends, 0 allocations when nothing changed): `alert`, `banner`, `tag`, `stat_card`,
  `empty_state`, `skeleton`, `snackbar` (with an action), `chip` (a toggle button: any number on), `segmented`, `breadcrumbs`,
  `pagination` (`page_window` for every page of every size, checked against the rules), `steps`, `accordion` (the body is kept and
  hidden, so it keeps its state). The showcase has a **Components** page that uses all but the accordion and answers each (a test presses every part)
- [x] **Gauge** (`Kind::Gauge`, `Tree::gauge`, `gauge()` view): an arc of 270 degrees with a number, stroked from a sine table in
  integers (no float, no allocation); 19 points, because the integer renderer holds about 20 segments of a stroked path
- [x] **Layout:** a wrapping row (`Kind::Flow`, `wrap()`), a grid of equal columns (`grid(cols, ..)`, `Tree::grid`; each row as tall as its
  tallest cell), both without allocation and following the width the parent gives them (two passes, like a paragraph); and
  **right-to-left** (`Tree::set_rtl`: rows, wraps and grids start at the right; the text itself is not reordered or shaped)
- [x] **More components made of views** (all in `components.rs` / `calendar.rs`, tested in `elm_components.rs`): `spinner` (a real
  turning node, still where there is no motion), `bottom_sheet` (a modal dialog on the bottom edge), `kbd`, `dismissible_alert`,
  `rating`, `page_dots`, `carousel`, `timeline`, `table`, `split_view` and `split_view_vertical` (a `Splitter` node that a drag or the
  arrow keys move, along its parent), `select` (button and menu), `combobox` (suggestions under the field, so it keeps the focus),
  `tree_item`, `image` (RGBA, `include_bytes!` or flash; shades in a text UI), line, area and bar **charts** (`Kind::Chart`, up to 32
  samples), a month **calendar** and a **time picker** with the date arithmetic (`weekday`, `days_in_month`, `shift_month`,
  `step_time`)
- [x] **Text areas** (`text_area`, `TextField::rows(n)`): Enter breaks the line, Up and Down move between lines, a press picks the
  line, Home and End go to the ends of the line; no wrapping, and the text is the field's 64 bytes (250 with `long-text`)
- [x] **System:** text scaling (`Theme::with_text_scale`, the person's font size: text and controls grow, nothing else),
  **drag and drop** (`set_draggable`, `set_drop_target`, `DragStarted`/`Dropped`/`DragCancelled`, a ghost and the target outlined, the
  `draggable` and `drop_target` views)
- [ ] **Still to do:** bidirectional text and shaping of right-to-left scripts (the renderer draws left to right), soft wrapping in a
  text area and a longer text for it, a combobox that filters by itself, dragging with the keyboard, a floating (popover) combobox
  menu that does not take the focus, image scaling, line charts with more than 32 points or a time axis, a date range picker,
  spinner and gauge animation on the text UI beyond one cell
- [ ] A `Widget` trait for custom widgets behind an `alloc` feature
- [ ] Port the Hibari OS launcher to Fusuma widgets

## View layer

The Elm architecture on the retained tree (design in Decisions). Gates for every step: zero allocations in a running
session, a node at most 184 bytes (aim 128), a view pass under 1 ms on the S3, a small update still 12 ms on the watch,
the same pixels as the hand-built page, a message log that replays to the same state.

- [x] **The Elm layer** (now in `fusuma` itself, `fusuma::Program`, `fusuma::button`; the toolkit is `fusuma::ui`): `update` is the only place state changes, `view(&State)` describes the screen, and the layer makes
  the tree match it. Views are values made on the stack and thrown away (the tree is the previous state: setters compare
  first, so only what differs asks for layout or drawing). Children are tuples, `Option` (the page's shape) and `show`
  (hidden nodes, state kept); `named` gives a node an app id; messages are `Copy` values in a fixed table next to the node
  that sends them (no closures, no id paths). Views for every widget: text, buttons, checkbox, radio, switch, slider,
  text field (controlled), list rows, tabs, paragraph, scroll, dialog, menu, toast, gestures, card, badge, avatar,
  stepper, divider. Builds `no_std` for RISC-V and Cortex-M0
- [x] **Every showcase page is a view** (`examples/showcase/src/pages.rs`): ten pages, one `PageMsg`, one `Host` (ten pages then, thirteen now); the
  45 screenshots (desktop dark and light, phone, watch, dialogs) are **pixel-identical** to the hand-built pages, the 46
  showcase tests pass, a running showcase makes **0 allocations** on every page. On the S3 watch a pass of a page's view
  takes **129 to 312 us** (22 nodes 135 us, 32 nodes 205 to 278 us, 40 nodes about 190 us; about 6 to 9 us a node, nothing
  changed or one value changed alike), a small update stays 11 ms and a scroll step 36 ms
- [x] **`Program` and `Runner`** (an Elm program is an `App`, so it runs wherever the showcase does): `update -> Task`,
  `view(&State, &Screen)`, `subscription`, `presets`. **`Task`** is a closed, fixed-size value (send, after, focus,
  scroll, toast, hide); **`Sub`** is at most four recipes (frames, every) diffed by id after every `update`, so a recipe
  that is no longer wanted stops by itself; timers and recipes give a `deadline`, so the runner sleeps (`Wake`) between
  them. Tested: a timer subscription, a toast task, a delayed message, a frame subscription that stops when the
  state says so, 0 allocations
- [x] **Message log and replay**: the runner keeps the last `L` messages with their times; `Runner::replay` runs them through
  `update` from a state with the tasks dropped (what a task caused arrived as messages of its own) and reaches the
  same state; a runner started from the replayed state shows the same page
- [x] **Keyed lists** (`each`: rows that stay keep their node, text and focus; new ones are made, gone ones removed, moved
  ones moved: a 200-step random test against the list), **`memo`** (a 16-bit version of the data; a skipped subtree keeps
  its messages), **`imperative`** (hand-built subtrees in a view, so pages can move one by one)
- [x] **Nodes on springs** (`animate_to`, `set_offset`, the `animate` view modifier): the node owns the spring, so the
  view runs once for a new target, not once a frame; a new target in flight keeps its momentum; every frame equals a
  full redraw
- [x] **`fusuma-test`**: find a widget by text, app id or role; click (pointer events) or activate (focus and Enter, as an
  assistive tool does) through the semantic tree; type; let time pass; read the state and the messages; PNG screenshots;
  run and record scripts (`click Save`, `type abc`, `wait 500`, `expect Saved`, `shot name`) with the failing line named
- [x] **Style classes** (`set_class`, `class(..)` on a view): a number every theme hook receives (`button_in`, `item_in`,
  `tab_in`, `panel_in`, `surface_in`), so a design system restyles a widget by name
- [x] **Palettes from a seed** (`fusuma-palette`, std): OKLCH, neutrals tinted by the seed's hue, the main action the seed
  at the lightness that stands out, every text colour searched until it is readable (body 7:1, muted 4.5:1, text on the
  main action 4.5:1 at rest, hover and pressed); checked over 72 seeds and the greys; `cargo run -p fusuma-palette
  --example gen -- NAME R G B` prints the Rust of a palette. The shipped palettes are still hand tuned tables
- [x] **Wake**, **hidden nodes**, **app ids**, **baselines** (0 allocations, node budget): see Decisions and the
  skill's pitfalls
- [ ] The ESP loop still sleeps a fixed 8 ms: touch is polled, so `Wake` saves nothing until the panel's interrupt line
  wakes the loop (the GT911 and FT3168 have one)
- [ ] The showcase itself is not a `Program` (its header, tiles and dialog are imperative around the page views); a
  `Program`-based showcase would use `Runner` and the simulator end to end
- [ ] Node at 128 bytes: needs the text out of the node (a shared arena with compaction); about 7 KB on a 112-node tree (the showcase has room for 256 nodes now: roughly double, not measured)
- [ ] A flash-per-added-page report, and a `riscv32imc` size report (no C3 board here: Tier 0 is proven by the allocation
  test and the `no_std` builds until there is one)
- [x] **Scoped effects as subscriptions** (they stop when the state stops wanting them, so nothing is ever cancelled by hand): `Sub::once`
  (an auto-dismiss: fires once after its time, silent while it stays wanted, a new start when it comes back, a deadline so the loop
  can sleep), `Sub::keys` (a shortcut: a `Some` is consumed, a `None` goes to the widgets) and `Sub::resizes`. `on_screen` covers the
  changes a layout cares about
- [ ] `Task::perform` with futures as an optional adapter for desktop and web
- [ ] The simulator on the web build (the same scripts in the browser)

## Animation

- [x] `fusuma-anim`: `Fx` (Q16.16), easing (incl. CSS Bezier), `Tween` with delay, stagger and retarget, `Keyframes`,
  `Spring` (stiffness and damping, or response and damping ratio), `Decay`, `Animator<N>`, `rubber_band`,
  `snap_page`, six spring tokens and a composable `Clip` with presets; builds for RISC-V, ARM and Xtensa; Hibari uses it
- [ ] More physics: gravity and bounce, a spring chain, velocity from touch history
- [ ] Shared and layout animations: a card growing into a page, list reorder and insert
- [ ] Vector animation: a state machine over timelines, playing Lottie (`velato`) and SVG (`vello_svg`)

## Effects and shaders

- [x] **Gradients and shadows** (`fusuma-render::effects`): `Renderer::fill_gradient(rect, radius, Gradient)` (vertical or horizontal, two
  colours with alpha, round corners) and `Renderer::draw_shadow(rect, radius, Shadow { offset, blur, spread, color })`. **Every backend
  has them**, because the trait defaults use rectangles only (a gradient is up to 256 strips with the corners cut by the circle, a
  shadow is up to 12 stacked translucent rounded rectangles, and none where colours with alpha are not blended); Vello overrides the
  gradient with a real one and says so in `Capabilities::effects`. Both are in the shared conformance suite (the ends are the
  colours, it only gets lighter, nothing outside the box, corners cut, clipped, the shadow fades out and is shifted by its offset).
  The toolkit uses the shadow: `Theme::elevation` (8 px; 0 on a watch's showcase and on a reflective glass) and `PanelStyle::shadow` give
  dialogs, menus and toasts a soft lift, and a toast's damage covers it (the page is exactly back after it goes)
- [ ] Opacity and blend **layers** (a group faded as one), blur of what is behind (backdrop), masks, radial and multi-stop gradients, and
  a gradient surface in the theme (`Surface::Gradient`); these need a backend with real layers (Vello has them, the integer renderer
  does not)
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

### Mobile

- [x] **Scaffolds** (`fusuma-platform-mobile` and its `android`, `ios` and `linux` features): the system-specific values translated and tested on
  the desktop (a random run of 200 lifecycle sequences never draws without a surface or in the background; mutated, 3 of the
  checks fail), `no_std` builds for RISC-V and Cortex-M0
- [ ] **Android:** an activity (`android-activity` through `winit`), Vulkan by `wgpu` with Vello CPU as the fallback, the IME
  for text and composition, `WindowInsets` into `Display`, a Gradle module (or Fusuma as a surface inside
  `android-app-framework`), an emulator in CI
- [ ] **iOS:** a `UIView` with a `CAMetalLayer`, `UIKeyInput`/`UITextInput`, safe-area insets, display-link pacing; needs a Mac
  runner for CI (none)
- [ ] **Linux phones:** run the desktop host on postmarketOS (it should already work on Wayland), then the shell's keyboard
  and output rotation from this crate
- [x] **A phone is a phone, not a watch:** `Display` carries the glass size and `Display::screen()` gives the `Screen` (see
  [Screens](#screens-one-description-on-every-platform)); the hosts' `display(...)` fill it
- [ ] A `mobile` feature on the `fusuma` crate (`fusuma = { features = ["android"] }` giving `fusuma::mobile`): tried, and `test_project.nu` refuses
  it, rightly: a published crate may not depend, even optionally, on an unpublished one. It comes with publishing `fusuma-platform-mobile`
  (the roadmap's condition: a host that runs an app). Until then an app depends on `fusuma-platform-mobile` itself. Lifecycle on the web (a
  page going to the background) through the same `Session`

### Screens: one description on every platform

A `Screen` is the one value that describes the device, and every host fills it in the same way. Hosts and board files report
**facts once**; an app never configures display hardware, it reads the result (`form_factor()`, the theme) and the toolkit
derives the rest.

- [x] **Form factor:** `FormFactor` = `Micro`, `Panel`, `Watch`, `Phone`, `Tablet`, `Desktop`. Decided by what the host
  declares (`form` in a board's `[screen]`), then the glass (`with_size_mm`: short side under 55 mm a watch, under 85 a
  phone), then the width (under `MICRO_BELOW` = 260 logical px a `Micro`). Small boards are declared: a 1.28 inch round board
  and a watch have the same glass. `class()` respects it, so a 360 dp phone with its size known is a phone, not a watch
- [x] **Display kind:** `DisplayKind` (`Emissive`, `Reflective`), depth in bits and refresh time (`with_glass`). A reflective
  glass turns motion off (`Screen::theme`, the showcase follows) and, below 4 bits, takes `Palette::PAPER` (`Screen::palette`)
- [x] **Inputs** (`Inputs`: touch, keys, encoder, pointer), `SizeClass` (compact, medium, expanded, follows the window) and
  `Metrics` (touch target, gap, columns per form factor, scaled): reported and tested, **no layout reads them yet**
- [x] **Board `[screen]`:** `preset = "..."` (`SCREEN_PRESETS`: the S3 watch, round 1.28, OLED 0.96, LCD 1.14, e-paper 1.54,
  2.13, 2.9, 3.97, 4.2, 7.5, the P4 7 inch) and any key to override; the ESP build emits `SCREEN`, and the driver sends
  `Input::Screen`, which reaches an app as `App::screen`. `Board::screen()` has a default
- [x] **Hosts report the glass:** ESP `board.toml`; `fusuma_platform_mobile::android::display` (`DisplayMetrics`),
  `fusuma_platform_mobile::ios::display` (points and scale, the model's size), `fusuma_platform_mobile::linux::display`
  (`wl_output` mm, rotation swaps the sizes)
- [x] **Desktop and web report what they know**: a desktop window declares itself a `Desktop` with pointer and keys (touch is
  added at the first finger), so a window dragged narrow gets a phone's layout, never a watch's tiles; a page reports pointer, keys
  and touch (`navigator.maxTouchPoints`) and its pixel ratio. Neither knows the glass in millimetres (winit and CSS have no such
  value). **`Program::on_screen`** turns a change a layout cares about (form factor, class, size class, orientation, reflective) into a
  message; not every pixel of a resize
- [x] **A layout for tiny panels in the showcase** (`Showcase::micro()`: one tile to a row, no palette switcher, short words, buttons
  stacked, small margins; looked at at 128x296 and 200x200). [ ] Still: one that reads `Inputs` and `Metrics`

### E-paper (reflective panels)

The app draws as always, on the integer renderer or Vello CPU. `fusuma-platform-eink` (over `fusuma-platform-embedded`) does the
rest: `EinkDriver` tells the app what the glass is, quantises the RGB565 picture to one bit (`pack_mono`: threshold or 4x4
ordered dither, a set bit is white), batches damage with a `Planner` (settle time, minimum interval, one union rectangle in
whole bytes, a full flashing refresh every N partials and after sleep) and sends it through a `Panel` over a small `Bus`
trait. Controllers: `Ssd1680` (Waveshare 1.54, 2.13, 2.9 inch), `Ssd1677` and `Ssd2677` (800x480, the two panels the Sticky
has shipped with, told apart by register 0x70 in `Sticky::detect`).

- [x] Quantising, `Planner`, `EinkDriver`, `Panel`, the three controllers over a recording bus; the showcase on both
  renderers, packed frames looked at as images. **Nothing has run on a panel.**
- [x] **Seeed reTerminal Sticky** (3.97 inch, 800x480, ESP32-S3R8, `s3-sticky`): pins from Seeed's ESPHome model, `[power]` and
  `[buttons]` in the board file, `epaper_spi.c` in `fusuma_esp`, the e-paper run loop in `fusuma-platform-esp`, GT911 touch.
  Builds as firmware next to the watch's. The SSD2677 has no published partial refresh: every refresh is a full one
- [ ] **On a Sticky:** which panel it has, the SSD2677's mirror and scan direction, the touch swap, real refresh times, deep
  sleep and the OK-key wake, battery gauge and RTC, the partition table. **Before flashing, back the board up:** `just showcase-esp-backup`
  reads the whole flash to `~/.fusuma/backups` with a checksum and `just showcase-esp-restore-flash` writes it back (`install --backup` does it first)
- [ ] On a Waveshare SSD1680 module (sequences follow Waveshare's V2/V4 drivers): `board.toml` `controller = "ssd1680"`
- [x] **UC8179** (Waveshare's 7.5 inch `epd7in5_V2`, 800x480, black and white): `Uc8179` follows that driver's own sequences (read from its
  source: init, `init_part`, the old and inverted new data, the partial window, sleep); the busy line is **low** while it works, so
  `Bus::wait_busy_low` and the C side's `active_low` exist; `controller = "uc8179"` in a board file (a built-in preset for the panel is
  `epaper-7in5`; no board file for a Waveshare ESP32 driver board yet, its pins are not known). Tested against a recording bus and as the
  showcase on a 800x480 one-bit canvas; **not run on a panel**
- [ ] The 4.2 inch UC8176 (it needs a look-up table of waveforms), four grays (a two-bit picture, which the pipeline does not make yet:
  the UC8179's four-gray mode is `init_4Gray` and two planes, ready to follow it), tri-colour and Spectra 6 panels, landscape on a
  portrait panel
- [x] **A one-bit framebuffer** (`MonoFrame` in `fusuma-render-integer`: the integer renderer draws on it, eight pixels a byte, a set
  bit is white, colours thresholded or ordered-dithered as they are drawn, scroll by whole bytes; `EinkDriver::new_mono`; the ESP
  loop uses it): 48 KB for 800x480 where RGB565 is 768 KB, nothing to pack; a partial refresh is checked to be exactly the change
  of the whole picture. Threshold 170 by default (a lower level loses the thin strokes of smooth text)
- [x] **Fixed (was not e-paper):** a paint nobody can see (a focus ring on a widget past the edge) left its request set, so the
  app asked for a frame for ever and drew none; `paint_frame` now answers it (`a_paint_nobody_can_see_is_answered...`, red without the fix)

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
