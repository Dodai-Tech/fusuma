//! Dialogs: a modal panel in the middle of the screen, drawn over everything else. While one is open the widgets
//! behind it cannot be pressed, scrolled or focused; the focus stays inside it and goes back where it was when it
//! closes. The toolkit never closes one by itself: a press outside it, or Cancel (Escape), sends
//! [`ActionKind::Dismissed`](crate::ActionKind::Dismissed) and the app decides.

use fusuma_render::{Color, Rect, Renderer};

use crate::theme::Theme;
use crate::tree::{Action, ActionKind, BuildError, Flags, Kind, NodeId, Style, Tree};

/// The dimming layer behind an open dialog. Only drawn on a renderer that blends alpha: on one that does not, a
/// translucent black would be painted opaque and hide the whole screen.
const SCRIM: Color = Color::rgba(0, 0, 0, 150);

impl<const N: usize> Tree<N> {
    /// Adds a dialog under `parent` (usually the root), closed. Its children are the content; its `style` sets the
    /// padding, the gap and, if wanted, a fixed width. It is laid out out of the flow of its parent, centred on the
    /// screen at the size of its content (cut to the screen less a margin), and drawn last. Show it with
    /// [`open_dialog`](Self::open_dialog).
    ///
    /// # Errors
    /// [`BuildError`] when the node cannot be added there.
    pub fn dialog(&mut self, parent: Option<NodeId>, style: Style) -> Result<NodeId, BuildError> {
        self.add(
            parent,
            Kind::Dialog {
                open: false,
                prev: None,
            },
            style,
        )
    }

    /// Whether dialog `id` is open.
    #[must_use]
    pub fn is_dialog_open(&self, id: NodeId) -> bool {
        self.slot(id)
            .is_some_and(|i| matches!(self.nodes[i].kind, Kind::Dialog { open: true, .. }))
    }

    /// Opens dialog `id`: the screen is dimmed behind it, the focus moves to its first widget, and the widgets
    /// behind it stop answering.
    pub fn open_dialog(&mut self, id: NodeId) {
        let Some(i) = self.slot(id) else { return };
        let prev = self.focused();
        let Kind::Dialog { open, prev: p } = &mut self.nodes[i].kind else {
            return;
        };
        if *open {
            return;
        }
        *open = true;
        *p = prev;
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        self.damage_all();
        if let Some(first) = self.first_focusable_in(i) {
            let visible = self.focus_visible;
            self.set_focus(Some(first), visible);
        }
    }

    /// Closes dialog `id`, and gives the focus back to what had it before the dialog opened, if that can take it.
    pub fn close_dialog(&mut self, id: NodeId) {
        let Some(i) = self.slot(id) else { return };
        let Kind::Dialog { open, prev } = &mut self.nodes[i].kind else {
            return;
        };
        if !*open {
            return;
        }
        let back = prev.take();
        *open = false;
        self.request(i, Flags::LAYOUT | Flags::PAINT);
        self.damage_all();
        // Focus is only taken back if it was inside the dialog: a program that moved it elsewhere keeps it there.
        let inside = self
            .focus
            .and_then(|f| self.slot(f))
            .is_some_and(|f| self.is_in_or_self(f, i));
        if inside {
            let target = back
                .and_then(|b| self.slot(b))
                .filter(|&b| self.focusable(b));
            let visible = self.focus_visible;
            self.set_focus(target, visible);
        }
    }

    /// Everything on the screen has to be drawn again: a dialog dims all of it.
    fn damage_all(&mut self) {
        if let Some(r) = self.root.map(usize::from) {
            let rect = self.nodes[r].rect;
            self.add_damage(rect);
        }
    }

    /// The first widget in dialog `d` that can take the focus.
    fn first_focusable_in(&self, d: usize) -> Option<usize> {
        let mut cur = self.nodes[d].first.map(usize::from);
        while let Some(i) = cur {
            if self.focusable(i) {
                return Some(i);
            }
            cur = self.preorder_next(i, d);
        }
        None
    }

