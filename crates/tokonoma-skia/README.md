# tokonoma-skia

The Tokonoma backend for desktops, phones, tablets and watches: anti-aliased CPU drawing on
[`tiny-skia`](https://crates.io/crates/tiny-skia). It owns an RGBA `Pixmap`, which you hand to a
window system (`winit` and `softbuffer`, a Wayland buffer, a texture) or write out as a PNG.

- Rectangles, rounded rectangles, circles and paths are anti-aliased. Colors blend with their alpha.
- Clip and translate map to a mask and a transform.
- Text uses the same mono fonts as `tokonoma-embedded`, so glyphs are identical on every target
  (anti-aliased vector fonts are planned).
- Strokes run through pixel centers, so a 3 px line at `y = 10` covers rows 9 to 11.

It passes the shared `tokonoma::conformance` suite, and a test draws one scene on this backend and
on `tokonoma-embedded` and compares the pixels.
