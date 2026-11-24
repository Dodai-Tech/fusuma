# fusuma-platform-embedded

Fusuma on boards without an operating system's windows: the part every chip shares, `no_std` with an allocator.

A chip's runtime brings its hardware up and implements one trait, **`Board`**: the screen size, a touch sample
(`Option<Point>`), and `present(region, pixels)`, which shows a rectangle of the picture, reading its pixels a row at
a time as RGB565. A board with a frame buffer copies the rows in; a panel on an SPI bus sends them as they come, so
no whole-frame buffer is needed on the chip side. Everything else is shared:

- **`Driver`**: one `step(now_ms)` reads touch, turns it into pointer events (`fusuma::TouchTracker`), runs a frame of
  the app (`fusuma::Shell`) and shows only the part that changed. It says whether the app wants another frame, so a
  runtime can sleep. A failing touch panel counts as a lifted finger.
- **`Surface`**: the renderer, whichever was chosen: the integer one always, `tiny-skia` (feature `skia`) and Vello on
  the CPU (`vello-cpu`) by cargo feature. The choice is `fusuma::platform`'s, from the board's memory.
- **`Arch`, `has_fpu()`, `has_simd()`**: what the compiler knows about the target. Memory it cannot know: a runtime
  probes its heap or a board file says. `Chip` and `esp_hardware` are ESP presets (RAM is **measured** for the C6 and
  P4, **estimated** for the rest).

The tests run on a computer with an in-memory board, so the loop, the touch handling and every renderer's output are
checked without hardware. A runtime crate (`fusuma-platform-esp`, a Cortex-M one, a Linux framebuffer one) is then
the board, a clock and a loop.

No widget or drawing code lives here: CI fails if a `fusuma-platform-*` crate builds a widget tree.
