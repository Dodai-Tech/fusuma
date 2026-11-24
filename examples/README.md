# Fusuma examples

**`showcase/`** is a small app that uses every Fusuma widget: buttons, checkboxes, radio buttons, a switch, a slider, a list,
a progress bar, keyboard focus and a dark/light theme switch. It is `no_std` and draws on any renderer, so
the same code is tested on the host (`just showcase-shots` writes a PNG of every page to
`docs/screenshots/`) and runs on a board.

**`showcase-p4/`** is that app on the Waveshare ESP32-P4 7" touch board (1024x600, GT911 touch), drawn with
`vello_cpu` straight into the DSI frame buffer, redrawing only the part that changed. It is an ESP-IDF
project of its own (it needs the ESP toolchain, so it is not a workspace member); the display and touch C
shim in `components/wt_display` comes from the WeatherTrack P4 firmware.

```
just showcase-p4-install /dev/ttyACM0   # build and flash to the ota_0 slot; the factory app stays
just showcase-p4-restore /dev/ttyACM0   # boot the factory app again
```

Measured on the board: the first frame (1024x600) takes about 210 ms (76 ms drawing), and a small update
such as a hovered button about 60 ms, most of it converting the pixels and clearing the surface, not
drawing. Text is Inter, Inter Bold and JetBrains Mono through `fusuma-text`, with the built-in bitmap font as the
fallback (the Typography page shows all three).

**`showcase-desktop/`** is the same app in a window on a computer: `just showcase-desktop` (or `just showcase-desktop
--renderer vello-cpu` to force the CPU). It uses `fusuma-platform-desktop`, which picks Vello on the GPU when there is one.

**`tools/fusuma-showcase-s3`** (in the Hibari repository, next to the drivers it reuses) is the same app on the
Waveshare ESP32-S3-Touch-AMOLED-2.06 watch (410x502 AMOLED, FT3168 touch), on `esp-hal` with no ESP-IDF. On a screen under
640 logical pixels wide the app switches to a compact layout (a Previous and Next row instead of the sidebar, pages that
scroll). From the Hibari repository: `just native-install board=s3-fusuma-showcase`. Measured on the watch, the first
full frame takes 196 ms with the integer renderer and bitmap text, 523 ms with Vello and bitmap text, 948 ms with Vello and
the Inter fonts, and 1883 ms with `tiny-skia`; the shared rule picks Vello when it is linked.
