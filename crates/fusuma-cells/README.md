# fusuma-cells

Fusuma's text-cell output, for a terminal, an SSH session, the serial console of a chip or a pixel panel showing a
grid of characters. `no_std`, **no allocator** and no dependencies, so it runs on a Cortex-M0 as well as a desktop.

- **`Cell`, `Style`, `Color`, `Attrs`**: a character with a foreground, a background and bold, dim, italic, underline
  or reverse.
- **`Depth`**: how many colours an output shows (truecolor, 256, 16 or none). A colour is shown at the nearest the
  output has; on a monochrome one only the attributes remain.
- **`Grid`**: `width x height` cells over any storage, a `[Cell; N]` on a chip or a `Vec<Cell>` on a desktop. Writing
  outside it is cut off, never a panic.
- **`changes(old, new)`**: the runs of cells that differ between two frames, row by row, with short unchanged gaps
  merged in (a cursor move costs more bytes than a few cells).
- **`Encoder`**: writes runs as ANSI escape sequences to any `core::fmt::Write`: a cursor move only where needed, a
  style change only when the style changes, control characters never.
- **`Decoder`**: turns the bytes a terminal sends (keys, arrows, escape sequences, UTF-8) into `CellKey`s, one byte at
  a time, for a serial console. A lone Escape is told from the start of a sequence by `flush` when input pauses.

A character takes the columns it has: one, or two for a wide one (CJK, emoji; a tail cell follows it and writes
nothing), none for a combining mark (`char_width`, `str_width`). The decoder also reads Alt and a key, and mouse
reports (SGR).

This is the bottom of Fusuma's text UI. The toolkit draws into it with `Tree::paint_cells` (`fusuma-ui`); the outputs are
`fusuma-platform-tui` (a terminal), `fusuma-platform-console` (a serial console) and `fusuma-cells-panel` (a pixel
panel).
