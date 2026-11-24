# fusuma-platform

The out-of-the-box renderer choice of Fusuma. Given a `Machine` (screen size, RAM, PSRAM, FPU, SIMD, GPU and what
the settings say) it picks the backend a device should draw with, and says why: the build beats the settings,
which beat an automatic rule; a memory check keeps or steps down an explicit choice; and a choice that is not
linked is replaced by the nearest lighter one.

The automatic rule: **Vello on the GPU** when there is one (the desktop falls back to Vello on the CPU if the GPU
fails at run time); **Vello on the CPU** where Vello has SIMD (x86, x86_64, aarch64, WebAssembly) and its frame
fits; else the **integer renderer**, which is several times faster than Vello's scalar fallback on a microcontroller
(on the S3 watch a tap takes 7 ms against 47). The anti-aliased look on those chips comes from the integer renderer
itself as it grows (see the roadmap), not from Vello. `no_std`, no dependencies, so a platform crate, a build
script and a flashing wizard share one answer.

## Settings

Every decision can be overridden from a TOML file, and nothing has to be. Layers, weakest first: the automatic
rule, the project's `fusuma.toml`, a user or device file, the build (a cargo feature or a flag). A key left out
says nothing, so the layer below decides; `backend = "auto"` written out undoes a weaker layer's pick.

```toml
[render]
backend = "auto"      # auto (the default), integer, vello-cpu or vello-gpu
fallback = true       # step down to a lighter backend if the chosen one does not fit; default true
min_free_kb = 100     # internal RAM that must stay free after the frame; default 100
```

`min_free_kb` is the minimum internal RAM left over once the frame is placed (a frame in PSRAM costs internal RAM
nothing). **The default is a conservative guess, not a measurement:** the right figure is what WiFi, Bluetooth and
the app need at their peak, which you can read from the heap's minimum-free watermark while the firmware runs.

With the `toml` feature, `RenderLayer::from_toml` reads a file (unknown keys and backend names are errors that
name them), `RawRender` lets another crate's file embed a `[render]` table, and `SETTINGS_SCHEMA` is the JSON
schema (`fusuma.schema.json`). `Render::layered(&[file, user, build])` combines layers.

## Board files

With the `toml` feature, `board::BoardFile::from_toml` reads a **board file**: one TOML file that describes a board,
its parts and how they are wired, so a board made of known parts is a file, not code.

```toml
name = "s3-amoled-206"
about = "Waveshare ESP32-S3-Touch-AMOLED-2.06 watch (410x502)"

[display]
width = 410
height = 502
shape = "rounded"     # rect (default), rounded (with radius) or circle
radius = 70
offset = [22, 0]      # where the visible area starts in the panel's memory
grid = 2              # rectangles start and end on multiples of this
controller = "co5300"
bus = "qspi"
pins = { sck = 11, d = [4, 5, 6, 7], cs = 12, rst = 8 }

[touch]
controller = "ft3168"
pins = { sda = 15, scl = 14, rst = 9 }

[firmware]            # optional: only for a board that is flashed
chip = "esp32s3"
flash_mb = 32
boot_limit_mb = 16    # the bootloader boots no app past this
```

`[backlight]`, `[hardware]` and `[render]` are optional too. Every key, controller, pin and option is checked against
`board::CONTROLLERS` (which chips a controller works on, which pins it needs, its numbers with their ranges and
defaults), and an error names the file and the key. `BOARD_SCHEMA` is the JSON schema (`board.schema.json`).
`fusuma-platform-esp` builds a firmware from these files, `scripts/showcase.nu` recognises and flashes boards from
them, and Hibari OS reads the same format.
