# ESP boards: what Fusuma runs, what is a file away, what needs a driver

Each board file has a `name` (the id `--board` takes) and an optional `title`, the friendly product name shown to people.

A board is a directory in `crates/fusuma-platform-esp/boards/` (see that crate's README). A board made of controllers the
crate already drives is **only a file**; a new controller is also a C driver (`components/fusuma_esp/`), a Rust module
(`src/display/`) and an entry in `CONTROLLERS` (`fusuma-platform/src/board.rs`).

Status words: **run** = a firmware ran on that board and the glass showed it. **file** = a board file read from the vendor's
own firmware, never run. **driver** = needs a controller or an expander Fusuma does not have. Facts below were read from
Waveshare's example repositories (`github.com/waveshareteam/*`) by pin and init tables, not by reading each repo in full.

## In Fusuma today

| Id | Friendly name | Screen | Controllers | Status |
|---|---|---|---|---|
| `p4-7in` | Waveshare ESP32-P4 7-inch Touch Display | 1024x600 | ek79007, gt911 | run |
| `s3-amoled-206` | Waveshare ESP32-S3-Touch-AMOLED-2.06 | 410x502 | co5300, ft3168 | run |
| `s3-sticky` | Seeed reTerminal Sticky | 800x480 e-paper | ssd1677 | run |
| `s3-amoled-164` | Waveshare ESP32-S3-Touch-AMOLED-1.64 (V2) | 280x456 | co5300 init, ft3168 | pins run (weathertrack-esp), not yet through Fusuma's driver |
| `s3-amoled-143` | Waveshare ESP32-S3-Touch-AMOLED-1.43 | 466x466 round | co5300 init, ft3168 | **file**, CO5300 variant only |

## One file away or close

* **1.64 V1**: the same board with chip select GPIO9 instead of 46 (the V2 firmware uses 46). A second file, or a `cs` list.
* **1.43, SH8601 variant**: offset 0 and a different init (`44 01 D1`, no `C4 80`). The board picks its variant by reading the
  panel ID at run time, which a board file cannot say: needs a `variant` option on the driver.

## Needs a driver

| Board | Screen | Controller, bus | Touch | Pins read from the vendor |
|---|---|---|---|---|
| ESP32-C6-LCD-1.47 (what WeatherTrack's `-gui` runs) | 172x320 | ST7789, SPI | none (button GPIO9, WS2812 GPIO8) | SCLK 7, MOSI 6, CS 14, DC 15, RST 21, BL 22, offset 34,0, inverted. **run** in weathertrack-gui |
| ESP32-C6-Touch-LCD-1.83 | | ST7789, SPI | yes | not extracted |
| ESP32-C6-Touch-LCD-2.8 | | SPI | I2C 6/7, INT 18 | SCLK 0, MOSI 1, DC 10, CS 11 |
| ESP32-C6-Touch-AMOLED-1.64 | 280x456 | SH8601 (the CO5300 init), QSPI | FT3168 | not extracted: a C6 has fewer pins than the S3 |
| ESP32-S3-Touch-AMOLED-1.8 | 368x448 | SH8601, QSPI | FT3168 or CST816, I2C 15/14 | **TCA9554 I/O expander** drives the display reset: Fusuma has no expander support |
| ESP32-S3-Touch-AMOLED-2.41 | 600x450 | SH8601 library with an RM690B0-style init (`FE 20`, `26 0A`, `24 80`), QSPI | FT5x06, I2C 47/48, INT 3 | CS 9, SCLK 10, D0..3 = 11..14 |
| ESP32-S3-Touch-AMOLED-1.75C, 2.16 | 1.75C: 466x466 | not confirmed | I2C 15/14 | SCLK 38, CS 12 (the pin file only) |
| ESP32-P4-WIFI6-Touch-LCD 3.5, 4.3, 5, X, XC | | not read | | `7B` is `p4-7in` |

Also exist as Waveshare repos and were not looked at: ESP32-S3-Touch-AMOLED-1.32, ESP32-C6-Touch-AMOLED-1.8, -2.06, -2.16,
ESP32-C6-GEEK, ESP32-C6-Zero. Other makers (Seeed, LilyGO T-Display / T-Watch, M5Stack, the "cheap yellow display"
ESP32-2432S028, Espressif's own DevKits) are not researched.

## Order that pays most

1. **ST7789 over SPI** (one driver): the C6 1.47 WeatherTrack already runs, plus the C6 Touch LCD 1.83 and 2.8, and a large
   share of cheap boards. It needs a plain-SPI bus next to the QSPI one.
2. **A `variant` option on `co5300`** (1.43 SH8601, 1.64 V1/V2): small, and turns three boards into files.
3. **An I/O expander in the board file** (TCA9554): the 1.8.
4. **RM690B0 init** (2.41) and a **FT5x06** touch driver.
