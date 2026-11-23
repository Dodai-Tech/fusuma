# fusuma-platform-esp

Runs a Fusuma `App` on an ESP-IDF board: brings up the display and the touch panel, feeds touch to the app,
ticks it, has it drawn and puts only what changed on the screen. **The app never names a renderer.** At start
it reads what the chip has (internal RAM, PSRAM, FPU), asks `fusuma-platform` which backend suits it, and logs
which one it chose and why:

```
renderer: vello-cpu (automatic: PSRAM holds a Vello frame twice over)
```

```rust
fusuma_platform_esp::run(|w, h| MyApp::new(w, h), fusuma_platform_esp::Options::default())
```

Overrides, strongest first: `Options::build` (a cargo feature or the program), the project's `fusuma.toml`
(`Options::settings`, embedded with `include_str!`), then the automatic rule; see `fusuma-platform` for the keys.
With `fallback` (on by default) an explicit choice that does not fit in memory steps down to a lighter one. Cargo features
`skia` and `vello-cpu` (on by default) link those renderers next to the integer one; turn both off for the
smallest firmware, and the choice is made among what is linked.

Today it supports the Waveshare ESP32-P4 7" touch board (1024x600, GT911); the C shim for its display and touch
is in `components/wt_display` (from the WeatherTrack P4 firmware). The firmware crate that uses it must list
that directory in its `package.metadata.esp-idf-sys.extra_components`, see `examples/showcase-p4`.

A choice is predicted from the memory the chip reports; allocation failure inside a renderer cannot be caught,
so the check has to be right before the surface is made.

## What this crate does not contain

Components, layout, theme, focus, scrolling, the frame clock and the turning of touch samples into pointer events
are in `fusuma-ui` (`Shell`, `TouchTracker`, `Input`), shared by every platform. This crate has only what is
specific to a board: the DSI display and the GT911 (`hw.rs`), converting the renderer's pixels to RGB565, and the
loop that ties them together. CI fails if a `fusuma-platform-*` crate builds a widget tree or implements a renderer.
