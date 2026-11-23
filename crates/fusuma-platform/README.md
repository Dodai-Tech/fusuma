# fusuma-platform

The out-of-the-box renderer choice of Fusuma. Given a `Machine` (screen size, RAM, PSRAM, FPU, GPU and what the
settings say) it picks the backend a device should draw with, and says why: the build beats the settings,
which beat an automatic rule; a memory check keeps or steps down an explicit choice; and a choice that is not
linked is replaced by the nearest lighter one. `no_std`, no dependencies, so a platform crate, a build
script and a flashing wizard share one answer.

## Settings

Every decision can be overridden from a TOML file, and nothing has to be. Layers, weakest first: the automatic
rule, the project's `fusuma.toml`, a user or device file, the build (a cargo feature or a flag). A key left out
says nothing, so the layer below decides; `backend = "auto"` written out undoes a weaker layer's pick.

```toml
[render]
backend = "auto"      # auto (the default), integer, skia, vello-cpu or vello-gpu
fallback = true       # step down to a lighter backend if the chosen one does not fit; default true
min_free_kb = 100     # internal RAM that must stay free after the frame; default 100
```

`min_free_kb` is the minimum internal RAM left over once the frame is placed (a frame in PSRAM costs internal RAM
nothing). **The default is a conservative guess, not a measurement:** the right figure is what WiFi, Bluetooth and
the app need at their peak, which you can read from the heap's minimum-free watermark while the firmware runs.

With the `toml` feature, `RenderLayer::from_toml` reads a file (unknown keys and backend names are errors that
name them), `RawRender` lets another crate's file embed a `[render]` table, and `SETTINGS_SCHEMA` is the JSON
schema (`fusuma.schema.json`). `Render::layered(&[file, user, build])` combines layers.
