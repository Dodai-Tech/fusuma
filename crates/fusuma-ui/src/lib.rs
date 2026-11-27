#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

mod anim;
mod app;
mod blocks;
mod cells;
mod cyberdeck;
#[cfg(test)]
mod damage;
mod decor;
mod dialog;
mod dnd;
mod event;
mod focus;
mod gauge;
mod gesture;
mod hidden;
mod iconed;
mod ids;
mod layout;
mod menu;
mod order;
mod paint;
mod paragraph;
mod pieces;
mod screen;
#[cfg(test)]
mod scroll;
mod scrollbar;
mod scrollphys;
mod scrollshift;
mod semantic;
mod shape;
mod shell;
mod springs;
mod textedit;
mod textinput;
mod theme;
mod toast;
mod transition;
mod tree;
mod virtual_list;
mod wake;
mod widgets;

pub use app::App;
pub use blocks::Stepper;
pub use cells::{CellMeasure, CellSink, CellTheme, CellThemeError, Emphasis, Glyphs};
pub use decor::Decor;
pub use event::Pointer;
pub use focus::Key;
pub use gesture::{LONG_PRESS_MS, SWIPE_PX, Swipe, SwipeDir};
pub use ids::{Id, MAX_IDS};
pub use pieces::{MAX_PIECES, Pieces};
pub use screen::{
    Class, DisplayKind, FormFactor, Inputs, MICRO_BELOW, Metrics, PHONE_BELOW, PHONE_SHORT_SIDE_MM,
    Screen, SizeClass, TABLET_BELOW, WATCH_BELOW, WATCH_SHORT_SIDE_MM,
};
pub use semantic::{Assist, Assisted, Role, Semantic};
pub use shape::{Insets, Shape};
pub use shell::{Clock, Input, Shell, Stats, TouchTracker, deliver, text_inputs, wheel_pixels};
pub use springs::MAX_SPRINGS;
pub use textinput::{Edit, INPUT_CAP, InputText};
pub use theme::{
    Accent, AccentColors, ButtonState, ButtonStyle, ButtonVariant, Class as StyleClass, Colors,
    Fonts, Hooks, ItemStyle, MotionBudget, MotionTimes, Palette, PanelKind, PanelStyle, Radii,
    Round, Spacing, Surface, TabStyle, TextRole, TextSizes, Theme, ThemeError, Tone, mix,
};
pub use transition::Edge;
pub use tree::{
    Action, ActionKind, Axis, BuildError, CHART_MAX, ChartStyle, CrossAlign, Length, NodeId,
    SmallText, Style, TEXT_CAP, Tree,
};
pub use virtual_list::Window;
pub use wake::Wake;
