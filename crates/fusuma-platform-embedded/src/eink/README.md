# E-paper (`fusuma_platform_embedded::eink`)

Fusuma on e-paper (and other reflective panels: memory LCDs): black and white, slow to refresh, no animation. `no_std` with
an allocator, tested on the computer against a recording bus; **not yet run on a panel** (see the roadmap).

The app draws as always, on the integer renderer or Vello on the CPU. This crate does the rest:

- **`EinkDriver`**: one `step(now_ms)` runs a frame, notes what changed and refreshes the panel when the policy allows. It
  tells the app what the glass is (`fusuma::Input::Screen`), so the theme turns to black on white (`Palette::PAPER`) and the
  motion stops without the app asking.
- **`pack_mono`, `Dither`**: the RGB565 picture to one bit a pixel (a threshold, or a 4x4 ordered dither), eight pixels a
  byte, a set bit white, in the byte-aligned region controllers address.
- **`Planner`, `RefreshPolicy`**: damage from many frames becomes one rectangle, shown after the screen has been quiet for a
  settle time and no sooner than a minimum interval, with a full flashing refresh now and then against ghosting.
- **`Panel`**: what a controller offers (`full`, `partial`, `sleep`, `screen`). Implemented by
  - `ssd1680::Ssd1680`: Waveshare's 1.54, 2.13 and 2.9 inch modules,
  - `ssd1677::Ssd1677`: 800x480 panels, with the mirrored wiring of the reTerminal Sticky,
  - `ssd2677::Ssd2677`: the newer Sticky panel, full refresh only, waveform from the temperature,
  - `uc8179::Uc8179`: Waveshare's 7.5 inch (800x480), partial refresh, a busy line that is low while it works,
  - `sticky::Sticky::detect`: asks the chip which of the two is in a Sticky (register 0x70) and gives that one.
- **`bus::Bus`**: what a controller needs from the chip (command, data, reset, busy, an optional read-back and delay), so a
  runtime (`fusuma-platform-esp` over SPI) implements four functions. `bus::Recorder` is the test bus.

It draws through the crate's `Surface` and `Pixels`; the emissive `Driver` and this module are two loops over one surface.
