# fusuma-cells-panel

Fusuma's text UI on a pixel panel: a `fusuma_cells::Grid` drawn through any `fusuma_render::Renderer`, the integer one
on a microcontroller included, so a chip with a display shows the same character UI as a terminal or a serial console.
`no_std`, **no allocator**. It replaces what `mousefood` does for `ratatui`, for Fusuma's own cells.

- **`CellPanel`** knows the size of a cell (the bitmap font's 6x10, 9x15 or 10x20) and where the grid sits on the
  screen. `draw` paints a whole grid; `draw_changes(renderer, old, new)` paints only the runs of cells that changed
  and returns the **one rectangle** that covers them, for a board to send in a single transfer.
- A cell's colours come from its style; bold draws the glyph twice, dim mixes it into the background, reverse swaps,
  underline draws a line.
- The characters a text UI draws widgets with that the bitmap font lacks (blocks, box drawing, bullets, triangles) are
  drawn as shapes, so `CellTheme::UNICODE` looks right on a panel with only ASCII glyphs.
