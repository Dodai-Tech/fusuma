# fusuma-anim

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/Dodai-Tech/fusuma/blob/main/LICENSE)

Animation for [Fusuma](https://github.com/Dodai-Tech/fusuma), in fixed-point maths so that it runs
the same on a microcontroller without an FPU as on a desktop. `no_std`, no allocator, no `unsafe`,
no dependencies.

| Module | What |
|---|---|
| `Fx` | Q16.16 fixed-point number with saturating arithmetic |
| `Easing` | linear, quad, cubic, quart, smooth, back, bounce and CSS-style cubic Bezier curves |
| `Tween` | a value that moves between two points over time, with a delay and an easing; can be retargeted mid-flight |
| `Keyframes` | a fixed number of timed values with an easing on each segment |
| `Spring` | a damped spring (physics): stiffness, damping and mass, or a response time and damping ratio, with presets |
| `Decay` | a fling: velocity that fades with friction, for scrolling and swipes |
| `Animator` | up to N running tweens, springs and flings on one frame clock; `step` says whether another frame is needed, so a still screen costs nothing |
| `rubber_band`, `snap_page` | dragging: overscroll that resists, and the page a flick settles on |

All of them are driven by a clock you pass in, in milliseconds, so there is no hidden time source
and every animation is testable and deterministic.

```rust
use fusuma_anim::{Easing, Fx, Spring, SpringConfig, Tween};

// 300 ms ease-out from 0 to 100, started at t = 1000 ms.
let tween = Tween::new(Fx::ZERO, Fx::from_int(100), 1000, 300, Easing::CubicOut);
assert_eq!(tween.value(1000), Fx::ZERO);
assert_eq!(tween.value(1300), Fx::from_int(100));

// A bouncy spring pulled from 0 towards 100, advanced by 16 ms frames.
let mut spring = Spring::new(SpringConfig::WOBBLY, Fx::ZERO);
spring.set_target(Fx::from_int(100));
while spring.step(16) {}
assert_eq!(spring.position(), Fx::from_int(100));
```

## Presets and clips

- **Spring tokens** (`SpringConfig`): `SPATIAL_FAST`, `SPATIAL`, `SPATIAL_SLOW` for things that move or resize (a tiny
  overshoot) and `EFFECTS_FAST`, `EFFECTS`, `EFFECTS_SLOW` for colour and opacity (never overshoot). Modelled on the
  spring values of Material 3's motion scheme, written from memory and not copied from its source. Tests check that the
  effects springs do not overshoot, that faster tokens settle sooner, and that a slow frame rate (120 ms steps, as on a
  watch) still ends on the target.
- **`Clip`**: a short timeline built by chaining `tween`, `segment` and `hold` with `then`, `delay`, `repeat`, `forever`,
  `ping_pong`, `reverse`, `speed` and `stagger`. It is a `Copy` value of at most 8 pieces, with no allocation and no
  closures, so it works on a chip. Presets: `fade`, `slide`, `pop` (overshoots) and `shake`. Read it with
  `value(t_ms)` and `is_done(t_ms)`. A clip longer than 8 pieces drops the extras; to move two properties together,
  read two clips at the same time. Arbitrary closures and `map`/`zip` are left out on purpose.

