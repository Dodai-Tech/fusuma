#![doc = include_str!("../README.md")]
#![no_std]

mod decay;
mod easing;
mod fixed;
mod keyframes;
mod spring;
mod tween;

pub use decay::Decay;
pub use easing::{Bezier, Easing};
pub use fixed::Fx;
pub use keyframes::{Keyframe, Keyframes};
pub use spring::{Spring, SpringConfig};
pub use tween::{Tween, stagger};
