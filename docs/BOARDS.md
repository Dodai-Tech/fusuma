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
| `c6-epaper-154` | Waveshare ESP32-C6-ePaper-1.54 | 200x200 e-paper | ssd1680 | **file**; panel power and battery latch through `[power.i2c]` on the TCA9554; built and linked for the C6, not run; 4 MB flash assumed |
| `s3-epaper-397` | Waveshare ESP32-S3-ePaper-3.97 | 800x480 e-paper | ssd1677 | **file**; AXP2101 voltages through `[power.i2c]`; built, not run; 16 MB flash; orientation unverified |
| `s3-epaper-154` | Waveshare ESP32-S3-ePaper-1.54 | 200x200 e-paper | ssd1680 | **file** from vendor firmware (GPIO6 low powers the panel); built, not run; 4 MB flash assumed |
| `s3-amoled-143` | Waveshare ESP32-S3-Touch-AMOLED-1.43 | 466x466 round | co5300 init, ft3168 | **file**, CO5300 variant only |

E-paper boards, the controllers and how to add one: [EINK.md](EINK.md).

## One file away or close

* **1.64 V1**: the same board with chip select GPIO9 instead of 46 (the V2 firmware uses 46). A second file, or a `cs` list.
* **1.43, SH8601 variant**: offset 0 and a different init (`44 01 D1`, no `C4 80`). The board picks its variant by reading the
  panel ID at run time, which a board file cannot say: needs a `variant` option on the driver.

## Needs a driver

| Board | Screen | Controller, bus | Touch | Pins read from the vendor |
|---|---|---|---|---|
| ESP32-C6-LCD-1.47 (what WeatherTrack's `-gui` runs) | 172x320 | ST7789, SPI | none (button GPIO9, WS2812 GPIO8) | SCLK 7, MOSI 6, CS 14, DC 15, RST 21, BL 22, offset 34,0, inverted. **run** in weathertrack-gui |
| ESP32-C6-ePaper-1.54 | 200x200 e-paper | SSD1681 (the `epaper-1in54` preset), SPI | none | SCK 6, MOSI 5, MISO 4, CS 7, DC 15, RST 11, BUSY 10; I2C SDA 18, SCL 8; BOOT 9, PWR button 2; 16 MB flash, no PSRAM. **The panel's power switch is expander output 0** (TCA9554 at 0x20, high = on; the same chip also gates audio on 1, the battery sense on 5 and the LED on 4). Source: `waveshareteam/ESP32-C6-ePaper-1.54`, `02_Example/espidf_v5.5.3`. Not a file yet: Fusuma's e-paper driver switches power with a GPIO (`en`), so without the expander the panel stays dark |
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

## Every Waveshare ESP32 board (from docs.waveshare.com/ESP32-Series-Products-Intro)

The vendor's own product list, by chip, read from its sidebar (the wiki says the list is still being migrated, so it may be
incomplete). Only the boards in the tables above have a file or a researched pin map; the rest are a name each. Pages are
`https://docs.waveshare.com/<name>`; the code repositories are `github.com/waveshareteam/<name>`.

Wanted next, by what you asked for:

* **Round IPS:** `ESP32-S3-Touch-LCD-1.28`, `ESP32-C6-Touch-LCD-1.28` (GC9A01 + CST816 is the `round-1in28` preset; needs the SPI display driver), and the dual-eye `ESP32-S3-DualEye-Touch-LCD-1.28`.
* **E-paper:** `ESP32-C6-ePaper-1.54`, `ESP32-S3-ePaper-1.54`, `ESP32-S3-ePaper-3.97`, `ESP32-S3-ePaper-1.54G`, `ESP32-S3-ePaper-13.3E6`, `ESP32-S3-PhotoPainter`. The 1.54 panel is the `epaper-1in54` preset (SSD1681), which Fusuma already knows; the board wiring is still to read from the vendor repo.
* **Watches:** `ESP32-S3-Touch-AMOLED-2.06` (done), `-1.43` (done as a file), `-1.75`, `-1.75C`, `-1.8`, `-1.32`.

