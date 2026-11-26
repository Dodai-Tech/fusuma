# fusuma-render-integer

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The Fusuma renderer backend for small displays. It draws with integers only, in `no_std` and without an
allocator, on any [`embedded-graphics`](https://crates.io/crates/embedded-graphics) `DrawTarget`
with `Rgb565` pixels: an SPI or QSPI display driver, a framebuffer in PSRAM, or the `Frame` type
of this crate.

- Rounded rectangles, circles and rounded borders are drawn from their coverage in integer maths, with smooth
  edges; plain rectangles, strokes along paths and images map to `embedded-graphics` primitives (hard edges, for
  now). Everything is clipped to the current clip.
- Paths (lines, quadratic and cubic curves) are flattened and filled with an integer even-odd
  scanline fill. A path may have up to 160 edges; a bigger one is an `Error::TooComplex`.
- Text: with the `fonts` feature, the families `fusuma-fonts` links (and its icons) from glyph atlases made at build
  time, blended in; otherwise, and for the bitmap font, the built-in mono fonts (6x10, 9x15, 10x20, ASCII).
- Fills and text blend with their alpha on a target that can (`Blend`; the `Frame` of this crate does), and
  `Capabilities` then report alpha and anti-aliasing. A target that cannot blend gets the pixels at least half
  covered, solid.

It passes the shared `fusuma_render::conformance` suite.

**One bit.** `MonoFrame` is a black and white framebuffer (eight pixels a byte, a set bit white) the same renderer draws on, for e-paper and
memory LCDs: 48 KB for an 800x480 picture where RGB565 is 768 KB. Colours become black or white as they are drawn, by a threshold
(crisp) or an ordered dither (gray as dots); smooth edges are mixed with what is there first, then turned.
