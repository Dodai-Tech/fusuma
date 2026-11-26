//! Components made of the views that exist: nothing here adds a node kind, an arrangement of widgets is a component. Taken
//! from the Material 3 and Dodai lists (alert, banner, tag, chip, segmented control, breadcrumbs, pagination, steps, stat
//! card, empty state, skeleton, snackbar), each as small as it can be and each the same on every size of screen.
//!
//! A message type `M` is only named by the handlers a component has; the ones with none work for every `M`.

use core::fmt::Write;

use fusuma_ui::{
    BuildError, CrossAlign, Length, NodeId, Round, SmallText, Style, Surface, TextRole, Tone,
};

use super::{Button, Cx, Handler, View, ViewMarker, button, card, column, row, spacer, text};

/// A message in a card with a bar of its tone down the side: a title and a line under it.
#[must_use]
pub fn alert<'a, M: Clone + 'a>(tone: Tone, title: &'a str, body: &'a str) -> impl View<M> + 'a {
    row((
        row(())
            .width(Length::Fixed(4))
            .height(Length::Fill(1))
            .background(Surface::Tone(tone)),
        column((text(title), text(body).caption())).gap(2),
    ))
    .padding(8)
    .gap(8)
    .align(CrossAlign::Stretch)
    .background(Surface::ToneSoft(tone))
    .round(Round::Medium)
}

/// A solid strip of a tone with one line on it (text in black or white, whichever reads).
#[must_use]
pub fn banner<'a, M: Clone + 'a>(tone: Tone, message: &'a str) -> impl View<M> + 'a {
    row((text(message),))
        .padding(8)
        .align(CrossAlign::Center)
        .background(Surface::Tone(tone))
        .round(Round::Medium)
}

/// A short word in a pill of a tone: a status, a count, a label.
#[must_use]
pub fn tag<'a, M: Clone + 'a>(tone: Tone, label: &'a str) -> impl View<M> + 'a {
    row((text(label).caption(),))
        .padding(4)
        .align(CrossAlign::Center)
        .background(Surface::Tone(tone))
        .round(Round::Full)
}

/// A number with its name and, under it, how it changed (a tag in the tone of the change).
#[must_use]
pub fn stat_card<'a, M: Clone + 'a>(
    label: &'a str,
    value: &'a str,
    change: &'a str,
    tone: Tone,
) -> impl View<M> + 'a {
    card((
        text(label).caption(),
        text(value).title(),
        tag(tone, change),
    ))
    .padding(8)
    .gap(4)
}

/// What to show when there is nothing to show: a title and a sentence, centred; put an action under it in the same column.
#[must_use]
pub fn empty_state<'a, M: Clone + 'a>(title: &'a str, hint: &'a str) -> impl View<M> + 'a {
    column((text(title).title(), text(hint).caption()))
        .gap(4)
        .padding(8)
        .align(CrossAlign::Center)
        .width(Length::Fill(1))
}

/// A grey bar where something will load (a line of text is about `height` 12). Still, so it costs an e-paper panel nothing.
#[must_use]
pub fn skeleton<'a, M: Clone + 'a>(width: Length, height: u32) -> impl View<M> + 'a {
    row(())
        .width(width)
        .height(Length::Fixed(height))
        .background(Surface::Raised)
        .round(Round::Small)
}

/// A line of text and the action it offers (undo, retry) in one strip.
#[must_use]
pub fn snackbar<'a, M: Clone + 'a>(
    message: &'a str,
    action: &'a str,
    on_action: M,
) -> impl View<M> + 'a {
    row((text(message), spacer(), button(action).on_press(on_action)))
        .padding(8)
        .gap(8)
        .align(CrossAlign::Center)
        .background(Surface::Raised)
        .round(Round::Medium)
}

/// A choice that can be switched on and off: a filter, a tag to pick. It is a button that is filled with the main
/// action's colour while `selected` and plain otherwise, and sends what `.on_press` says; the app flips its state (a row
/// of them can have any number chosen, which the list rows and tabs, one-of-a-group, cannot).
#[must_use]
pub fn chip<M>(label: &str, selected: bool) -> Button<'_, M> {
    let b = button(label);
    if selected { b.primary() } else { b }
}

