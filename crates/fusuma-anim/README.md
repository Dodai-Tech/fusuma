# fusuma-anim

[![Crates.io](https://img.shields.io/crates/v/fusuma-anim.svg)](https://crates.io/crates/fusuma-anim)
[![docs.rs](https://img.shields.io/docsrs/fusuma-anim)](https://docs.rs/fusuma-anim)
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
