#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

mod anim;
mod app;
#[cfg(test)]
mod damage;
mod event;
mod focus;
mod layout;
mod paint;
mod theme;
mod tree;
mod widgets;

pub use app::App;
pub use event::Pointer;
pub use focus::Key;
pub use theme::{
    ButtonState, ButtonStyle, ButtonVariant, Colors, MotionTimes, Radii, Round, Spacing, Surface,
    TextRole, TextSizes, Theme,
};
pub use tree::{
    Action, ActionKind, Axis, BuildError, CrossAlign, Length, NodeId, SmallText, Style, TEXT_CAP,
    Tree,
};
