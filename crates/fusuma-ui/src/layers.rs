//! The layers of a picture, back to front: what the toolkit draws and the places a host fills itself.
//!
//! The toolkit draws the whole interface as one layer (`LayerKind::Ui`). A host that has content the toolkit cannot
//! draw (a video, a camera, a map, a native widget, a 3D view) puts a *surface* in the tree (`Tree::surface`): a node
//! that takes room like any other and draws nothing. The plan lists the surfaces, in tree order, over the interface layer.
//! After a frame the host fills the surfaces that the frame drew around or over (`LayerPlan::stale`), and fills one
//! that changed by itself after it calls `Tree::surface_changed`, which makes that rectangle part of the next frame's
//! damage. The same plan tells a GPU host which parts it may keep from the last frame.

use fusuma_render::Rect;

use crate::tree::{BuildError, Flags, Kind, NodeId, Style, Tree};

/// The most surfaces a plan lists. More are left out (`LayerPlan::left_out` says how many).
pub const MAX_SURFACES: usize = 8;

/// What a layer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// The interface, drawn by the toolkit.
    Ui,
    /// A place the host fills: the id the app gave it.
    Surface(u32),
}

/// One layer and where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layer {
    /// What the layer is.
    pub kind: LayerKind,
    /// The area it covers, in pixels of the screen.
    pub rect: Rect,
}

/// The layers of the current picture, back to front: the interface first, then the surfaces in tree order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerPlan {
    layers: [Layer; MAX_SURFACES + 1],
    len: usize,
    left_out: usize,
}

impl LayerPlan {
    /// A plan with the interface layer over `screen` and nothing else.
    #[must_use]
    pub const fn ui(screen: Rect) -> Self {
        let empty = Layer {
            kind: LayerKind::Ui,
            rect: screen,
        };
        let mut layers = [empty; MAX_SURFACES + 1];
        layers[0] = empty;
        Self {
            layers,
            len: 1,
            left_out: 0,
        }
    }

    /// Adds a surface over the layers so far. Beyond [`MAX_SURFACES`] it is counted and left out.
    pub fn push_surface(&mut self, id: u32, rect: Rect) {
        if self.len > MAX_SURFACES {
            self.left_out += 1;
            return;
        }
        self.layers[self.len] = Layer {
            kind: LayerKind::Surface(id),
            rect,
        };
        self.len += 1;
    }

    /// The layers, back to front.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers[..self.len]
    }

    /// The surfaces, in tree order.
    pub fn surfaces(&self) -> impl Iterator<Item = &Layer> {
        self.layers().iter().skip(1)
    }

    /// The surfaces a frame that redrew `region` has to be filled again: the interface was drawn around or over them.
    pub fn stale(&self, region: Rect) -> impl Iterator<Item = &Layer> {
        self.surfaces().filter(move |l| {
            let o = l.rect.intersect(&region);
            o.w > 0 && o.h > 0
        })
    }

    /// How many surfaces did not fit.
    #[must_use]
    pub const fn left_out(&self) -> usize {
        self.left_out
    }
}

impl<const N: usize> Tree<N> {
    /// The layers of the picture: the interface over the screen, then every visible surface in tree order, cut to the
    /// screen. A surface inside a hidden node is not listed.
    #[must_use]
    pub fn layers(&self) -> LayerPlan {
        let screen = self
            .root()
            .and_then(|r| self.rect(r))
            .unwrap_or(Rect::new(0, 0, 0, 0));
        let mut plan = LayerPlan::ui(screen);
        self.collect_surfaces(&mut plan, screen);
        plan
    }
}

impl<const N: usize> Tree<N> {
    /// Adds a surface: a place the host fills, named `id` by the app. It takes the room its `style` asks for and draws
    /// nothing; [`layers`](Self::layers) lists where it is.
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn surface(
        &mut self,
        parent: Option<NodeId>,
        id: u32,
        style: Style,
    ) -> Result<NodeId, BuildError> {
        self.add(parent, Kind::Surface { id }, style)
    }

    /// Says that what the host shows in surface `id` changed: its rectangle is part of the next frame, so a host that
    /// fills the stale surfaces after the frame fills this one. Does nothing for a name that is not in the tree.
    pub fn surface_changed(&mut self, id: u32) {
        for i in 0..N {
            if self.nodes[i].alive
                && matches!(self.nodes[i].kind, Kind::Surface { id: s } if s == id)
                && !self.in_hidden(i)
            {
                let rect = self.nodes[i].rect;
                self.add_damage(rect);
                self.request(i, Flags::PAINT);
            }
        }
    }

    /// Adds the visible surfaces to `plan` in tree order (depth first), cut to `screen`.
    pub(crate) fn collect_surfaces(&self, plan: &mut LayerPlan, screen: Rect) {
        let Some(root) = self.root.map(usize::from) else {
            return;
        };
        let mut at = Some(root);
        while let Some(i) = at {
            let n = &self.nodes[i];
            if n.hidden {
                // Skip this subtree: next sibling, or the next one up.
                at = self.next_after(i, root);
                continue;
            }
            if let Kind::Surface { id } = n.kind {
                let r = n.rect.intersect(&screen);
                if !r.is_empty() {
                    plan.push_surface(id, r);
                }
            }
            at = n
                .first
                .map(usize::from)
                .or_else(|| self.next_after(i, root));
        }
    }

    /// The node after `i` once its subtree is done: its next sibling, or the next sibling of the closest ancestor that has
    /// one, never leaving the subtree of `top`.
    fn next_after(&self, mut i: usize, top: usize) -> Option<usize> {
        loop {
            if i == top {
                return None;
            }
            if let Some(n) = self.nodes[i].next {
                return Some(usize::from(n));
            }
            i = usize::from(self.nodes[i].parent?);
        }
    }
}
