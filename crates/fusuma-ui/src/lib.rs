#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

mod anim;
mod app;
#[cfg(test)]
mod damage;
mod dialog;
mod event;
mod focus;
mod gesture;
mod iconed;
mod layout;
mod menu;
mod paint;
mod paragraph;
mod pieces;
mod screen;
#[cfg(test)]
mod scroll;
mod scrollbar;
mod scrollphys;
mod scrollshift;
mod shape;
mod shell;
mod textedit;
mod textinput;
mod theme;
mod toast;
mod transition;
mod tree;
mod widgets;

pub use app::App;
pub use event::Pointer;
pub use focus::Key;
pub use gesture::{LONG_PRESS_MS, SWIPE_PX, Swipe, SwipeDir};
pub use pieces::{MAX_PIECES, Pieces};
pub use screen::{Class, PHONE_BELOW, Screen, TABLET_BELOW, WATCH_BELOW};
pub use shape::{Insets, Shape};
pub use shell::{Clock, Input, Shell, Stats, TouchTracker, deliver, text_inputs, wheel_pixels};
pub use textinput::{Edit, INPUT_CAP, InputText};
pub use theme::{
    Accent, AccentColors, ButtonState, ButtonStyle, ButtonVariant, Colors, Fonts, MotionBudget,
    MotionTimes, Palette, Radii, Round, Spacing, Surface, TextRole, TextSizes, Theme, ThemeError,
};
pub use transition::Edge;
pub use tree::{
    Action, ActionKind, Axis, BuildError, CrossAlign, Length, NodeId, SmallText, Style, TEXT_CAP,
    Tree,
};
