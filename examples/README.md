# Fusuma examples

**`showcase/`** is a small app that uses every Fusuma widget: buttons, checkboxes, radio buttons, a switch, a slider, a list,
a progress bar, keyboard focus and a dark/light theme switch. It is `no_std` and draws on any renderer, so
the same code is tested on the host (`just showcase-shots` writes a PNG of every page to
`docs/screenshots/`) and runs on a board.

**`showcase-esp/`** is that app on ESP32 boards, on ESP-IDF, one build per board. A board is a board file and its
sdkconfig and partition table in `crates/fusuma-platform-esp/boards/<name>/`; the chip gives the Rust target and
toolchain:

| Board | Chip | Screen | Toolchain |
|---|---|---|---|
| `p4-7in`: Waveshare ESP32-P4 7" touch board | ESP32-P4 | 1024x600 DSI with a frame buffer, GT911 touch | `nightly-2026-09-15` |
| `s3-amoled-206`: Waveshare ESP32-S3-Touch-AMOLED-2.06 watch | ESP32-S3 | 410x502 CO5300 AMOLED over QSPI, FT3168 touch | `esp` (espup) |

`just showcase-esp` asks the connected chip what it is (`espflash board-info`), picks the board for it from the board
files, builds and flashes the `ota_0` slot; the factory app stays. Two boards with the same chip are told apart by flash
size, or by `board=`. A new board made of known controllers is a new directory, no code. It is not a workspace member (it
needs the ESP toolchain).

```
just showcase-esp /dev/ttyACM0                  # detect, build, flash
just showcase-esp /dev/ttyACM0 render-integer   # ask for a renderer
just showcase-esp-log                           # read the board's log without a reset
just showcase-esp-restore /dev/ttyACM0          # boot the factory app again
```

Measured on the P4: the first frame (1024x600) takes about 210 ms (76 ms drawing), and a small update
such as a hovered button about 60 ms, most of it converting the pixels and clearing the surface, not
drawing. Text is Inter, Inter Bold and JetBrains Mono through `fusuma-text`, with the built-in bitmap font as the
fallback (the Typography page shows all three).

**`showcase-desktop/`** is the same app in a window on a computer: `just showcase-desktop` (or `just showcase-desktop
--renderer vello-cpu` to force the CPU). It uses `fusuma-platform-desktop`, which picks Vello on the GPU when there is one.

Measured on the S3 watch with this firmware (ESP-IDF, 240 MHz): the first full frame takes **558 ms** with Vello and the
Inter fonts, of which 49 ms is turning the picture into the panel's RGB565; the panel transfer hides behind the
conversion but 2 ms. On a screen under 640 logical pixels wide the app switches to a compact layout.

The watch's earlier firmware, `tools/fusuma-showcase-s3` in the Hibari repository (on `esp-hal`, no ESP-IDF, the CPU at
160 MHz), measured 196 ms with the integer renderer and bitmap text, 523 ms with Vello and bitmap text, and 948 ms with
Vello and Inter. Those are not comparable one to one with the numbers above (another runtime and
clock).