// ── Segmented control ───────────────────────────────────────────────────────────────────────────────────────────

/// One of a few choices side by side in one box, the chosen one marked.
#[derive(Clone, Copy, Debug)]
pub struct Segmented<'a, M> {
    labels: &'a [&'a str],
    selected: usize,
    on_select: fn(usize) -> M,
}

/// A segmented control: `labels`, the chosen index, and the message made from the index that was pressed. The number of
/// labels is part of the shape of the page: it stays the same for as long as the control is shown.
#[must_use]
pub const fn segmented<'a, M>(
    labels: &'a [&'a str],
    selected: usize,
    on_select: fn(usize) -> M,
) -> Segmented<'a, M> {
    Segmented {
        labels,
        selected,
        on_select,
    }
}

impl<M: Clone> Segmented<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        let mut at = cx.tree.first_child(node);
        for (i, label) in self.labels.iter().enumerate() {
            let Some(id) = at else { break };
            cx.tree.set_text(id, label);
            cx.tree.set_checked(id, i == self.selected);
            cx.msgs.on(id, Handler::Press((self.on_select)(i)));
            at = cx.tree.next_sibling(id);
        }
    }
}

impl<M: Clone> View<M> for Segmented<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let bar = cx.tree.row(
            parent,
            Style::new()
                .padding(2)
                .gap(2)
                .align(CrossAlign::Stretch)
                .background(Surface::Raised)
                .round(Round::Medium),
        )?;
        for (i, label) in self.labels.iter().enumerate() {
            let id = cx.tree.list_item(Some(bar), label, i == self.selected)?;
            cx.tree.set_style(id, Style::new().width(Length::Fill(1)));
        }
        self.sync(cx, bar);
        Ok(bar)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Segmented<'_, M> {}

// ── Breadcrumbs ─────────────────────────────────────────────────────────────────────────────────────────────────

/// Where you are: `Home / Settings / Display`, every part but the last one pressable.
#[derive(Clone, Copy, Debug)]
pub struct Breadcrumbs<'a, M> {
    parts: &'a [&'a str],
    on_press: fn(usize) -> M,
}

/// Breadcrumbs for `parts`, sending the index of the part that was pressed. The last part is where you are: plain text.
#[must_use]
pub const fn breadcrumbs<'a, M>(
    parts: &'a [&'a str],
    on_press: fn(usize) -> M,
) -> Breadcrumbs<'a, M> {
    Breadcrumbs { parts, on_press }
}

impl<M: Clone> Breadcrumbs<'_, M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        let mut at = cx.tree.first_child(node);
        for (i, part) in self.parts.iter().enumerate() {
            if i > 0 {
                // The separator.
                at = at.and_then(|s| cx.tree.next_sibling(s));
            }
            let Some(id) = at else { break };
            cx.tree.set_text(id, part);
            if i + 1 < self.parts.len() {
                cx.msgs.on(id, Handler::Press((self.on_press)(i)));
            }
            at = cx.tree.next_sibling(id);
        }
    }
}

impl<M: Clone> View<M> for Breadcrumbs<'_, M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let bar = cx
            .tree
            .row(parent, Style::new().gap(4).align(CrossAlign::Center))?;
        for (i, part) in self.parts.iter().enumerate() {
            if i > 0 {
                cx.tree.label(Some(bar), "/", TextRole::Caption)?;
            }
            if i + 1 < self.parts.len() {
                cx.tree.list_item(Some(bar), part, false)?;
            } else {
                cx.tree.label(Some(bar), part, TextRole::Body)?;
            }
        }
        self.sync(cx, bar);
        Ok(bar)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Breadcrumbs<'_, M> {}

// ── Pagination ──────────────────────────────────────────────────────────────────────────────────────────────────

/// How many page buttons a pagination has between its arrows.
pub const PAGE_SLOTS: usize = 7;

