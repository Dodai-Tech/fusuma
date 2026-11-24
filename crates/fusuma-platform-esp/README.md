# fusuma-platform-esp

Runs a Fusuma `App` on an ESP-IDF board: brings up the display and the touch panel, feeds touch to the app,
ticks it, has it drawn and puts only what changed on the screen. **The app never names a renderer.** At start
it reads what the chip has (internal RAM, PSRAM, FPU), asks `fusuma-platform` which backend suits it, and logs
which one it chose and why:

```
renderer: vello-cpu (automatic: PSRAM holds a Vello frame twice over)
```

```rust
fusuma_platform_esp::run(|w, h| MyApp::new(w, h), fusuma_platform_esp::Options::default())
```

Overrides, strongest first: `Options::build` (a cargo feature or the program), the project's `fusuma.toml`
(`Options::settings`, embedded with `include_str!`), then the automatic rule; see `fusuma-platform` for the keys.
With `fallback` (on by default) an explicit choice that does not fit in memory steps down to a lighter one. Cargo features
`vello-cpu` and `fonts` (off by default) link Vello and its fonts next to the integer one, for a board that pins Vello; leave them off for the
smallest firmware, and the choice is made among what is linked.

It supports two boards, one per build, chosen by a cargo feature:

| Feature | Board | C shim |
|---|---|---|
| `board-p4-7in` | Waveshare ESP32-P4 7" touch board (1024x600 DSI, GT911) | `components/wt_display` (from the WeatherTrack P4 firmware) |
| `board-s3-amoled-206` | Waveshare ESP32-S3-Touch-AMOLED-2.06 watch (410x502 CO5300 over QSPI, FT3168) | `components/s3_amoled` (from Hibari's Rust drivers) |

The firmware crate lists every shim in its `package.metadata.esp-idf-sys.extra_components`; each one builds empty on
the other chips, see `examples/showcase-esp`. The S3's panel has no frame buffer: an update goes out in bands of even
rows through two DMA buffers, one converted while the other is sent, and the log reports the conversion and the wait
every 10 seconds. Its 70 px corner radius is a guess by eye, not a measurement.

A choice is predicted from the memory the chip reports; allocation failure inside a renderer cannot be caught,
so the check has to be right before the surface is made.

## Memory and text

Vector text (the default `fonts` feature: Inter, Inter Bold, JetBrains Mono, about 215 KB of flash) needs two things
from the firmware:

- **A stack of its own.** Parley and glyph drawing are deep calls, and the main task's 32 KB overflowed. `run` runs the
  app on a thread with `Options::stack_kb` (128 by default).
- **Allocations in PSRAM.** Set `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=512` in the firmware's sdkconfig (see
  `examples/showcase-esp/boards/p4-7in/sdkconfig.defaults.in`). At the default of 16384 almost everything Parley allocates lands in
  internal RAM: measured on the P4, the first frame left **9 KB** of internal RAM free (1 KB at the lowest); at 512 it
  leaves **234 KB** (190 KB steady, 157 KB at the lowest), more than the bitmap text did (116 KB, 86 KB lowest).

The log line `alive: ... internal RAM free ... (lowest ever ...)` is the number to set `min_free_kb` from.

## Where the shared code is

The frame loop, the renderer surface and the touch handling are `fusuma-platform-embedded`, shared by every board. This
crate is the Waveshare P4 as a `Board` (`hw.rs`: the DSI frame buffer and the GT911), the ESP-IDF thread and heap
probe, and the settings. A new board is a new `Board` implementation, not a new loop.

## What this crate does not contain

Components, layout, theme, focus, scrolling, the frame clock and the turning of touch samples into pointer events
are in `fusuma-ui` (`Shell`, `TouchTracker`, `Input`), shared by every platform. This crate has only what is
specific to a board: the DSI display and the GT911 (`hw.rs`), converting the renderer's pixels to RGB565, and the
loop that ties them together. CI fails if a `fusuma-platform-*` crate builds a widget tree or implements a renderer.
