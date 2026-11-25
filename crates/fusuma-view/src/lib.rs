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

/// A description of one node (and what is under it) as a function of the state. A view is a cheap value made on the
/// stack, used once and dropped: **the tree is the previous state**, so there is nothing to keep between passes.
pub trait View<M: Clone> {
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

/// Views side by side: a tuple of views is the children of a container. Each view is one node, in order.
pub trait Children<M: Clone> {
    /// Adds every view's nodes under `parent`.
    ///
    /// # Errors
    /// [`BuildError`] when the tree has no room.
    fn build_all<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: NodeId,
    ) -> Result<(), BuildError>;

    /// Rebuilds each view against the children of `parent`, in order.
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
        impl<M: Clone, $($v: View<M>),+> Children<M> for ($($v,)+) {
            fn build_all<const N: usize, const K: usize>(
                &self,
                cx: &mut Cx<'_, M, N, K>,
                parent: NodeId,
            ) -> Result<(), BuildError> {
                $( self.$i.build(cx, Some(parent))?; )+
                Ok(())
            }

            fn rebuild_all<const N: usize, const K: usize>(
                &self,
                cx: &mut Cx<'_, M, N, K>,
                parent: NodeId,
            ) {
                let mut at = cx.tree.first_child(parent);
                $(
                    let Some(node) = at else { return };
                    self.$i.rebuild(cx, node);
                    at = cx.tree.next_sibling(node);
                )+
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
