# fusuma-platform-desktop

Runs a Fusuma `App` in a window on Linux, macOS and Windows, with mouse, touch and keyboard. **The app never
names a renderer:** at start it asks `fusuma-platform` which backend suits the machine, Vello on the GPU when
`wgpu` finds an adapter and Vello on the CPU otherwise, and logs which one and why. If the GPU surface cannot be
made it steps down to the CPU.

```rust,ignore
fusuma_platform_desktop::run(|w, h| MyApp::new(w, h), fusuma_platform_desktop::Options::default())
```

Overrides, strongest first: `Options::build`, the project's `fusuma.toml` (`Options::settings`), then the
automatic rule; see `fusuma-platform` for the keys.

The window is `winit`, shown through `softbuffer`. Both backends hand over CPU pixels (the GPU one reads its
texture back), so one path presents either; drawing straight from the GPU to the window is not done yet.

## What it translates

| Window system | Fusuma |
|---|---|
| left mouse button, cursor | `Pointer::Down`, `Up`, `Move`, `Leave` |
| touch | `Pointer::Down`, `Move`, `Up` then `Leave` |
| wheel (lines or pixels, sign flipped) | `Pointer::Wheel` (a line is 40 px) |
| Tab, Shift+Tab, arrows, Enter, Space | `Key::Next`, `Prev`, `Up`, `Down`, `Left`, `Right`, `Activate` |
| resize, scale factor | `Input::Resize { width, height, scale_percent }` |

## Scale and idle

The display scale reaches the app as `Input::Resize { scale_percent }`, and `Theme::scaled(percent)` turns a theme into
one for that scale (lengths multiplied, outlines at least 1 px). The built-in bitmap font is magnified by a whole
number for sizes above 29 px. The loop sleeps (`ControlFlow::Wait`) when the app has nothing to animate or draw and
wakes on an event, so an idle window makes no wake-ups; it runs at about 60 frames a second only while the app
reports an animation.

## Limits

- Text is the bitmap fonts (up to 10x20) until `fusuma-text` exists.
- No text input, IME, clipboard or window decorations of its own.

## What this crate does not contain

Components, layout, theme, focus, scrolling, the frame clock and the turning of samples into pointer events are in
`fusuma-ui`, shared by every platform. This crate has only the window, the translation of its events, and showing
pixels. CI fails if a `fusuma-platform-*` crate builds a widget tree or implements a renderer.
