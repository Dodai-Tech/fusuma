# fusuma-platform-esp

Runs a Fusuma `App` on an ESP-IDF board: brings up the display and the touch panel, feeds touch to the app,
ticks it, has it drawn and puts only what changed on the screen. **The app never names a renderer.** At start
it reads what the chip has (internal RAM, PSRAM, FPU), asks `fusuma-platform` which backend suits it, and logs
which one it chose and why:

```
renderer: vello-cpu (automatic: no GPU, so Vello on the CPU)
```

```rust
fusuma_platform_esp::run(|w, h| MyApp::new(w, h), fusuma_platform_esp::Options::default())
```

Fusuma draws with Vello only; a board has no GPU `wgpu` can use, so this is Vello on the CPU. A board without
the memory for a frame (no PSRAM, like the ESP32-C6) is refused at start with a message instead of running out
of memory later. `Options::backend` is the build's override layer.

Today it supports the Waveshare ESP32-P4 7" touch board (1024x600, GT911); the C shim for its display and touch
is in `components/wt_display` (from the WeatherTrack P4 firmware). The firmware crate that uses it must list
that directory in its `package.metadata.esp-idf-sys.extra_components`, see `examples/showcase-p4`.

The check uses the memory the chip reports; an allocation failure inside the renderer cannot be caught, so it
has to be right before the surface is made.
