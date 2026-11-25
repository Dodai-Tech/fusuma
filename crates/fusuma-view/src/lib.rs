#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]

mod blocks;
mod container;
mod host;
mod leaf;
mod msgs;

pub use blocks::{
    Avatar, Badge, Divider, Section, Stepper, avatar, badge, divider, section, stepper,
};
pub use container::{Column, Container, Named, Row, Show, card, column, named, row, show};
pub use host::Host;
pub use leaf::{
    Button, Checkbox, Progress, Spacer, Text, button, checkbox, progress, spacer, text, text_fmt,
};
pub use msgs::{Handler, Msgs};

use fusuma_ui::{BuildError, NodeId, Tree};

/// What a view works on while it builds or rebuilds: the tree, and the table of messages the widgets send.
#[derive(Debug)]
pub struct Cx<'a, M, const N: usize, const K: usize> {
    /// The tree the views make match themselves.
    pub tree: &'a mut Tree<N>,
    /// The messages of the widgets, filled in as the views are visited.
    pub msgs: &'a mut Msgs<M, K>,
}

/// Marks a type as a view. It exists so that `Option` and tuples can be given parts of their own without clashing with
/// the blanket implementation for views (a view is anything with this marker and a [`View`] implementation).
pub trait ViewMarker {}

/// A description of one node (and what is under it) as a function of the state. A view is a cheap value made on the
/// stack, used once and dropped: **the tree is the previous state**, so there is nothing to keep between passes.
pub trait View<M: Clone>: ViewMarker {
    /// Adds the nodes of this view under `parent` and returns the one at its root.
    ///
    /// # Errors
    /// [`BuildError`] when the tree has no room.
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError>;

    /// Makes the node `node` (which an earlier [`build`](Self::build) of the same view made) match this view: every
    /// setter compares first, so only what differs changes and asks for layout or drawing.
    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId);
}

/// A piece of a container's children: a view (one node) or an `Option` of one (a node, or none).
///
/// An `Option` has to stay `Some` or stay `None` for as long as its [`Host`] lives: it chooses the *shape* of the page
/// (a title on a big screen and none on a watch), not what is on show now. For that, use [`show`], which keeps the node
/// and hides it.
pub trait Part<M: Clone> {
    /// Adds this part's node, if it has one, under `parent`.
    ///
    /// # Errors
    /// [`BuildError`] when the tree has no room.
    fn build_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: NodeId,
    ) -> Result<(), BuildError>;

    /// Rebuilds this part against the node at `cursor`, and moves the cursor past the node it used (not at all when the
    /// part has none).
    fn rebuild_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        cursor: &mut Option<NodeId>,
    );
}

impl<M: Clone, V: View<M>> Part<M> for V {
    fn build_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: NodeId,
    ) -> Result<(), BuildError> {
        self.build(cx, Some(parent)).map(|_| ())
    }

    fn rebuild_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        cursor: &mut Option<NodeId>,
    ) {
        if let Some(node) = *cursor {
            self.rebuild(cx, node);
            *cursor = cx.tree.next_sibling(node);
        }
    }
}

impl<M: Clone, V: View<M>> Part<M> for Option<V> {
    fn build_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: NodeId,
    ) -> Result<(), BuildError> {
        match self {
            Some(v) => v.build(cx, Some(parent)).map(|_| ()),
            None => Ok(()),
        }
    }

    fn rebuild_part<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        cursor: &mut Option<NodeId>,
    ) {
        if let Some(v) = self {
            v.rebuild_part(cx, cursor);
        }
    }
}

/// Parts side by side: a tuple of them is the children of a container, in order.
pub trait Children<M: Clone> {
    /// Adds every part's nodes under `parent`.
    ///
    /// # Errors
    /// [`BuildError`] when the tree has no room.
    fn build_all<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: NodeId,
    ) -> Result<(), BuildError>;

    /// Rebuilds each part against the children of `parent`, in order.
    fn rebuild_all<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, parent: NodeId);
}

impl<M: Clone> Children<M> for () {
    fn build_all<const N: usize, const K: usize>(
        &self,
        _: &mut Cx<'_, M, N, K>,
        _: NodeId,
    ) -> Result<(), BuildError> {
        Ok(())
    }

    fn rebuild_all<const N: usize, const K: usize>(&self, _: &mut Cx<'_, M, N, K>, _: NodeId) {}
}

macro_rules! tuple_children {
    ($(($($v:ident $i:tt),+))+) => {$(
        impl<M: Clone, $($v: Part<M>),+> Children<M> for ($($v,)+) {
            fn build_all<const N: usize, const K: usize>(
                &self,
                cx: &mut Cx<'_, M, N, K>,
                parent: NodeId,
            ) -> Result<(), BuildError> {
                $( self.$i.build_part(cx, parent)?; )+
                Ok(())
            }

            fn rebuild_all<const N: usize, const K: usize>(
                &self,
                cx: &mut Cx<'_, M, N, K>,
                parent: NodeId,
            ) {
                let mut at = cx.tree.first_child(parent);
                $( self.$i.rebuild_part(cx, &mut at); )+
                let _ = at;
            }
        }
    )+};
}

tuple_children! {
    (A 0)
    (A 0, B 1)
    (A 0, B 1, C 2)
    (A 0, B 1, C 2, D 3)
    (A 0, B 1, C 2, D 3, E 4)
    (A 0, B 1, C 2, D 3, E 4, F 5)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, T10 10)
    (A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, T10 10, T11 11)
}
