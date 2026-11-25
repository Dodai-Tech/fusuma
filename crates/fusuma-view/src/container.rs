//! Views that hold other views: a column, a row, a card, and the two that change how one view behaves.

use fusuma_ui::{BuildError, CrossAlign, Id, Length, NodeId, Style, Surface};

use crate::{Children, Cx, View, ViewMarker};

/// What a container is: a column, a row, or a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Column,
    Row,
    Card,
}

/// A container: children laid out along an axis, with a gap, padding and a size.
#[derive(Clone, Copy, Debug)]
pub struct Container<C> {
    shape: Shape,
    style: Style,
    children: C,
}

/// A container that stacks its children top to bottom.
pub type Column<C> = Container<C>;
/// A container that puts its children side by side.
pub type Row<C> = Container<C>;

/// Children top to bottom.
#[must_use]
pub const fn column<C>(children: C) -> Column<C> {
    Container {
        shape: Shape::Column,
        style: Style::DEFAULT,
        children,
    }
}

/// Children side by side.
#[must_use]
pub const fn row<C>(children: C) -> Row<C> {
    Container {
        shape: Shape::Row,
        style: Style::DEFAULT,
        children,
    }
}

/// A column on the raised, rounded card surface.
#[must_use]
pub const fn card<C>(children: C) -> Container<C> {
    Container {
        shape: Shape::Card,
        style: Style::DEFAULT,
        children,
    }
}

impl<C> Container<C> {
    /// Room around the children, in pixels.
    #[must_use]
    pub fn padding(mut self, px: u32) -> Self {
        self.style = self.style.padding(px);
        self
    }

    /// Room between the children, in pixels.
    #[must_use]
    pub fn gap(mut self, px: u32) -> Self {
        self.style = self.style.gap(px);
        self
    }

    /// The width.
    #[must_use]
    pub fn width(mut self, length: Length) -> Self {
        self.style = self.style.width(length);
        self
    }

    /// The height.
    #[must_use]
    pub fn height(mut self, length: Length) -> Self {
        self.style = self.style.height(length);
        self
    }

    /// How the children line up across the axis.
    #[must_use]
    pub fn align(mut self, align: CrossAlign) -> Self {
        self.style = self.style.align(align);
        self
    }

    /// A background.
    #[must_use]
    pub fn background(mut self, surface: Surface) -> Self {
        self.style = self.style.background(surface);
        self
    }

    /// How round the corners of the background are.
    #[must_use]
    pub fn round(mut self, round: fusuma_ui::Round) -> Self {
        self.style = self.style.round(round);
        self
    }

    /// The style a card has: its own, with the card's surface and corners.
    fn style(&self) -> Style {
        match self.shape {
            Shape::Card => self
                .style
                .background(Surface::Raised)
                .round(fusuma_ui::Round::Medium),
            _ => self.style,
        }
    }
}

impl<M: Clone, C: Children<M>> View<M> for Container<C> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = match self.shape {
            Shape::Row => cx.tree.row(parent, self.style())?,
            Shape::Column | Shape::Card => cx.tree.column(parent, self.style())?,
        };
        self.children.build_all(cx, node)?;
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        cx.tree.set_style(node, self.style());
        self.children.rebuild_all(cx, node);
    }
}

/// A view that is there or not: the node always exists (so the places of the others never move in the tree) and is
/// [hidden](fusuma_ui::Tree::set_hidden) while `when` is false, keeping what it holds.
#[derive(Clone, Copy, Debug)]
pub struct Show<V> {
    when: bool,
    view: V,
}

/// `view` while `when` is true, nothing (and no room) while it is false.
#[must_use]
pub const fn show<V>(when: bool, view: V) -> Show<V> {
    Show { when, view }
}

impl<M: Clone, V: View<M>> View<M> for Show<V> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = self.view.build(cx, parent)?;
        cx.tree.set_hidden(node, !self.when);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.view.rebuild(cx, node);
        cx.tree.set_hidden(node, !self.when);
    }
}

/// A view whose node has a name the app chose (see [`fusuma_ui::Tree::set_id`]), for focus, scroll and tests.
#[derive(Clone, Copy, Debug)]
pub struct Named<V> {
    id: Id,
    view: V,
}

/// `view`, called `id`.
#[must_use]
pub const fn named<V>(id: Id, view: V) -> Named<V> {
    Named { id, view }
}

impl<M: Clone, V: View<M>> View<M> for Named<V> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = self.view.build(cx, parent)?;
        cx.tree.set_id(node, self.id);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.view.rebuild(cx, node);
    }
}

impl<C> ViewMarker for Container<C> {}
impl<V> ViewMarker for Show<V> {}
impl<V> ViewMarker for Named<V> {}

/// A view whose node sits `to` pixels from where its parent puts it, getting there on a spring. When the target changes the
/// toolkit carries the node there with physics, from wherever it is, keeping its speed: the app's `view` runs once for the
/// new target, not every frame. A node that is just built starts at its target.
#[derive(Clone, Copy, Debug)]
pub struct Animated<V> {
    view: V,
    to: (i32, i32),
    spring: fusuma_anim::SpringConfig,
}

/// `view`, moved by `to` on a spring (see [`fusuma_anim::SpringConfig`] for the presets).
#[must_use]
pub const fn animate<V>(view: V, to: (i32, i32), spring: fusuma_anim::SpringConfig) -> Animated<V> {
    Animated { view, to, spring }
}

impl<M: Clone, V: View<M>> View<M> for Animated<V> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = self.view.build(cx, parent)?;
        cx.tree.set_offset(node, self.to);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.view.rebuild(cx, node);
        cx.tree.animate_to(node, self.to, self.spring);
    }
}

impl<V> ViewMarker for Animated<V> {}

/// A view whose node has a style class: a number the theme's hooks map to a look, so a design system restyles a widget by
/// name (`class(button("Delete"), DANGER)`) and the widget itself does not change.
#[derive(Clone, Copy, Debug)]
pub struct Classed<V> {
    class: u8,
    view: V,
}

/// `view`, in style class `class` (0 is the default look).
#[must_use]
pub const fn class<V>(class: u8, view: V) -> Classed<V> {
    Classed { class, view }
}

impl<M: Clone, V: View<M>> View<M> for Classed<V> {
    fn build<const N: usize, const K: usize>(
        &self,
        cx: &mut Cx<'_, M, N, K>,
        parent: Option<NodeId>,
    ) -> Result<NodeId, BuildError> {
        let node = self.view.build(cx, parent)?;
        cx.tree.set_class(node, self.class);
        Ok(node)
    }

    fn rebuild<const N: usize, const K: usize>(&self, cx: &mut Cx<'_, M, N, K>, node: NodeId) {
        self.view.rebuild(cx, node);
        cx.tree.set_class(node, self.class);
    }
}

impl<V> ViewMarker for Classed<V> {}
