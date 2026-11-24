#![doc = include_str!("../README.md")]
#![no_std]

mod animator;
mod clip;
mod decay;
mod drag;
mod easing;
mod fixed;
mod keyframes;
mod spring;
mod tween;

pub use animator::{Animator, Full, Id};
pub use clip::{Clip, MAX_SEGMENTS};
pub use decay::Decay;
pub use drag::{rubber_band, snap_page};
pub use easing::{Bezier, Easing};
pub use fixed::Fx;
pub use keyframes::{Keyframe, Keyframes};
pub use spring::{Spring, SpringConfig};
pub use tween::{Tween, stagger};
