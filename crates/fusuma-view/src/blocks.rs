//! Views for the building blocks of `fusuma-ui`: a divider, a section heading, a badge, an avatar and a stepper.

use fusuma_ui::{BuildError, NodeId, SmallText};

use crate::{Cx, Handler, View, ViewMarker};

/// A thin line.
#[derive(Clone, Copy, Debug)]
pub struct Divider;

/// A thin line across the parent.
#[must_use]
pub const fn divider() -> Divider {
    Divider
}

impl<M: Clone> View<M> for Divider {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.divider(parent)
    }

    fn rebuild<const N: usize, const K: usize>(&self, _: &mut Cx<'_, M, N, K>, _: NodeId) {}
}

/// A small heading that starts a group.
#[derive(Clone, Copy, Debug)]
pub struct Section<'a>(&'a str);

/// A small heading.
#[must_use]
pub const fn section(title: &str) -> Section<'_> {
    Section(title)
}

impl<M: Clone> View<M> for Section<'_> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.section(parent, self.0)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_text(node, self.0);
    }
}

/// A short text in a pill.
#[derive(Clone, Copy, Debug)]
pub struct Badge<'a> {
    text: &'a str,
    primary: bool,
}

/// A badge: on the main colour when `primary`, else on the page colour.
#[must_use]
pub const fn badge(text: &str, primary: bool) -> Badge<'_> {
    Badge { text, primary }
}

impl<M: Clone> View<M> for Badge<'_> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.badge(parent, self.text, self.primary)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        if let Some(label) = cx.tree.first_child(node) {
            cx.tree.set_text(label, self.text);
        }
    }
}

/// A circle with initials.
#[derive(Clone, Copy, Debug)]
pub struct Avatar<'a> {
    initials: &'a str,
    size: u32,
}

/// An avatar `size` pixels across.
#[must_use]
pub const fn avatar(initials: &str, size: u32) -> Avatar<'_> {
    Avatar { initials, size }
}

impl<M: Clone> View<M> for Avatar<'_> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        cx.tree.avatar(parent, self.initials, self.size)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        // The initials are the middle child, between two springs.
        if let Some(label) = cx
            .tree
            .first_child(node)
            .and_then(|spring| cx.tree.next_sibling(spring))
        {
            cx.tree.set_text(label, self.initials);
        }
    }
}

/// `[-] value [+]`.
#[derive(Clone, Copy, Debug)]
pub struct Stepper<M> {
    value: SmallText,
    on_dec: Option<M>,
    on_inc: Option<M>,
}

/// A stepper showing `value`.
#[must_use]
pub fn stepper<M>(value: &str) -> Stepper<M> {
    Stepper {
        value: SmallText::new(value),
        on_dec: None,
        on_inc: None,
    }
}

impl<M> Stepper<M> {
    /// Sends `message` when the minus button is pressed.
    #[must_use]
    pub fn on_dec(mut self, message: M) -> Self {
        self.on_dec = Some(message);
        self
    }

    /// Sends `message` when the plus button is pressed.
    #[must_use]
    pub fn on_inc(mut self, message: M) -> Self {
        self.on_inc = Some(message);
        self
    }
}

impl<M: Clone> Stepper<M> {
    fn sync<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, row: NodeId) {
        let minus = cx.tree.first_child(row);
        let value = minus.and_then(|n| cx.tree.next_sibling(n));
        let plus = value.and_then(|n| cx.tree.next_sibling(n));
        if let Some(v) = value {
            cx.tree.set_text(v, self.value.as_str());
        }
        if let (Some(n), Some(m)) = (minus, &self.on_dec) {
            cx.msgs.on(n, Handler::Press(m.clone()));
        }
        if let (Some(n), Some(m)) = (plus, &self.on_inc) {
            cx.msgs.on(n, Handler::Press(m.clone()));
        }
    }
}

impl<M: Clone> View<M> for Stepper<M> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let s = cx.tree.stepper(parent, self.value.as_str())?;
        let row = cx.tree.parent_of(s.minus).ok_or(BuildError::Full)?;
        self.sync(cx, row);
        Ok(row)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.sync(cx, node);
    }
}

impl ViewMarker for Divider {}
impl ViewMarker for Section<'_> {}
impl ViewMarker for Badge<'_> {}
impl ViewMarker for Avatar<'_> {}
impl<M> ViewMarker for Stepper<M> {}
