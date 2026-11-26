# E-paper (e-ink) in Fusuma: hardware and what is supported

Code: `fusuma_platform_embedded::eink` (the drivers and the refresh logic, `no_std`, tested on the computer against a recording
bus) and `fusuma-platform-esp` (`src/eink.rs`, `components/fusuma_esp/epaper_spi.c`: the SPI bus, the pins, the run loop).
Nothing here has been checked on a panel except the reTerminal Sticky's path in earlier work; every other panel is a driver
that follows the vendor's sequences and has run against the recording bus only.

## What a panel is, for the app

* One bit a pixel (some panels four grays), black on white. The app draws as always; `EinkDriver` tells it the glass is
  reflective (`Input::Screen`), so the theme turns to `Palette::PAPER` and motion stops.
* A **full refresh** flashes for one to four seconds and clears ghosts; a **partial refresh** is a few tenths of a second and
  leaves a trace. `RefreshPolicy` decides: it waits for the screen to be quiet (`settle_ms`), keeps a minimum gap
  (`min_interval_ms`), and runs a full refresh every `full_every` partial ones.
* Panels differ in whether they refresh partially at all (the Sticky's newer SSD2677 panel does not: full only).

## Wiring

Always: SCK, MOSI, CS, DC (data or command), RST, BUSY. Often: a pin or expander output that switches the panel's power. The
controller's busy polarity differs (SSD1677, SSD1680: high while working; UC8179: low). Voltage is 3.3 V.

A write may carry a fourth number, a mask: `[0x34, 0x90, 0x04, 0x04]` reads register 0x90 and changes only bit 2 (the AXP2101 keeps several rails in one register).
A rail behind an I2C chip (an I/O expander, a PMIC) is `[power.i2c]`: the bus pins and `[address, register, value]` writes done
at boot, after the `[power]` pins (the C6 board's TCA9554: output register 1, direction register 3). The bus is freed after
the writes, so a touch driver may open its own on the same pins.

A board file names them (`display.pins`, `[power]` for rails that must be high or low at boot); see
`crates/fusuma-platform-esp/boards/*/board.toml`.

## Controllers

| Controller | Panels | In a board file | Notes |
|---|---|---|---|
| `ssd1680` | 1.54" 200x200 (SSD1681), 2.13" 122x250, 2.9" 128x296 | yes, `s3-epaper-154` | Waveshare V2/V4 sequences, partial refresh |
| `ssd1677` | 3.97" and other 800x480 | yes, `s3-sticky` | `mirror_x` for the Sticky's wiring |
| `ssd2677` | the Sticky's newer 3.97" panel | through `Sticky::detect` (reads register 0x70) | full refresh only, waveform from the temperature |
| `uc8179` | Waveshare 7.5" 800x480 | yes (controller name) | busy low while working, partial refresh |

Screen presets in `fusuma-platform`: `epaper-1in54`, `-2in13`, `-2in9`, `-4in2`, `-7in5`, `-3in97`. The 4.2" (UC8176) has a
preset and no driver.

## Boards

| Id | Friendly name | Panel | Status |
|---|---|---|---|
| `s3-sticky` | Seeed reTerminal Sticky | 3.97" 800x480, SSD1677 or SSD2677, GT911 touch, three keys | file, built |
| `s3-epaper-154` | Waveshare ESP32-S3-ePaper-1.54 | 1.54" 200x200, SSD1681 | file from vendor firmware, built, not run |
| `s3-epaper-397` | Waveshare ESP32-S3-ePaper-3.97 | 3.97" 800x480, SSD1677; rails from an AXP2101 (`[power.i2c]`: voltages as the vendor sets them, ALDO3 switched on with a masked write) | file from vendor firmware, built, not run; the vendor counts Y down, the driver up: the picture may be upside down |
| `c6-epaper-154` | Waveshare ESP32-C6-ePaper-1.54 | 1.54" 200x200, SSD1681; panel power and battery latch on a TCA9554 (`[power.i2c]`) | file from vendor firmware, built and linked for the C6, not run |

Not boards yet:

| Board | Why not |
|---|---|
| Waveshare ESP32-S3-ePaper-1.54G | read: a **four-colour** (black, white, red, yellow) 1.54" 200x200 panel; the init (`0x4D`, PSR `0x00` = `0x0F 0x29`, ...) is the UC81xx colour family, not SSD168x, and the app would need a four-colour pipeline (palette quantiser), not the one-bit one. Not supported |
| Waveshare ESP32-S3-ePaper-13.3E6, S3-PhotoPainter | not read; the 13.3" is an IT8951-class or six-colour controller (a different driver) |
| Waveshare ESP32-S3-RLCD-4.2, S3-LR1121-ePaper-2.9 | not read yet (RLCD is a reflective LCD, not e-paper) |
| Raspberry Pi Pico / RP2040 e-paper hats | not an ESP; needs the Cortex-M target |

## Adding an e-paper board

1. Read the vendor's pin header and panel driver (`waveshareteam/<board>`): pins, the busy polarity, whether the power switch is
   a GPIO (`[power]`) or an expander, the controller (its init sequence names it: `0x12` soft reset then `0x01` driver output =
   SSD168x; `0x00` panel setting = UC81xx).
2. Copy `boards/s3-epaper-154/`, set the pins, the size, the preset, the flash size you can verify, and a partition table.
3. If the controller is not in the table above it is a Rust `Panel` implementation in `eink/`, an entry in `CONTROLLERS`
   (`fusuma-platform/src/board.rs`), the schema's list and `build.rs`; a test against `bus::Recorder` first.
4. `nu scripts/showcase.nu build --board <id>`, then, with a board and a backup, `install --backup`.

## Checking a driver without a panel

`crates/fusuma-platform-embedded/tests/eink_sim.rs` is a model of the SSD168x memory (window, address counter, entry mode, the
two RAMs, what an update shows) used as the driver's bus. It checks what ends up on the glass: a full refresh and partial ones
must show exactly the picture, on a straight and a mirrored panel, for the 1.54", 2.13", 2.9" and 800x480 sizes, with no partial
refresh that would flash. Two tests make the model fail on purpose (a mirrored panel, the wrong X unit) so a pass means
something. It also holds the vendor's init as written in Waveshare's firmware and lists every command where the driver differs
(`differences`), so a new difference fails the test.

What it found so far, and cannot settle without a panel:

* **3.97":** the vendor counts the Y address down (entry mode 1, window 479 to 0), the driver up. With the same gate scan one is
  upside down. Look at the vertical direction first.
* **1.54":** the vendor loads its own waveform table (command 0x32, then update 0xC7); the driver uses the built-in waveform (0xF7,
  border 0x05) of Waveshare's V4 sequence, with Y counting up. If the picture is faint or ghosts, try the vendor's table.
* **3.97" power:** the vendor switches ALDO3 on in `EPD_Power_ON`. An earlier version of the board file left that to the chip's
  defaults; the masked write fixes it.

The model is built from the same data sheets and vendor code as the drivers, so it finds the driver's own mistakes, not a wrong
reading of the chip.

## Ghosting, temperature, power

Partial refreshes leave ghosts; the policy's full refresh clears them. Cold panels refresh slowly and with weaker contrast
(the SSD2677 reads the temperature; the others use the controller's own sensor, `TEMP_SENSOR` 0x18). Put the panel to
sleep (`Panel::sleep`) between updates: it holds the picture with no power, which is the point of e-paper, and the driver
wakes it on the next refresh.
