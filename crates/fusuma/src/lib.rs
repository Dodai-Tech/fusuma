#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

mod anim;
#[cfg(test)]
mod damage;
mod event;
mod layout;
mod paint;
mod theme;
mod tree;

pub use event::Pointer;
pub use theme::{
    ButtonState, ButtonStyle, ButtonVariant, Colors, MotionTimes, Radii, Round, Spacing, Surface,
    TextRole, TextSizes, Theme,
};
pub use tree::{
    Action, ActionKind, Axis, BuildError, CrossAlign, Length, NodeId, SmallText, Style, TEXT_CAP,
    Tree,
};