    /// The next node in tree order that is not inside node `i`.
    pub(crate) fn after_subtree(&self, i: usize, top: usize) -> Option<usize> {
        let mut cur = i;
        while cur != top {
            if let Some(n) = self.nodes[cur].next {
                return Some(usize::from(n));
            }
            cur = usize::from(self.nodes[cur].parent?);
        }
        None
    }

    /// The dialog that holds the modal state: the last open one in tree order that is not itself inside a closed
    /// dialog. `None` when no dialog is open.
    pub(crate) fn modal(&self) -> Option<usize> {
        let root = usize::from(self.root?);
        let mut found = None;
        let mut cur = Some(root);
        while let Some(i) = cur {
            match self.nodes[i].kind {
                Kind::Dialog { open: true, .. } => {
                    found = Some(i);
                    cur = self.preorder_next(i, root);
                }
                // What is inside a closed dialog is not there at all.
                Kind::Dialog { open: false, .. } => cur = self.after_subtree(i, root),
                _ => cur = self.preorder_next(i, root),
            }
        }
        found
    }

    /// Whether node `i` is inside (or is) node `ancestor`.
    pub(crate) fn is_in_or_self(&self, i: usize, ancestor: usize) -> bool {
        let mut cur = Some(i);
        while let Some(c) = cur {
            if c == ancestor {
                return true;
            }
            cur = self.nodes[c].parent.map(usize::from);
        }
        false
    }

    /// Whether node `i` cannot be reached right now: it is in a closed dialog, or a dialog is open and it is not in it.
    pub(crate) fn blocked(&self, i: usize) -> bool {
        let modal = self.modal();
        let mut inside_modal = false;
        let mut cur = Some(i);
        while let Some(c) = cur {
            match self.nodes[c].kind {
                Kind::Dialog { open: false, .. } => return true,
                Kind::Dialog { open: true, .. } if Some(c) == modal => inside_modal = true,
                _ => {}
            }
            cur = self.nodes[c].parent.map(usize::from);
        }
        modal.is_some() && !inside_modal
    }

    /// Tells the app that the user asked to close the open dialog.
    pub(crate) fn dismiss_modal(&mut self) {
        if let Some(m) = self.modal() {
            let source = self.id_of(m);
            self.push_action(Action {
                source,
                kind: ActionKind::Dismissed,
            });
        }
    }

    /// Draws the open dialogs over what is already drawn: a dimming layer over the whole screen where the renderer
    /// can blend, then the dialog and what is in it. `region` limits the drawing to what overlaps it.
    pub(crate) fn paint_dialogs<R: Renderer>(
        &self,
        r: &mut R,
        theme: &Theme,
        region: Option<Rect>,
    ) -> Result<(), R::Error> {
        let Some(root) = self.root.map(usize::from) else {
            return Ok(());
        };
        let alpha = r.capabilities().alpha;
        let mut cur = Some(root);
        while let Some(i) = cur {
            match self.nodes[i].kind {
                Kind::Dialog { open: true, .. } => {
                    if alpha {
                        r.fill_rect(self.nodes[root].rect, 0, SCRIM)?;
                    }
                    let seen = region.is_none_or(|g| {
                        let o = self.nodes[i].rect.intersect(&g);
                        o.w > 0 && o.h > 0
                    });
                    if seen {
                        self.paint_walk(i, r, theme, region)?;
                    }
                    cur = self.preorder_next(i, root);
                }
                Kind::Dialog { open: false, .. } => cur = self.after_subtree(i, root),
                _ => cur = self.preorder_next(i, root),
            }
        }
        Ok(())
    }

    /// The panel of an open dialog: a raised surface with an outline.
    pub(crate) fn paint_dialog_panel<R: Renderer>(
        &self,
        i: usize,
        r: &mut R,
        theme: &Theme,
    ) -> Result<(), R::Error> {
        let rect = self.nodes[i].rect;
        let radius = theme.radius(crate::theme::Round::Large);
        r.fill_rect(rect, radius, theme.colors.surface_raised)?;
        r.stroke_rect(rect, radius, theme.border_width, theme.colors.border_strong)
    }
}
