//! More components made of the views that exist, taken from the Dodai gpui set and the Material 3 list: a top bar, a
//! toolbar, a navigation bar and rail, a form field, a key and value row, a button group, a toggle group, a chip group, a
//! split button, a floating action button, an avatar group, a search bar and an icon with a count.
//!
//! Like [`components`](super::components), none adds a node kind. The ones that take a slice of labels (navigation, toggle
//! and chip groups, avatar group) keep their number of parts for as long as they are shown: it is part of the shape of the
//! page, as it is for a segmented control.

use core::fmt::Write;

use fusuma_ui::{
    BuildError, ButtonVariant, CrossAlign, Length, NodeId, Round, SmallText, Style, Surface,
    TextRole, Tone,
};

use super::{
    Button, Children, Cx, Handler, View, ViewMarker, button, column, icon, row, spacer, text,
    text_field,
};

/// The bar across the top of a screen: a title on the left and, on the right, the actions (icon buttons, a menu).
#[must_use]
pub fn top_bar<'a, M: Clone + 'a, C: Children<M> + 'a>(
    title: &'a str,
    actions: C,
) -> impl View<M> + 'a {
    row((text(title).title(), spacer(), row(actions).gap(4)))
        .padding(8)
        .gap(8)
        .align(CrossAlign::Center)
        .width(Length::Fill(1))
        .background(Surface::Surface)
}

/// A row of tools in a raised strip: buttons, a divider, a switch.
#[must_use]
pub fn toolbar<'a, M: Clone + 'a, C: Children<M> + 'a>(tools: C) -> impl View<M> + 'a {
    row(tools)
        .padding(4)
        .gap(4)
        .align(CrossAlign::Center)
        .background(Surface::Raised)
        .round(Round::Medium)
}

/// A label, the control it names, and under it a hint (a rule, an example) or, when `error` is not empty, the error in the
/// colour of a danger. The hint and the error are one line of text that is kept: it changes, it is never removed.
#[must_use]
pub fn form_field<'a, M: Clone + 'a, V: View<M> + 'a>(
    label: &'a str,
    control: V,
    hint: &'a str,
    error: &'a str,
) -> impl View<M> + 'a {
    column((
        text(label).caption(),
        control,
        row((
            text(if error.is_empty() { hint } else { error }).caption(),
            spacer(),
        ))
        .padding(2)
        .background(if error.is_empty() {
            Surface::Surface
        } else {
            Surface::ToneSoft(Tone::Danger)
        }),
    ))
    .gap(4)
    .align(CrossAlign::Stretch)
}

/// `name` on the left and `value` on the right, one line of a description list (a setting and what it is set to).
#[must_use]
pub fn key_value<'a, M: Clone + 'a>(name: &'a str, value: &'a str) -> impl View<M> + 'a {
    row((text(name).caption(), spacer(), text(value)))
        .gap(8)
        .padding(4)
        .align(CrossAlign::Center)
        .width(Length::Fill(1))
}

/// A number on a small round badge on an icon (a bell with 3 unread). `count` 0 shows the icon alone, kept so the shape does
/// not change; above 99 the badge says `99+`.
#[derive(Clone, Copy, Debug)]
pub struct IconBadge {
    glyph: char,
    count: u16,
}

/// The icon `glyph` with `count` on it.
#[must_use]
pub const fn icon_badge(glyph: char, count: u16) -> IconBadge {
    IconBadge { glyph, count }
}

impl IconBadge {
    fn count_text(self) -> SmallText {
        let mut s = SmallText::EMPTY;
        if self.count > 99 {
            let _ = write!(s, "99+");
        } else if self.count > 0 {
            let _ = write!(s, "{}", self.count);
        }
        s
    }

    fn badge_style(self) -> Style {
        Style::new()
            .padding(if self.count == 0 { 0 } else { 3 })
            .align(CrossAlign::Center)
            .background(if self.count == 0 {
                Surface::Background
            } else {
                Surface::Tone(Tone::Danger)
            })
            .round(Round::Full)
    }
}

impl<M: Clone> View<M> for IconBadge {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let at = cx
            .tree
            .row(parent, Style::new().gap(2).align(CrossAlign::Start))?;
        cx.tree.icon(Some(at), self.glyph)?;
        let badge = cx.tree.row(Some(at), self.badge_style())?;
        cx.tree
            .label(Some(badge), self.count_text().as_str(), TextRole::Caption)?;
        Ok(at)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        let Some(glyph) = cx.tree.first_child(node) else {
            return;
        };
        let mut buf = [0u8; 4];
        cx.tree.set_text(glyph, self.glyph.encode_utf8(&mut buf));
        if let Some(badge) = cx.tree.next_sibling(glyph) {
            cx.tree.set_style(badge, self.badge_style());
            if let Some(l) = cx.tree.first_child(badge) {
                cx.tree.set_text(l, self.count_text().as_str());
            }
        }
    }
}

