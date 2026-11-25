# fusuma-text

Text for Fusuma: [Parley](https://github.com/linebender/parley) lays text out over the fonts of `fusuma-fonts`, and
the answer (the size, and the glyphs with their positions) is cached, because a UI asks for the same few strings
every frame. `no_std` with an allocator; it uses Parley's `libm` mode, so it builds for ESP-IDF.

```rust,ignore
let mut engine = fusuma_text::TextEngine::new();
let size = engine.measure("Hello", Font::Sans, 20);          // Some(Size) or None
let laid = engine.layout("Hello", Font::SansBold, 20);       // the glyph runs to draw
```

`None` means the font is not available here (its cargo feature did not bundle it): the caller draws the built-in
bitmap font instead. `size` is the line height in pixels, in whatever font, so a layout does not move when a font
is swapped. The fonts are registered from their `'static` data and not copied.

Which fonts exist is decided by `fusuma-fonts`' features (a family or a preset each); which of them draw each role is
a `FontSet` (`set_fonts`), from `[fonts]` in `fusuma.toml`.
