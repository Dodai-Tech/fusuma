//! Lists that change, views that are skipped when what they are made from did not change, and a way in for code that
//! builds nodes by hand.

use fusuma_ui::{BuildError, NodeId};

use super::{Cx, Part, View, ViewMarker};

/// A keyed list: one view per item, found again by a key the app chooses, so rows that stay keep their node (their text,
/// focus and scroll), rows that arrive are made, rows that leave are removed and rows that move are moved.
///
/// An `each` must be the **last part** of its container: the rows are all the children from where it starts. Keys must
/// differ within one list; a repeated key is not an error but the rows will not keep their identity.
#[derive(Clone, Copy, Debug)]
pub struct Each<'a, T, F> {
    items: &'a [T],
    key: fn(&T) -> u32,
    row: F,
}

/// A keyed list of `items`, each shown by `row(item)`, identified by `key(item)`.
#[must_use]
pub const fn each<T, V, F: Fn(&T) -> V>(items: &[T], key: fn(&T) -> u32, row: F) -> Each<'_, T, F> {
    Each { items, key, row }
}

impl<M: Clone, T, V: View<M>, F: Fn(&T) -> V> Part<M> for Each<'_, T, F> {
    fn build_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: NodeId,
    ) -> Result<(), BuildError> {
        for item in self.items {
            let node = (self.row)(item).build(cx, Some(parent))?;
            cx.tree.set_key(node, (self.key)(item));
        }
        Ok(())
    }

    fn rebuild_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: NodeId,
        cursor: &mut Option<NodeId>,
    ) {
        // Where the rows start: after what is before the cursor, or at the first child.
        let before = match *cursor {
            Some(c) => cx.tree.nodes_prev(c),
            None => cx.tree.children(parent).last(),
        };
        let start = |cx: &Cx<'_, M, N, K>| match before {
            Some(b) => cx.tree.next_sibling(b),
            None => cx.tree.first_child(parent),
        };
        // Rows whose key is no longer in the list go.
        let mut at = start(cx);
        while let Some(row) = at {
            at = cx.tree.next_sibling(row);
            let key = cx.tree.key_of(row);
            if !self.items.iter().any(|i| (self.key)(i) == key) {
                cx.tree.remove(row);
            }
        }
        // Then each item, in order, against the rows that are left.
        let mut at = start(cx);
        for item in self.items {
            let key = (self.key)(item);
            let mut found = at;
            while let Some(n) = found {
                if cx.tree.key_of(n) == key {
                    break;
                }
                found = cx.tree.next_sibling(n);
            }
            let view = (self.row)(item);
            match found {
                Some(n) if Some(n) == at => {
                    view.rebuild(cx, n);
                    at = cx.tree.next_sibling(n);
                }
                Some(n) => {
                    // A row that moved up.
                    cx.tree.move_before(n, at);
                    view.rebuild(cx, n);
                }
                None => {
                    if let Ok(n) = view.build(cx, Some(parent)) {
                        cx.tree.set_key(n, key);
                        cx.tree.move_before(n, at);
                    }
                }
            }
        }
        *cursor = None;
    }
}

/// A view that is made again only when `value` (a 16-bit version or hash the app computes from what the view shows; two
/// different values that collide are not told apart) is not what it was: while it is the same the whole subtree is skipped, and its messages are kept as they were.
#[derive(Clone, Copy, Debug)]
pub struct Memo<V> {
    value: u16,
    view: V,
}

/// `view`, skipped while `value` stays the same.
#[must_use]
pub const fn memo<V>(value: u16, view: V) -> Memo<V> {
    Memo { value, view }
}

impl<M: Clone, V: View<M>> View<M> for Memo<V> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = self.view.build(cx, parent)?;
        cx.tree.set_memo(node, self.value);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        if cx.tree.memo_of(node) == self.value {
            cx.msgs.carry_over(cx.tree, node);
        } else {
            self.view.rebuild(cx, node);
            cx.tree.set_memo(node, self.value);
        }
    }
}

impl<V> ViewMarker for Memo<V> {}

/// Code that builds and updates a subtree by hand, for what the views do not cover (or for a page that has not been moved
/// over yet). It gets the tree and the table of messages; what it makes is part of the page like any other node.
pub trait Imperative<M: Clone> {
    /// Adds the subtree under `parent` and returns its root.
    ///
    /// # Errors
    /// [`BuildError`] when the tree has no room.
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError>;

    /// Brings the subtree at `node` up to date.
    fn update<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId);
}

/// A view made of hand-written code.
#[derive(Clone, Copy, Debug)]
pub struct Custom<T>(T);

/// `code` as a view.
#[must_use]
pub const fn imperative<T>(code: T) -> Custom<T> {
    Custom(code)
}

impl<M: Clone, T: Imperative<M>> View<M> for Custom<T> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        self.0.build(cx, parent)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.0.update(cx, node);
    }
}

impl<T> ViewMarker for Custom<T> {}
