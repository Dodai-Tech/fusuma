//! The messages of the widgets: which node sends which message. Filled in every time the views are visited, so it is
//! always the table of what is on the screen now and a late event from a node that has gone finds nothing.

use fusuma_ui::{Action, ActionKind, NodeId, SwipeDir, Tree};

/// What a node sends when the user acts on it.
#[derive(Clone, Copy, Debug)]
pub enum Handler<M> {
    /// A press (or a choice made) sends this message.
    Press(M),
    /// A checkbox, radio button or switch sends a message made from its new state.
    Toggle(fn(bool) -> M),
    /// A slider sends a message made from its new value, in thousandths.
    Value(fn(u16) -> M),
    /// A text field sends a message made from its text.
    Text(fn(&str) -> M),
    /// A dialog or menu was dismissed (a press outside, Escape).
    Dismiss(M),
    /// A held press sends this message.
    LongPress(M),
    /// A swipe sends a message made from its direction.
    Swipe(fn(SwipeDir) -> M),
}

/// A fixed table of `(node, handler)`. When it is full the extra handlers are dropped and counted.
#[derive(Clone, Debug)]
pub struct Msgs<M, const K: usize> {
    slots: [Option<(NodeId, Handler<M>)>; K],
    len: usize,
    dropped: u32,
    /// What the table held before the last [`clear`](Self::clear), so a memoised subtree that is not visited again can
    /// keep its handlers.
    before: [Option<(NodeId, Handler<M>)>; K],
    before_len: usize,
}

impl<M: Clone, const K: usize> Msgs<M, K> {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: core::array::from_fn(|_| None),
            len: 0,
            dropped: 0,
            before: core::array::from_fn(|_| None),
            before_len: 0,
        }
    }

    /// Forgets every handler (the views are about to register the current ones).
    pub fn clear(&mut self) {
        core::mem::swap(&mut self.slots, &mut self.before);
        self.before_len = self.len;
        for s in &mut self.slots {
            *s = None;
        }
        self.len = 0;
    }

    /// Keeps the handlers the table had before the last [`clear`](Self::clear) for `node` and everything under it: a
    /// memoised subtree that was not visited this time still answers to the same messages.
    pub fn carry_over<const N: usize>(&mut self, tree: &Tree<N>, node: NodeId) {
        for i in 0..self.before_len {
            if let Some((n, h)) = self.before[i].clone()
                && tree.is_within(node, n)
            {
                self.on(n, h);
            }
        }
    }

    /// Registers `handler` for `node`.
    pub fn on(&mut self, node: NodeId, handler: Handler<M>) {
        match self.slots.get_mut(self.len) {
            Some(slot) => {
                *slot = Some((node, handler));
                self.len += 1;
            }
            None => self.dropped = self.dropped.saturating_add(1),
        }
    }

    /// How many handlers there are.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// How many handlers did not fit since the table was made (a sign `K` is too small).
    #[must_use]
    pub const fn dropped(&self) -> u32 {
        self.dropped
    }

    /// The message an action of the tree stands for, if its node has a handler of the right kind (a node may have
    /// several: a card that takes a swipe and a long press).
    #[must_use]
    pub fn resolve<const N: usize>(&self, tree: &Tree<N>, action: Action) -> Option<M> {
        self.slots[..self.len]
            .iter()
            .flatten()
            .filter(|(n, _)| *n == action.source)
            .find_map(|(_, handler)| match (handler, action.kind) {
                (Handler::Press(m), ActionKind::Clicked | ActionKind::Selected) => Some(m.clone()),
                (Handler::Toggle(f), ActionKind::Toggled(on)) => Some(f(on)),
                (Handler::Value(f), ActionKind::Changed(v)) => Some(f(v)),
                (Handler::Text(f), ActionKind::TextChanged) => Some(f(tree.text(action.source)?)),
                (Handler::Dismiss(m), ActionKind::Dismissed) => Some(m.clone()),
                (Handler::LongPress(m), ActionKind::LongPressed) => Some(m.clone()),
                (Handler::Swipe(f), ActionKind::Swiped(d)) => Some(f(d)),
                _ => None,
            })
    }
}

impl<M: Clone, const K: usize> Default for Msgs<M, K> {
    fn default() -> Self {
        Self::new()
    }
}