impl ViewMarker for IconBadge {}

/// A round button with an icon, in the main action's colour: the one thing a screen is for (compose, add). With a `label` it
/// is an extended one.
#[must_use]
pub fn fab<M>(glyph: char, label: &str) -> Button<'_, M> {
    button(label).icon(glyph).primary()
}

/// A button and, beside it, a second one that opens the other choices (a menu): the main action and its alternatives.
#[must_use]
pub fn split_button<'a, M: Clone + 'a>(
    label: &'a str,
    on_press: M,
    on_more: M,
) -> impl View<M> + 'a {
    row((
        button(label).primary().on_press(on_press),
        button("v").primary().on_press(on_more),
    ))
    .gap(1)
}

/// A search field in a pill with the icon `glyph` before it (the font's `search`; a space for none) and `on_submit` for Enter.
#[must_use]
pub fn search_bar<'a, M: Clone + 'a>(
    glyph: char,
    value: &'a str,
    on_input: fn(&str) -> M,
    on_submit: M,
) -> impl View<M> + 'a {
    row((
        icon(glyph),
        text_field(value)
            .placeholder("Search")
            .on_input(on_input)
            .on_submit(on_submit),
    ))
    .gap(6)
    .padding(4)
    .align(CrossAlign::Center)
    .background(Surface::Raised)
    .round(Round::Full)
}

// ── Groups of labels ────────────────────────────────────────────────────────────────────────────────────────────

/// How a [`Choices`] is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Look {
    /// A strip along the bottom of a screen: equal parts, one chosen.
    NavBar,
    /// A column down the side: one chosen.
    Rail,
    /// Joined buttons in one raised box, any number on.
    Toggle,
    /// Separate chips that wrap, any number on.
    Chips,
    /// Joined buttons, none chosen: each is an action.
    Actions,
}

/// Labels with a mark each. It is the one thing behind the navigation bar, the navigation rail, the toggle group, the chip
/// group and the button group: they differ in how they are laid out and whether more than one is on.
#[derive(Clone, Copy, Debug)]
pub struct Choices<'a, M> {
    look: Look,
    labels: &'a [&'a str],
    on: u32,
    press: fn(usize) -> M,
}

impl<M: Clone> Choices<'_, M> {
    /// Whether the parts are buttons (so any number can be on) and not list rows, which are one-of-a-group.
    fn buttons(&self) -> bool {
        matches!(self.look, Look::Toggle | Look::Chips | Look::Actions)
    }

    fn is_on(&self, i: usize) -> bool {
        i < 32 && self.on >> i & 1 == 1
    }

    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        let mut at = cx.tree.first_child(node);
        for (i, label) in self.labels.iter().enumerate() {
            let Some(id) = at else { break };
            cx.tree.set_text(id, label);
            if self.buttons() {
                cx.tree.set_variant(
                    id,
                    if self.is_on(i) {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    },
                );
            } else {
                cx.tree.set_checked(id, self.is_on(i));
            }
            cx.msgs.on(id, Handler::Press((self.press)(i)));
            at = cx.tree.next_sibling(id);
        }
    }
}

impl<M: Clone> View<M> for Choices<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let style = match self.look {
            Look::NavBar => Style::new()
                .padding(4)
                .gap(4)
                .width(Length::Fill(1))
                .align(CrossAlign::Stretch)
                .background(Surface::Surface),
            Look::Rail => Style::new()
                .padding(4)
                .gap(4)
                .align(CrossAlign::Stretch)
                .background(Surface::Surface),
            Look::Toggle | Look::Actions => Style::new()
                .padding(2)
                .gap(2)
                .align(CrossAlign::Stretch)
                .background(Surface::Raised)
                .round(Round::Medium),
            Look::Chips => Style::new().gap(6).width(Length::Fill(1)),
        };
        let bar = match self.look {
            Look::Rail => cx.tree.column(parent, style)?,
            Look::Chips => cx.tree.flow(parent, style)?,
            _ => cx.tree.row(parent, style)?,
        };
        for (i, label) in self.labels.iter().enumerate() {
            let id = if self.buttons() {
                cx.tree.button(Some(bar), label, ButtonVariant::Secondary)?
            } else {
                cx.tree.list_item(Some(bar), label, self.is_on(i))?
            };
            if self.look != Look::Chips && self.look != Look::Rail {
                cx.tree.set_style(id, Style::new().width(Length::Fill(1)));
            }
        }
        self.sync(cx, bar);
        Ok(bar)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Choices<'_, M> {}

