# fusuma-platform-tui

Fusuma in a terminal or an SSH session, on [`crossterm`](https://crates.io/crates/crossterm). It is a platform, not a
renderer: the cells come from `fusuma-cells`, and this crate brings the terminal up, writes frames and turns what the
terminal sends into Fusuma input.

- **`Terminal`**: raw mode, the alternate screen, a hidden cursor and mouse reporting while it lives; all of it is put
  back when it is dropped, a panic included. `present(frame)` writes only the cells that changed since the last frame
  (one write and one flush a frame); a new size repaints everything.
- **`detect_depth`**: the colour depth from the environment (`NO_COLOR`, `COLORTERM`, `TERM`, Windows Terminal), so
  colours degrade to 256, 16 or none on terminals that have fewer.
- **`map_event`**: a key, a click, a wheel turn or a resize as a `fusuma::Input`, the same input a window or a board
  sends. Mouse positions are in cells.

`cargo run -p fusuma-platform-tui --example cells` shows a hand-drawn screen, echoes keys and the mouse, and quits on
`q` or Escape.

**A scaffold.** Not built yet (see `docs/ROADMAP.md`): running a Fusuma app here (`run(make)`, which needs the toolkit
to lay out in cells and paint its widgets into them), the cell theme, pasted text, and a serial-console twin for chips
(which uses `fusuma-cells` without this crate).
