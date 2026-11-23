# Fusuma examples

**`showcase/`** is a small app that uses every Fusuma widget: buttons, checkboxes, radio buttons, a list,
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
drawing. Text uses the three built-in bitmap fonts (up to 10x20), so it looks small on a 7" screen until
`fusuma-text` exists.