/// The marks of a list of flags as the bits the groups take (bit `i` is `flags[i]`; at most 32).
#[must_use]
pub fn marks(flags: &[bool]) -> u32 {
    flags
        .iter()
        .take(32)
        .enumerate()
        .fold(0, |bits, (i, on)| bits | (u32::from(*on) << i))
}

/// A bar along the bottom of a screen with one part for each top-level place; `current` is the place you are in and
/// `on_select` makes the message from the index that was pressed. `current` has the bit of the place you are in set
/// (see [`marks`]).
#[must_use]
pub const fn navigation_bar<'a, M>(
    labels: &'a [&'a str],
    current: u32,
    on_select: fn(usize) -> M,
) -> Choices<'a, M> {
    Choices {
        look: Look::NavBar,
        labels,
        on: current,
        press: on_select,
    }
}

/// The same as a [`navigation_bar`], as a column down the side of a wide screen.
#[must_use]
pub const fn navigation_rail<'a, M>(
    labels: &'a [&'a str],
    current: u32,
    on_select: fn(usize) -> M,
) -> Choices<'a, M> {
    Choices {
        look: Look::Rail,
        labels,
        on: current,
        press: on_select,
    }
}

/// Joined buttons of which any number is on (bold, italic, underline); pressing one sends `on_toggle(index)` and the app
/// flips it in `on`.
#[must_use]
pub const fn toggle_group<'a, M>(
    labels: &'a [&'a str],
    on: u32,
    on_toggle: fn(usize) -> M,
) -> Choices<'a, M> {
    Choices {
        look: Look::Toggle,
        labels,
        on,
        press: on_toggle,
    }
}

/// Chips that wrap onto more lines, any number on (filters, tags to pick).
#[must_use]
pub const fn chip_group<'a, M>(
    labels: &'a [&'a str],
    on: u32,
    on_toggle: fn(usize) -> M,
) -> Choices<'a, M> {
    Choices {
        look: Look::Chips,
        labels,
        on,
        press: on_toggle,
    }
}

/// Joined buttons that are each an action, none of them chosen (Cut, Copy, Paste).
#[must_use]
pub const fn button_group<'a, M>(
    labels: &'a [&'a str],
    on_press: fn(usize) -> M,
) -> Choices<'a, M> {
    Choices {
        look: Look::Actions,
        labels,
        on: 0,
        press: on_press,
    }
}

// ── Avatar group ────────────────────────────────────────────────────────────────────────────────────────────────

/// A few avatars side by side and, when there are more people than fit, a `+3`.
#[derive(Clone, Copy, Debug)]
pub struct AvatarGroup<'a> {
    names: &'a [&'a str],
    size: u32,
    max: usize,
}

/// Avatars for `names` (initials are taken from each), `size` pixels across, at most `max` of them and a `+n` for the rest.
#[must_use]
pub const fn avatar_group<'a>(names: &'a [&'a str], size: u32, max: usize) -> AvatarGroup<'a> {
    AvatarGroup { names, size, max }
}

impl AvatarGroup<'_> {
    fn shown(&self) -> usize {
        self.names.len().min(self.max)
    }

    fn initials(name: &str) -> SmallText {
        let mut s = SmallText::EMPTY;
        for c in name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .take(2)
        {
            let _ = write!(s, "{}", c.to_uppercase());
        }
        s
    }

    fn more(&self) -> SmallText {
        let mut s = SmallText::EMPTY;
        let _ = write!(s, "+{}", self.names.len() - self.shown());
        s
    }
}

impl<M: Clone> View<M> for AvatarGroup<'_> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let at = cx
            .tree
            .row(parent, Style::new().gap(2).align(CrossAlign::Center))?;
        for name in &self.names[..self.shown()] {
            cx.tree
                .avatar(Some(at), Self::initials(name).as_str(), self.size)?;
        }
        if self.names.len() > self.shown() {
            cx.tree.avatar(Some(at), self.more().as_str(), self.size)?;
        }
        Ok(at)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        let mut at = cx.tree.first_child(node);
        let mut texts = self.names[..self.shown()].iter().map(|n| Self::initials(n));
        while let Some(id) = at {
            // The initials are the middle child, between two springs.
            let label = cx
                .tree
                .first_child(id)
                .and_then(|spring| cx.tree.next_sibling(spring));
            let want = texts.next().unwrap_or_else(|| self.more());
            if let Some(l) = label {
                cx.tree.set_text(l, want.as_str());
            }
            at = cx.tree.next_sibling(id);
        }
    }
}

impl ViewMarker for AvatarGroup<'_> {}
