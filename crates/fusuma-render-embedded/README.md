# fusuma-render-embedded

[![Crates.io](https://img.shields.io/crates/v/fusuma-render-embedded.svg)](https://crates.io/crates/fusuma-render-embedded)
[![docs.rs](https://img.shields.io/docsrs/fusuma-render-embedded)](https://docs.rs/fusuma-render-embedded)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

The Fusuma renderer backend for small displays. It draws with integers only, in `no_std` and without an
allocator, on any [`embedded-graphics`](https://crates.io/crates/embedded-graphics) `DrawTarget`
with `Rgb565` pixels: an SPI or QSPI display driver, a framebuffer in PSRAM, or the `Frame` type
of this crate.

- Rectangles, rounded rectangles, circles, strokes and images map to `embedded-graphics`
  primitives, clipped to the current clip.
- Paths (lines, quadratic and cubic curves) are flattened and filled with an integer even-odd
  scanline fill. A path may have up to 160 edges; a bigger one is an `Error::TooComplex`.
- Text uses the built-in mono fonts (6x10, 9x15, 10x20), picked by the requested size.
- There is no alpha blending and no anti-aliasing (`Capabilities` say so): a color is drawn
  opaque unless it is fully transparent, which draws nothing.

It passes the shared `fusuma_render::conformance` suite.