**ESP32-S3** (74): `ESP32-S3-AMOLED-1.91`, `ESP32-S3-AUDIO-Board`, `ESP32-S3-CAM-OVxxxx`, `ESP32-S3-CAN-2CH`, `ESP32-S3-DEV-KIT-N8R8`, `ESP32-S3-DualEye-LCD-1.28`, `ESP32-S3-DualEye-Touch-LCD-1.28`, `ESP32-S3-ETH`, `ESP32-S3-GEEK`, `ESP32-S3-LCD-0.85`, `ESP32-S3-LCD-1.28`, `ESP32-S3-LCD-1.47B`, `ESP32-S3-LCD-1.69`, `ESP32-S3-LCD-1.85`, `ESP32-S3-LCD-1.9`, `ESP32-S3-LCD-2`, `ESP32-S3-LCD-2.8`, `ESP32-S3-LCD-2.8B`, `ESP32-S3-LCD-2.8C`, `ESP32-S3-LCD-Driver-Board`, `ESP32-S3-LR1121-XF`, `ESP32-S3-LR1121-ePaper-2.9`, `ESP32-S3-Matrix`, `ESP32-S3-Nano`, `ESP32-S3-POE-ETH-8DI-8DO`, `ESP32-S3-PhotoPainter`, `ESP32-S3-Pico`, `ESP32-S3-RGB-Matrix`, `ESP32-S3-RLCD-4.2`, `ESP32-S3-RS485-CAN`, `ESP32-S3-RS485-WLED`, `ESP32-S3-Relay-6CH`, `ESP32-S3-Tiny`, `ESP32-S3-Touch-AMOLED-1.32`, `ESP32-S3-Touch-AMOLED-1.43`, `ESP32-S3-Touch-AMOLED-1.43C`, `ESP32-S3-Touch-AMOLED-1.64`, `ESP32-S3-Touch-AMOLED-1.75`, `ESP32-S3-Touch-AMOLED-1.75C`, `ESP32-S3-Touch-AMOLED-1.8`, `ESP32-S3-Touch-AMOLED-2.06`, `ESP32-S3-Touch-AMOLED-2.16`, `ESP32-S3-Touch-AMOLED-2.41`, `ESP32-S3-Touch-LCD-1.28`, `ESP32-S3-Touch-LCD-1.46`, `ESP32-S3-Touch-LCD-1.47`, `ESP32-S3-Touch-LCD-1.54`, `ESP32-S3-Touch-LCD-1.69`, `ESP32-S3-Touch-LCD-1.83`, `ESP32-S3-Touch-LCD-1.85`, `ESP32-S3-Touch-LCD-1.85B`, `ESP32-S3-Touch-LCD-1.85C`, `ESP32-S3-Touch-LCD-2`, `ESP32-S3-Touch-LCD-2.1`, `ESP32-S3-Touch-LCD-2.8`, `ESP32-S3-Touch-LCD-2.8B`, `ESP32-S3-Touch-LCD-2.8C`, `ESP32-S3-Touch-LCD-3.49`, `ESP32-S3-Touch-LCD-3.5`, `ESP32-S3-Touch-LCD-3.5B`, `ESP32-S3-Touch-LCD-4.3`, `ESP32-S3-Touch-LCD-4.3B`, `ESP32-S3-Touch-LCD-4.3C`, `ESP32-S3-Touch-LCD-4B`, `ESP32-S3-Touch-LCD-5`, `ESP32-S3-Touch-LCD-7`, `ESP32-S3-Touch-LCD-7B`, `ESP32-S3-Touch-LCD-7C-BOX`, `ESP32-S3-Zero`, `ESP32-S3-ePaper-1.54`, `ESP32-S3-ePaper-1.54G`, `ESP32-S3-ePaper-13.3E6`, `ESP32-S3-ePaper-3.97`, `ESP32-S3-LR1121-OLED-1.54`

**ESP32-C6** (25): `ESP32-C6-DEV-KIT-N8`, `ESP32-C6-GEEK`, `ESP32-C6-LCD-0.85`, `ESP32-C6-LCD-1.28`, `ESP32-C6-LCD-1.3`, `ESP32-C6-LCD-1.47`, `ESP32-C6-LCD-1.69`, `ESP32-C6-LCD-1.9`, `ESP32-C6-LCD-2.73`, `ESP32-C6-Pico`, `ESP32-C6-Touch-AMOLED-1.32`, `ESP32-C6-Touch-AMOLED-1.43`, `ESP32-C6-Touch-AMOLED-1.64`, `ESP32-C6-Touch-AMOLED-1.8`, `ESP32-C6-Touch-AMOLED-2.06`, `ESP32-C6-Touch-AMOLED-2.16`, `ESP32-C6-Touch-LCD-1.28`, `ESP32-C6-Touch-LCD-1.47`, `ESP32-C6-Touch-LCD-1.54`, `ESP32-C6-Touch-LCD-1.69`, `ESP32-C6-Touch-LCD-1.83`, `ESP32-C6-Touch-LCD-2.8`, `ESP32-C6-Zero`, `ESP32-C6-Zero-B`, `ESP32-C6-ePaper-1.54`

**ESP32-P4** (19): `ESP32-P4-Core`, `ESP32-P4-Core-DEV-KIT`, `ESP32-P4-ETH`, `ESP32-P4-Knob-Touch-AMOLED-1.75X`, `ESP32-P4-Module-DEV-KIT`, `ESP32-P4-NANO`, `ESP32-P4-NANO-WIFI6-DB`, `ESP32-P4-Pico`, `ESP32-P4-WIFI6`, `ESP32-P4-WIFI6-DB`, `ESP32-P4-WIFI6-DEV-KIT`, `ESP32-P4-WIFI6-POE-ETH`, `ESP32-P4-WIFI6-Touch-LCD-3.5`, `ESP32-P4-WIFI6-Touch-LCD-4.3`, `ESP32-P4-WIFI6-Touch-LCD-4B`, `ESP32-P4-WIFI6-Touch-LCD-5`, `ESP32-P4-WIFI6-Touch-LCD-7B`, `ESP32-P4-WIFI6-Touch-LCD-X`, `ESP32-P4-WIFI6-Touch-LCD-XC`

**ESP32-C5** (9): `ESP32-C5-LCD-1.47`, `ESP32-C5-LCD-2.73`, `ESP32-C5-MINI-KIT`, `ESP32-C5-Pico`, `ESP32-C5-Touch-LCD-1.69`, `ESP32-C5-Touch-LCD-2.8`, `ESP32-C5-Touch-LCD-3.5`, `ESP32-C5-WIFI6-KIT`, `ESP32-C5-Zero`

**ESP32-S31** (2): `ESP32-S31-Zero`, `ESP32-S31-Touch-LCD-4.3C`

**ESP32-C3** (3): `ESP32-C3-LCD-0.71`, `ESP32-C3-LCD-1.47`, `ESP32-C3-Zero`

**ESP32-H2** (2): `ESP32-H2-DEV-KIT-N4`, `ESP32-H2-Zero`

**ESP32 (classic)** (2): `DEV-KIT-XX`, `Touch-LCD-3.5`