/// What one of the page buttons shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageSlot {
    /// A page, counted from 1.
    Page(u32),
    /// A gap: pages left out.
    Gap,
    /// Nothing (fewer pages than buttons).
    Empty,
}

/// The [`PAGE_SLOTS`] buttons for page `current` of `total` (both counted from 1; out-of-range values are held in range):
/// every page when they fit, else the first, the last, the neighbours of the current one and a gap where pages are left
/// out, the current one never at the edge of a gap.
#[must_use]
pub fn page_window(current: u32, total: u32) -> [PageSlot; PAGE_SLOTS] {
    use PageSlot::{Empty, Gap, Page};
    let total = total.max(1);
    let c = current.clamp(1, total);
    let mut out = [Empty; PAGE_SLOTS];
    let n = PAGE_SLOTS as u32;
    if total <= n {
        for (i, slot) in out.iter_mut().enumerate().take(total as usize) {
            *slot = Page(i as u32 + 1);
        }
    } else if c <= n - 3 {
        // Near the start: 1 2 3 4 5 … last.
        for (i, slot) in out.iter_mut().enumerate().take(PAGE_SLOTS - 2) {
            *slot = Page(i as u32 + 1);
        }
        out[PAGE_SLOTS - 2] = Gap;
        out[PAGE_SLOTS - 1] = Page(total);
    } else if c > total - (n - 3) {
        // Near the end: 1 … last-4 … last.
        out[0] = Page(1);
        out[1] = Gap;
        for (i, slot) in out.iter_mut().enumerate().skip(2) {
            *slot = Page(total - (n - 3) + (i as u32 - 2));
        }
    } else {
        // In the middle: 1 … c-1 c c+1 … last.
        out = [
            Page(1),
            Gap,
            Page(c - 1),
            Page(c),
            Page(c + 1),
            Gap,
            Page(total),
        ];
    }
    out
}

/// Previous, a window of page numbers, next.
#[derive(Clone, Copy, Debug)]
pub struct Pagination<M> {
    current: u32,
    total: u32,
    on_page: fn(u32) -> M,
}

/// Pagination for page `current` of `total` (from 1), sending the page that was pressed (the arrows send the one before
/// and after, and cannot be pressed at the ends).
#[must_use]
pub const fn pagination<M>(current: u32, total: u32, on_page: fn(u32) -> M) -> Pagination<M> {
    Pagination {
        current,
        total,
        on_page,
    }
}

impl<M: Clone> Pagination<M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        let total = self.total.max(1);
        let c = self.current.clamp(1, total);
        let Some(prev) = cx.tree.first_child(node) else {
            return;
        };
        cx.tree.set_enabled(prev, c > 1);
        cx.msgs.on(
            prev,
            Handler::Press((self.on_page)(c.saturating_sub(1).max(1))),
        );
        let mut at = cx.tree.next_sibling(prev);
        for slot in page_window(c, total) {
            let Some(id) = at else { return };
            let mut label = SmallText::EMPTY;
            match slot {
                PageSlot::Page(p) => {
                    let _ = write!(label, "{p}");
                    cx.tree.set_hidden(id, false);
                    cx.tree.set_enabled(id, true);
                    cx.tree.set_variant(
                        id,
                        if p == c {
                            fusuma_ui::ButtonVariant::Primary
                        } else {
                            fusuma_ui::ButtonVariant::Secondary
                        },
                    );
                    cx.msgs.on(id, Handler::Press((self.on_page)(p)));
                }
                PageSlot::Gap => {
                    let _ = write!(label, "...");
                    cx.tree.set_hidden(id, false);
                    cx.tree.set_enabled(id, false);
                    cx.tree.set_variant(id, fusuma_ui::ButtonVariant::Secondary);
                }
                PageSlot::Empty => cx.tree.set_hidden(id, true),
            }
            if slot != PageSlot::Empty {
                cx.tree.set_text(id, label.as_str());
            }
            at = cx.tree.next_sibling(id);
        }
        if let Some(next) = at {
            cx.tree.set_enabled(next, c < total);
            cx.msgs
                .on(next, Handler::Press((self.on_page)((c + 1).min(total))));
        }
    }
}

