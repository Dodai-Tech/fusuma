#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

mod anim;
mod app;
#[cfg(test)]
mod damage;
mod dialog;
mod event;
mod focus;
mod iconed;
mod layout;
mod menu;
mod paint;
mod paragraph;
mod pieces;
#[cfg(test)]
mod scroll;
mod scrollbar;
mod scrollphys;
mod scrollshift;
mod shape;
mod shell;
mod textinput;
mod theme;
mod toast;
mod tree;
mod widgets;

pub use app::App;
pub use event::Pointer;
pub use focus::Key;
pub use pieces::{MAX_PIECES, Pieces};
pub use shape::{Insets, Shape};
pub use shell::{Clock, Input, Shell, Stats, TouchTracker, deliver, text_inputs, wheel_pixels};
pub use textinput::{Edit, INPUT_CAP, InputText};
pub use theme::{
    Accent, AccentColors, ButtonState, ButtonStyle, ButtonVariant, Colors, Fonts, MotionTimes,
    Palette, Radii, Round, Spacing, Surface, TextRole, TextSizes, Theme, ThemeError,
};
pub use tree::{
    Action, ActionKind, Axis, BuildError, CrossAlign, Length, NodeId, SmallText, Style, TEXT_CAP,
    Tree,
};
