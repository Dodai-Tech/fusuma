# fusuma-platform-mobile

What every phone and tablet host has in common, and, behind a feature for each OS (`android`, `ios`, `linux`), the translation of that system's own input and display values into them. `no_std`, no allocator, and tested
on the desktop: no phone is needed to check the rules.

- **`Session`** turns the system's lifecycle events (created, resumed, paused, surface created, surface destroyed, low memory)
  into what the host must do: make the GPU surface, drop it, draw, stop drawing, or release memory. A phone destroys the
  surface when the app goes to the background and gives a new one on return; drawing to a dead surface is a crash.
- **`Display`** is the screen as the system describes it (pixels, scale, notch and gesture-bar insets, the soft keyboard's
  height) and gives the rectangle an app may lay out in, with the keyboard counted as an inset.
- **`SoftKeyboard`** says when to show or hide the keyboard, from whether a text field has the focus, and only once per change.
- **`back`** maps the system back gesture or button to Fusuma's cancel key (which closes a dialog or a menu first).

Status: the rules and their tests are done; the hosts that use them are scaffolds (see the roadmap).

## One crate, a feature for each OS

The three OS hosts are modules of this crate, behind features, so there is one name to depend on:

```toml
fusuma-platform-mobile = { version = "0.2", features = ["android"] }   # or "ios", "linux"
```

- `android`: `MotionEvent` and `KeyEvent` codes to `Pointer`, `Key` and `Edit`; `android::display(..)` from `DisplayMetrics`.
- `ios`: `UITouch` phases, hardware key commands, the edge-swipe back; `ios::display(..)` from a `UIScreen`.
- `linux`: shell detection (Phosh, Plasma Mobile, Sxmo), `wl_output` rotation; `linux::display(..)` from a `wl_output`.

All three are scaffolds: no activity, view or window runs an app yet. When a host needs heavy dependencies (`android-activity`,
`objc2`, a Wayland client) they come in behind the same feature.