impl<M: Clone> View<M> for Pagination<M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let bar = cx
            .tree
            .row(parent, Style::new().gap(4).align(CrossAlign::Center))?;
        cx.tree
            .button(Some(bar), "<", fusuma_ui::ButtonVariant::Secondary)?;
        for _ in 0..PAGE_SLOTS {
            cx.tree
                .button(Some(bar), "1", fusuma_ui::ButtonVariant::Secondary)?;
        }
        cx.tree
            .button(Some(bar), ">", fusuma_ui::ButtonVariant::Secondary)?;
        self.sync(cx, bar);
        Ok(bar)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl<M> ViewMarker for Pagination<M> {}

// ── Steps ───────────────────────────────────────────────────────────────────────────────────────────────────────

/// How far along a step is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    /// Finished.
    Done,
    /// The one being done.
    Current,
    /// Not yet.
    Todo,
}

/// The state of step `index` when the current one is `current` (counted from 0).
#[must_use]
pub const fn step_state(index: usize, current: usize) -> StepState {
    if index < current {
        StepState::Done
    } else if index == current {
        StepState::Current
    } else {
        StepState::Todo
    }
}

/// Steps of a process in a line: a numbered circle and a name for each, a rule between them.
#[derive(Clone, Copy, Debug)]
pub struct Steps<'a> {
    labels: &'a [&'a str],
    current: usize,
}

/// Steps named `labels`, the current one marked (earlier ones are done).
#[must_use]
pub const fn steps<'a>(labels: &'a [&'a str], current: usize) -> Steps<'a> {
    Steps { labels, current }
}

impl Steps<'_> {
    fn surface(state: StepState) -> Surface {
        match state {
            StepState::Done | StepState::Current => Surface::Primary,
            StepState::Todo => Surface::Raised,
        }
    }

    fn sync<M: Clone, const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        node: NodeId,
    ) {
        let mut at = cx.tree.first_child(node);
        for (i, name) in self.labels.iter().enumerate() {
            let state = step_state(i, self.current);
            // The circle (a number, or a tick when done), the name, and the rule that follows.
            let Some(circle) = at else { break };
            let style = Style::new()
                .padding(6)
                .align(CrossAlign::Center)
                .background(Self::surface(state))
                .round(Round::Full);
            cx.tree.set_style(circle, style);
            if let Some(num) = cx.tree.first_child(circle) {
                let mut s = SmallText::EMPTY;
                if state == StepState::Done {
                    let _ = write!(s, "+");
                } else {
                    let _ = write!(s, "{}", i + 1);
                }
                cx.tree.set_text(num, s.as_str());
            }
            let label = cx.tree.next_sibling(circle);
            if let Some(l) = label {
                cx.tree.set_text(l, name);
            }
            at = label.and_then(|l| cx.tree.next_sibling(l));
            if i + 1 < self.labels.len() {
                at = at.and_then(|rule| cx.tree.next_sibling(rule));
            }
        }
    }
}

impl<M: Clone> View<M> for Steps<'_> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let bar = cx
            .tree
            .row(parent, Style::new().gap(6).align(CrossAlign::Center))?;
        for (i, name) in self.labels.iter().enumerate() {
            let circle = cx.tree.row(Some(bar), Style::new())?;
            cx.tree.label(Some(circle), "1", TextRole::Caption)?;
            cx.tree.label(Some(bar), name, TextRole::Caption)?;
            if i + 1 < self.labels.len() {
                cx.tree.row(
                    Some(bar),
                    Style::new()
                        .width(Length::Fill(1))
                        .height(Length::Fixed(1))
                        .background(Surface::Border),
                )?;
            }
        }
        self.sync::<M, N, K>(cx, bar);
        Ok(bar)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync::<M, N, K>(cx, node);
    }
}

impl ViewMarker for Steps<'_> {}
