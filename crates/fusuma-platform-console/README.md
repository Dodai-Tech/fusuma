# fusuma-platform-console

Fusuma over a serial console: a UART, a USB serial port, anything that takes bytes out and gives bytes in, and a
terminal program (or `picocom`, `screen`, a browser's xterm.js) on the other end. `no_std`, **no allocator**.

- **`Console<W, S>`** writes frames of cells (`fusuma_cells::Grid`) to any `core::fmt::Write`, sending only the
  cells that changed since the last frame (`present`); `start` and `stop` bring the terminal's alternate screen,
  cursor and mouse reporting up and back.
- **`Console::feed(byte, wants_text)`** turns the bytes the terminal sends (keys, arrows, Alt, mouse reports, UTF-8)
  into `fusuma::Input`, one byte at a time; `flush` tells a lone Escape from the start of a sequence when input
  pauses.

The toolkit draws into the grid with `Tree::paint_cells` and lays out in cells (`Theme::CELLS`, `CellMeasure`); this
crate only moves bytes. On an ESP32 the writer is the USB serial port or UART0 (not the log's: send the log
elsewhere, or the two mix).
