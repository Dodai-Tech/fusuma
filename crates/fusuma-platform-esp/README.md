# fusuma-platform-esp

Runs a Fusuma `App` on an ESP-IDF board: brings up the display and the touch panel, feeds touch to the app,
ticks it, has it drawn and puts only what changed on the screen. **The app never names a renderer.** At start
it reads what the chip has (internal RAM, PSRAM, FPU), asks `fusuma-platform` which backend suits it, and logs
which one it chose and why:

```
renderer: integer (automatic: Vello has no SIMD on this chip, so the integer renderer is several times faster)
```

```rust
fusuma_platform_esp::run(|w, h| MyApp::new(w, h), fusuma_platform_esp::Options::default())
```

Overrides, strongest first: `Options::build` (a cargo feature or the program), the project's `fusuma.toml`
(`Options::settings`, embedded with `include_str!`), then the automatic rule; see `fusuma-platform` for the keys.
With `fallback` (on by default) an explicit choice that does not fit in memory steps down to a lighter one. Cargo features
`vello-cpu` and `fonts` (off by default) link Vello and its fonts next to the integer one, for a board that pins Vello; leave them off for the
smallest firmware, and the choice is made among what is linked.

## Boards are files, drivers are per controller

A board is a directory under `boards/` with its **board file** (`board.toml`, the format of
`fusuma_platform::board`, schema in `crates/fusuma-platform/board.schema.json`), its `sdkconfig.defaults.in` and its
`partitions.csv`. The build reads the board named by the `FUSUMA_BOARD` environment variable (`scripts/showcase.nu`
sets it), checks it, compiles in the drivers its controllers need and hands them the pins and numbers. A board made of
known controllers is one file; a mistake stops the build with the file and the key (`display.pins.cs: the co5300
needs it`). Which boards exist, which are a file away and which need a driver: [docs/BOARDS.md](../../docs/BOARDS.md).

| Controller | Part | Bus | Notes |
|---|---|---|---|
| `co5300` | display | QSPI | AMOLED without a frame buffer: updates go out in bands of rows through two DMA buffers, one filled while the other is sent; `grid = 2` |
| `ek79007` | display | MIPI DSI | ESP32-P4 only, 1024x600; a frame buffer the DMA scans out |
| `ft3168` | touch | I2C | |
| `gt911` | touch | I2C | address found at 0x5D or 0x14 |
| `ledc` | backlight | PWM | |

The C drivers are one ESP-IDF component, `components/fusuma_esp`, which the firmware lists in its
`package.metadata.esp-idf-sys.extra_components`; the linker keeps the ones the board uses. All I2C goes through the
new `i2c_master` driver: ESP-IDF aborts at boot when the legacy driver is in the same image. Touch is turned to the
screen from the file's `mirror` and `swap_xy`, and a held finger survives a few failed reads (the FT3168 answers NAK
now and then).

| Board | Parts |
|---|---|
| `p4-7in`: Waveshare ESP32-P4 7" touch board | `ek79007`, `gt911`, `ledc` |
| `c6-epaper-154`: Waveshare ESP32-C6-ePaper-1.54 (200x200 e-paper, TCA9554 rail switch) | `ssd1680`, `[power.i2c]`; from vendor firmware, built, not run; see docs/EINK.md |
| `s3-epaper-397`: Waveshare ESP32-S3-ePaper-3.97 (800x480 e-paper, AXP2101) | `ssd1677`, `[power.i2c]`; from vendor firmware, built, not run; see docs/EINK.md |
| `s3-epaper-154`: Waveshare ESP32-S3-ePaper-1.54 (200x200 e-paper) | `ssd1680`; from vendor firmware, built, not run; see docs/EINK.md |
| `s3-amoled-143`: Waveshare ESP32-S3-Touch-AMOLED-1.43 (466x466 round, CO5300 variant) | `co5300`, `ft3168`; read from the vendor's firmware, not run on a board; see docs/BOARDS.md |
| `s3-amoled-164`: Waveshare ESP32-S3-Touch-AMOLED-1.64 (280x456) | `co5300`, `ft3168`; pins verified on a real board, CS is GPIO46 on this V2 board (9 on a V1) |
| `s3-amoled-206`: Waveshare ESP32-S3-Touch-AMOLED-2.06 watch | `co5300`, `ft3168`; its 70 px corner radius is a guess by eye |

