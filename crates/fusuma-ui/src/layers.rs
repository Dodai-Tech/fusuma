//! The layers of a picture, back to front: what the toolkit draws and the places a host fills itself.
//!
//! The toolkit draws the interface as one layer (`LayerKind::Ui`). A host that has content the toolkit cannot draw (a
//! video, a camera, a map, a native widget, a 3D view) puts a *surface* in the tree (`Tree::surface`): a node that takes
//! room like any other and draws nothing itself. The plan lists, back to front, the interface and then each surface in
//! tree order (`LayerKind::Surface`), each followed by what the app put *inside* it (`LayerKind::Over`: the controls of a
//! player, a caption), drawn over what the host shows. A surface inside another one's children comes after the outer one
//! and what is over it, so it is over them.
//!
//! A host that fills surfaces calls `Tree::set_host_surfaces(true)`. After the interface is drawn it goes through the
//! plan: it fills a surface when the frame drew around or over it (or something before it in the plan was filled), puts
//! the picture in, and draws what is over it (`Tree::paint_over`); at the end what must be above everything (dialogs,
//! toasts, a drag) is drawn again over the filled places (`Tree::paint_overlays`). A surface that changed by itself calls
//! `Tree::surface_changed`, which makes its rectangle part of the next frame's damage. A host that does not fill surfaces
//! leaves the flag off, and what is inside a surface is drawn in tree order with the rest.

use fusuma_render::Rect;

use crate::tree::{BuildError, Flags, Kind, NodeId, Style, Tree};

/// The most surfaces a plan lists. More are left out (`LayerPlan::left_out` says how many).
pub const MAX_SURFACES: usize = 8;

/// The most layers a plan holds: the interface, and a surface and what is over it for each.
const MAX_LAYERS: usize = 2 * MAX_SURFACES + 1;

/// What a layer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    /// The interface, drawn by the toolkit.
    Ui,
    /// A place the host fills: the id the app gave it.
    Surface(u32),
    /// What the app drew inside surface `id`, over what the host shows there (empty when the surface has no children).
    Over(u32),
}

/// One layer and where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layer {
    /// What the layer is.
    pub kind: LayerKind,
    /// Where the node is, in pixels of the screen. Not cut: a surface half out of the screen or scrolled partly out of its
    /// list keeps its size, so a picture made for it is not squeezed. For an `Over` layer it is the same as `visible`.
    pub rect: Rect,
    /// The part of `rect` that can be seen: cut to the screen and to the scrolling lists it is in. A host draws only
    /// inside it.
    pub visible: Rect,
}

/// The layers of the current picture, back to front: the interface first, then the surfaces in tree order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LayerPlan {
    layers: [Layer; MAX_LAYERS],
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
            visible: screen,
        };
        let layers = [empty; MAX_LAYERS];
        Self {
            layers,
            len: 1,
            left_out: 0,
        }
    }

    /// Adds a surface over the layers so far, and over it what the app put inside it when `over` is set. Beyond
    /// [`MAX_SURFACES`] it is counted and left out.
    pub fn push_surface(&mut self, id: u32, rect: Rect, visible: Rect, over: bool) {
        if self.surfaces().count() >= MAX_SURFACES {
            self.left_out += 1;
            return;
        }
        self.layers[self.len] = Layer {
            kind: LayerKind::Surface(id),
            rect,
            visible,
        };
        self.len += 1;
        if over {
            self.layers[self.len] = Layer {
                kind: LayerKind::Over(id),
                rect: visible,
                visible,
            };
            self.len += 1;
        }
    }

    /// The layers, back to front.
    #[must_use]
    pub fn layers(&self) -> &[Layer] {
        &self.layers[..self.len]
    }

    /// The surfaces, in tree order.
    pub fn surfaces(&self) -> impl Iterator<Item = &Layer> {
        self.layers()
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Surface(_)))
    }

    /// The layers over a surface (what the app put inside it), in tree order.
    pub fn overs(&self) -> impl Iterator<Item = &Layer> {
        self.layers()
            .iter()
            .filter(|l| matches!(l.kind, LayerKind::Over(_)))
    }

    /// The surfaces a frame that redrew `region` has to be filled again: the interface was drawn around or over them.
    pub fn stale(&self, region: Rect) -> impl Iterator<Item = &Layer> {
        self.surfaces().filter(move |l| {
            let o = l.visible.intersect(&region);
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
    /// The layers of the picture: the interface over the screen, then every visible surface in tree order (see [`Layer`] for what is cut). A surface inside a hidden node is not listed.
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

    /// Says whether a host fills the surfaces and calls [`paint_over`](Self::paint_over) and [`paint_overlays`](Self::paint_overlays) after each frame. When it does,
    /// what is inside a surface is left out of the normal pass (the host's pixels would cover it); when it does not (the
    /// default), it is drawn in tree order with the rest.
    pub fn set_host_surfaces(&mut self, on: bool) {
        if self.host_surfaces != on {
            self.host_surfaces = on;
            if let Some(r) = self.root.map(usize::from) {
                let rect = self.nodes[r].rect;
                self.add_damage(rect);
                self.request(r, Flags::PAINT);
            }
        }
    }

    /// Draws what is inside surface `id` (not what is inside a surface nested in it) over what the host put there, inside
    /// `region` and inside the edges of the lists it scrolls in. Call it after the surface was filled.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn paint_over<R: fusuma_render::Renderer>(
        &self,
        r: &mut R,
        theme: &crate::theme::Theme,
        id: u32,
        region: Rect,
    ) -> Result<(), R::Error> {
        if !self.host_surfaces {
            return Ok(());
        }
        for i in 0..N {
            let n = &self.nodes[i];
            if !n.alive
                || n.first.is_none()
                || !matches!(n.kind, Kind::Surface { id: s } if s == id)
                || self.in_hidden(i)
            {
                continue;
            }
            let seen = self.visible_rect(i).intersect(&region);
            if seen.is_empty() {
                continue;
            }
            r.save()?;
            let done = r
                .clip(seen)
                .and_then(|()| self.paint_walk(i, r, theme, Some(seen)));
            let back = r.restore();
            done?;
            back?;
        }
        Ok(())
    }

    /// Draws again what is above everything else (open dialogs and menus, toasts, a dragged item) inside `region`. The host
    /// calls it for each place it put its own pixels in: they were drawn after the interface and cover those layers.
    /// `region` must not have had them drawn over it since the host filled it, or a dimming layer would be drawn twice.
    ///
    /// # Errors
    /// The renderer's error.
    pub fn paint_overlays<R: fusuma_render::Renderer>(
        &self,
        r: &mut R,
        theme: &crate::theme::Theme,
        region: Rect,
    ) -> Result<(), R::Error> {
        if !self.host_surfaces || region.is_empty() {
            return Ok(());
        }
        r.save()?;
        let mut done = r.clip(region);
        if done.is_ok() {
            done = self.paint_dialogs(r, theme, Some(region));
        }
        if done.is_ok() {
            done = self.paint_toasts(r, theme, Some(region));
        }
        if done.is_ok() {
            done = self.paint_drag(r, theme, Some(region));
        }
        let back = r.restore();
        done?;
        back
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
                let visible = self.visible_rect(i).intersect(&screen);
                if !visible.is_empty() {
                    plan.push_surface(id, n.rect, visible, n.first.is_some());
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
