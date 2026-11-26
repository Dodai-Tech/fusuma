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
| `s3-epaper-397` | Waveshare ESP32-S3-ePaper-3.97 | 800x480 e-paper | ssd1677 | **file**; AXP2101 voltages and the ALDO3 enable (masked write) through `[power.i2c]`; built, not run; 16 MB flash; Y direction differs from the vendor (see EINK.md) |
| `s3-epaper-154` | Waveshare ESP32-S3-ePaper-1.54 | 200x200 e-paper | ssd1680 | **file** from vendor firmware (GPIO6 low powers the panel); built, not run; 4 MB flash assumed |
| `s3-epaper-154g` | Waveshare ESP32-S3-ePaper-1.54G | 200x200 four-colour e-paper | epd4g | **file** from vendor firmware; built, not run; 8 MB flash; whole refreshes only, seconds each |
| `s3-epaper-133e6` | Waveshare ESP32-S3-ePaper-13.3E6 | 1200x1600 six-colour e-paper | epd13e6 | **file** from vendor firmware; built, not run; two panel controllers; 16 MB flash from the example's partition table, PSRAM assumed |
| `s3-amoled-175` | Waveshare ESP32-S3-Touch-AMOLED-1.75 | 466x466 round | co5300, cst92xx | **file**; the -B and -G variants are the same board with a case and a GPS module |
| `s3-photopainter` | Waveshare ESP32-S3-PhotoPainter | 800x480 six-colour e-paper (7.3") | epd7in3e | **file** from vendor firmware; the AXP2101 rail enables are an assumption (the firmware sets voltages and never says it enables them); whole refreshes, tens of seconds |
| `s3-rlcd-42` | Waveshare ESP32-S3-RLCD-4.2 | 400x300 reflective LCD | st7305 | **file** from vendor firmware; quick, no flash, no backlight; the ST7305's block memory layout is in `st7305::place` |
| `c6-amoled-132` | Waveshare ESP32-C6-Touch-AMOLED-1.32 | 466x466 (round assumed) | co5300, cst816 | **file**; flash 8 MB (the examples say 2 and 8); 512 KB RAM may not hold the colour surface |
| `c6-amoled-143` | Waveshare ESP32-C6-Touch-AMOLED-1.43 | 466x466 round | co5300, ft3168 | **file**; one start-up in this board's firmware, so no ID probe |
| `c6-amoled-164` | Waveshare ESP32-C6-Touch-AMOLED-1.64 | 280x456 | co5300, ft3168 | **file**; offset 20 as the S3's |
| `c6-amoled-18` | Waveshare ESP32-C6-Touch-AMOLED-1.8 | 368x448 | sh8601, ft3168 | **file**; a TCA9554's pins 4 and 5 are set high first (their purpose is not named in the examples) |
| `c6-amoled-206` | Waveshare ESP32-C6-Touch-AMOLED-2.06 | 410x502 | co5300, ft3168 | **file** |
| `c6-amoled-216` | Waveshare ESP32-C6-Touch-AMOLED-2.16 | 480x480 | co5300, cst92xx | **file** |
| `s3-amoled-143` | Waveshare ESP32-S3-Touch-AMOLED-1.43 | 466x466 round | co5300 or sh8601 (read from the chip), ft3168 | **file**; the chip's ID is read at start-up (`probe_id`), bit-banged, not run |
| `s3-amoled-18` | Waveshare ESP32-S3-Touch-AMOLED-1.8 | 368x448 | sh8601, ft3168 | **file**; reset and display power through `[power.i2c]` on the TCA9554; the modified CO5300 + CST816 variant is not a file |
| `s3-amoled-241` | Waveshare ESP32-S3-Touch-AMOLED-2.41 | 450x600 | rm690b0, ft5x06 | **file**; resets through `[power.i2c]`; the column offset 16 is the convention of the chip, not stated by Waveshare |
| `s3-amoled-175c` | Waveshare ESP32-S3-Touch-AMOLED-1.75C | 466x466 round | co5300, cst92xx | **file**; 32 MB flash |
| `s3-amoled-216` | Waveshare ESP32-S3-Touch-AMOLED-2.16 | 480x480 | co5300, cst92xx | **file** |
| `s3-amoled-132` | Waveshare ESP32-S3-Touch-AMOLED-1.32 | 466x466 round (assumed) | co5300, cst816 | **file**; flash 8 MB assumed (examples say 8 and 16) |
| `c6-lcd-147` | Waveshare ESP32-C6-LCD-1.47 | 172x320 | st7789 | **file**; pins and panel settings are what WeatherTrack's `weathertrack-gui` runs (so the display path ran on hardware, through mipidsi, not through Fusuma) |
| `c6-touch-lcd-128` | Waveshare ESP32-C6-Touch-LCD-1.28 | 240x240 round | gc9a01, cst816 | **file**; the TF card's select (GPIO15) is held high on the shared bus |
| `c6-touch-lcd-183` | Waveshare ESP32-C6-Touch-LCD-1.83 | 240x284 | st7789, cst816 | **file** |
| `s3-lcd-128` | Waveshare ESP32-S3-Touch-LCD-1.28 | 240x240 round | gc9a01, cst816 | **file**; 2 MB quad PSRAM |

E-paper boards, the controllers and how to add one: [EINK.md](EINK.md).

Every file marked **file** was written from the vendor's firmware, built and linked, and never run. Their drivers
(`qspi_amoled.c`, `lcd_spi.c`, `cst92xx.c`, the FocalTech-style touch in `ft3168.c`) were read from Waveshare's sources, not
run either. The hardware-specific facts that stay open are listed in each board file's header.

## One file away or close

* **1.64 V1**: the same board with chip select GPIO9 instead of 46 (the V2 firmware uses 46). A second file, or a `cs` list.
* **1.8, the modified variant** (CO5300 with a CST816): the board ships in two variants, told apart by which touch chip answers
  (0x15 or 0x38). The file is the original SH8601 + FT3168.

## Needs a driver or more reading

| Board | Screen | Controller, bus | Why it is not a file |
|---|---|---|---|
| ESP32-C6-Touch-LCD-2.8 | 240x320 | ST7789, SPI (SCLK 0, MOSI 1, DC 10, CS 11) | the LCD reset, the backlight PWM and the touch reset all go through a **CH32V003 co-processor** ("EXIO") on I2C 6/7: a protocol Fusuma has not read. Touch is a CST3530 (another protocol), INT 18 |
| ESP32-P4-WIFI6-Touch-LCD 3.5, 4.3, 5, X, XC | | not read | `7B` is `p4-7in` |

Also exist as Waveshare repos and were not looked at: ESP32-C6-Touch-AMOLED-1.8, -2.06, -2.16, ESP32-C6-GEEK, ESP32-C6-Zero.
Other makers (Seeed, LilyGO T-Display / T-Watch, M5Stack, the "cheap yellow display" ESP32-2432S028, Espressif's own DevKits)
are not researched.

## What is left

1. Run any of the files above on a board: the first thing to read is the serial log (`panel errors` in the per-update line) and
   whether the glass shows the picture; then `espflash board-info` for the real flash size.
2. A CH32V003 EXIO driver (the C6 2.8").
3. CST3530 touch.

## Every Waveshare ESP32 board (from docs.waveshare.com/ESP32-Series-Products-Intro)

The vendor's own product list, by chip, read from its sidebar (the wiki says the list is still being migrated, so it may be
incomplete). Only the boards in the tables above have a file or a researched pin map; the rest are a name each. Pages are
`https://docs.waveshare.com/<name>`; the code repositories are `github.com/waveshareteam/<name>`.

Wanted next, by what you asked for:

* **Round IPS:** `ESP32-S3-Touch-LCD-1.28`, `ESP32-C6-Touch-LCD-1.28` (GC9A01 + CST816 is the `round-1in28` preset; both are files now), and the dual-eye `ESP32-S3-DualEye-Touch-LCD-1.28`.
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

