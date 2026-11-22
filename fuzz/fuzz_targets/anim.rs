#![no_main]
//! Any easing, tween, keyframes, spring or decay on any input never panics, and the physics keeps
//! its promises: a spring stays inside the number range and settles exactly on its target; an
//! easing is 0 at 0 and 1 at 1.

use fusuma_anim::{Bezier, Decay, Easing, Fx, Keyframe, Keyframes, Spring, SpringConfig, Tween};
use libfuzzer_sys::fuzz_target;

fn fx(b: &[u8]) -> Fx {
    Fx::from_raw(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn easing(sel: u8, b: &[u8]) -> Easing {
    match sel % 8 {
        0 => Easing::Linear,
        1 => Easing::CubicOut,
        2 => Easing::QuartInOut,
        3 => Easing::BackInOut,
        4 => Easing::BounceInOut,
        5 => Easing::Smoother,
        _ => Easing::Bezier(Bezier::new(fx(&b[..4]), fx(&b[4..8]), fx(&b[8..12]), fx(&b[12..16]))),
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() < 40 {
        return;
    }
    let e = easing(data[0], &data[1..]);
    let t = fx(&data[17..]);
    let v = e.apply(t);
    if t <= Fx::ZERO {
        assert!((v.to_f32()).abs() < 0.01, "{e:?} at {t:?} is {v:?}");
    }
    if t >= Fx::ONE {
        assert!((v.to_f32() - 1.0).abs() < 0.01, "{e:?} at {t:?} is {v:?}");
    }

    let (from, to) = (fx(&data[21..]), fx(&data[25..]));
    let start = u32::from_le_bytes([data[29], data[30], data[31], data[32]]);
    let dur = u32::from_le_bytes([data[33], data[34], data[35], data[36]]);
    let tw = Tween::new(from, to, start, dur, e).delayed(u32::from(data[37]));
    for now in [0, start, start.wrapping_add(dur / 2), start.wrapping_add(dur), u32::MAX] {
        let _ = tw.value(now);
        let _ = tw.is_done(now);
        let _ = tw.retarget(now, to).value(now);
    }

    let kf = Keyframes::new([
        Keyframe::new(u32::from(data[38]) * 7, from, e),
        Keyframe::new(u32::from(data[39]) * 9, to, Easing::Linear),
        Keyframe::new(u32::MAX - u32::from(data[0]), from, Easing::CubicOut),
    ]);
    for now in [0, 50, 500, u32::MAX / 2, u32::MAX] {
        let _ = kf.value(now);
    }

    // Physics: any configuration settles on its target, or keeps swinging inside the range.
    let cfg = SpringConfig::new(fx(&data[1..]), fx(&data[5..]), fx(&data[9..]));
    let mut s = Spring::new(cfg, from);
    s.set_target(to);
    s.impulse(fx(&data[13..]));
    for i in 0..3000u32 {
        let dt = u32::from(data[(i as usize) % data.len()]);
        s.step(dt);
        let _ = (s.position(), s.velocity());
    }
    if s.is_at_rest() {
        assert_eq!(s.position(), s.target());
        assert_eq!(s.velocity(), Fx::ZERO);
    }

    let mut d = Decay::new(from, to, fx(&data[13..]));
    for i in 0..3000u32 {
        d.step(u32::from(data[(i as usize) % data.len()]));
    }
    let _ = d.projected_end();
});
