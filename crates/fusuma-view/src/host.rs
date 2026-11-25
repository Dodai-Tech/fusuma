//! Where views meet a tree: a host owns a place in the tree, makes the views' nodes match each time it is given a view,
//! and turns the actions of those nodes into messages.

use fusuma_ui::{BuildError, NodeId, Tree};

use crate::{Children, Cx, Msgs, View};

/// A view, kept in step with a part of a tree.
#[derive(Clone, Debug)]
pub struct Host<M, const K: usize = 32> {
    msgs: Msgs<M, K>,
    root: Option<NodeId>,
}

impl<M: Clone, const K: usize> Host<M, K> {
    /// A host with nothing built yet.
    #[must_use]
    pub fn new() -> Self {
        Self {
            msgs: Msgs::new(),
            root: None,
        }
    }

    /// Makes the tree match `view` under `parent`: built the first time, then only changed. Every pass fills the table
    /// of messages again, so it is always the table of what is on the screen now.
    ///
    /// # Errors
    /// [`BuildError`] when the tree has no room.
    pub fn sync<const N: usize, V: View<M>>(
        &mut self,
        tree: &mut Tree<N>,
        parent: Option<NodeId>,
        view: &V,
    ) -> Result<(), BuildError> {
        self.msgs.clear();
        let mut cx = Cx {
            tree,
            msgs: &mut self.msgs,
        };
        match self.root.filter(|&r| cx.tree.contains(r)) {
            Some(root) => view.rebuild(&mut cx, root),
            None => self.root = Some(view.build(&mut cx, parent)?),
        }
        Ok(())
    }

    /// Like [`sync`](Self::sync) for several views that go straight under `parent`, with no node of their own around them: the
    /// children of a page container, say. `parent` must hold nothing else.
    ///
    /// # Errors
    /// [`BuildError`] when the tree has no room.
    pub fn sync_children<const N: usize, C: Children<M>>(
        &mut self,
        tree: &mut Tree<N>,
        parent: NodeId,
        children: &C,
    ) -> Result<(), BuildError> {
        self.msgs.clear();
        let mut cx = Cx {
            tree,
            msgs: &mut self.msgs,
        };
        if self.root == Some(parent) && cx.tree.contains(parent) {
            children.rebuild_all(&mut cx, parent);
        } else {
            self.root = Some(parent);
            children.build_all(&mut cx, parent)?;
        }
        Ok(())
    }

    /// Forgets the built nodes (the page they were on was removed): the next [`sync`](Self::sync) builds again.
    pub fn forget(&mut self) {
        self.root = None;
        self.msgs.clear();
    }

    /// The message an action stands for, if one of this host's widgets sent it: pop actions from the tree and ask this
    /// for each; one that is not answered is for the code around the host.
    #[must_use]
    pub fn message<const N: usize>(&self, tree: &Tree<N>, action: fusuma_ui::Action) -> Option<M> {
        self.msgs.resolve(tree, action)
    }

    /// The table of messages, for a test or a debugger.
    #[must_use]
    pub const fn msgs(&self) -> &Msgs<M, K> {
        &self.msgs
    }

    /// The node at the root of the built view.
    #[must_use]
    pub const fn root(&self) -> Option<NodeId> {
        self.root
    }
}

impl<M: Clone, const K: usize> Default for Host<M, K> {
    fn default() -> Self {
        Self::new()
    }
}