A choice is predicted from the memory the chip reports; allocation failure inside a renderer cannot be caught,
so the check has to be right before the surface is made.

## Bringing up a new board: what the 1.64" taught

`s3-amoled-164` was found on a real board (a Waveshare ESP32-S3-Touch-AMOLED-1.64, 280x456, sold with a board sticker
that did not say which PCB). Checklist, in the order that saved time:

1. **Scan I2C before trusting a pinout.** Pairs of GPIOs, addresses 0x08..0x77: here only SDA 47 / SCL 48 answered
   (0x38 the FT3168, 0x6b the IMU), which ruled out the 1.8" board's SDA 15 / SCL 14 and its TCA9554 expander.
2. **Waveshare calls the panel an SH8601; the init it runs is the one this crate's `co5300` driver sends** (`C4 80`, `3A 55`,
   `35`, `53 20`, `63 FF`, `51`, `29`; control opcode 0x02, pixel opcode 0x32, the register as `reg << 8`). The first dark
   screen was blamed on the controller; it was the chip select (next point). An init that returns OK says nothing: only the glass does.
3. **Chip select differs between PCB revisions** (GPIO9 on V1, GPIO46 on V2; Waveshare's `ESP32-S3-Touch-AMOLED-1.64-v2`
   firmware uses 46). On the board here GPIO9 alone showed nothing and both together lit the panel, so it is a V2: the file says `cs = 46` (a V1 board needs 9). Open: a `cs`
   list or a probe, so one image fits both. Not yet run through this crate's driver on the glass.
4. **Start the clock low** (10 MHz) and raise it in steps; the first try at 40 MHz showed nothing.
5. **The touch controller needs its reset pulsed** (`rst` under `[touch]`, GPIO8): without it every read is empty and a
   tap never arrives. Touch reads fine with I2C acks and still reports nothing.
6. **Turn on the PSRAM** (octal, 8 MB). Without it the first real frame of text and charts aborts on a failed allocation.
7. **Read the vendor partition table before choosing a slot** (`espflash read-flash 0x8000 0xc00`). This board has one
   6 MB `factory` slot and no OTA: `showcase install` picks `factory` and refuses without `--backup` or `--force`.

## Memory and text

Vector text (the default `fonts` feature: Inter, Inter Bold, JetBrains Mono, about 215 KB of flash) needs two things
from the firmware:

- **A stack of its own.** Parley and glyph drawing are deep calls, and the main task's 32 KB overflowed. `run` runs the
  app on a thread with `Options::stack_kb` (128 by default).
- **Allocations in PSRAM.** Set `CONFIG_SPIRAM_MALLOC_ALWAYSINTERNAL=512` in the firmware's sdkconfig (see
  `boards/p4-7in/sdkconfig.defaults.in`). At the default of 16384 almost everything Parley allocates lands in
  internal RAM: measured on the P4, the first frame left **9 KB** of internal RAM free (1 KB at the lowest); at 512 it
  leaves **234 KB** (190 KB steady, 157 KB at the lowest), more than the bitmap text did (116 KB, 86 KB lowest).

The log line `alive: ... internal RAM free ... (lowest ever ...)` is the number to set `min_free_kb` from.

## Where the shared code is

The frame loop, the renderer surface and the touch handling are `fusuma-platform-embedded`, shared by every board. This
crate is the Waveshare P4 as a `Board` (`hw.rs`: the DSI frame buffer and the GT911), the ESP-IDF thread and heap
probe, and the settings. A new board is a new `Board` implementation, not a new loop.

## What this crate does not contain

Components, layout, theme, focus, scrolling, the frame clock and the turning of touch samples into pointer events
are in `fusuma-ui` (`Shell`, `TouchTracker`, `Input`), shared by every platform. This crate has only what is
specific to a board: the DSI display and the GT911 (`hw.rs`), converting the renderer's pixels to RGB565, and the
loop that ties them together. CI fails if a `fusuma-platform-*` crate builds a widget tree or implements a renderer.
