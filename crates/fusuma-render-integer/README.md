# fusuma-render-integer

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The Fusuma renderer backend for small displays. It draws with integers only, in `no_std` and without an
allocator, on any [`embedded-graphics`](https://crates.io/crates/embedded-graphics) `DrawTarget`
with `Rgb565` pixels: an SPI or QSPI display driver, a framebuffer in PSRAM, or the `Frame` type
of this crate.

- Rounded rectangles, circles, rounded borders, filled paths and strokes (miter joins to Vello's limit of 4, round caps)
  are drawn from their coverage in integer maths, with smooth edges; images with alpha are mixed pixel by pixel; plain
  rectangles are exact. Everything is clipped to the current clip, and a scrolled area can be moved instead of redrawn
  (`Blend::shift_rows`).
- Paths (lines, quadratic and cubic curves) are flattened and filled by a four-samples-a-row scanline fill in integers,
  with no allocation. A path may have up to 160 edges (about 20 stroked segments); a bigger one is an `Error::TooComplex`
  and draws nothing. Coordinates anywhere in `i32` are fuzzed (with overflow checks on).
- Text: with the `fonts` feature, the families `fusuma-fonts` links (and its icons) from glyph atlases made at build
  time, blended in; otherwise, and for the bitmap font, the built-in mono fonts (6x10, 9x15, 10x20, ASCII).
- Fills and text blend with their alpha on a target that can (`Blend`; the `Frame` of this crate does), and
  `Capabilities` then report alpha and anti-aliasing. A target that cannot blend gets the pixels at least half
  covered, solid.

It passes the shared `fusuma_render::conformance` suite.

**One bit.** `MonoFrame` is a black and white framebuffer (eight pixels a byte, a set bit white) the same renderer draws on, for e-paper and
memory LCDs: 48 KB for an 800x480 picture where RGB565 is 768 KB. Colours become black or white as they are drawn, by a threshold
(crisp) or an ordered dither (gray as dots); smooth edges are mixed with what is there first, then turned.
