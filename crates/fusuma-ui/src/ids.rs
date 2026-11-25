//! Names the app gives to nodes: stable numbers that survive a page being built again, so a message can say "focus the
//! search field" or "scroll the list to the top" without holding a [`NodeId`], and a test can find a widget by what the
//! app called it. A node's own id (the handle [`Tree`] gives it) changes whenever it is rebuilt; an [`Id`] does not.

use crate::tree::{NodeId, Tree};

/// How many named nodes one tree keeps track of.
pub const MAX_IDS: usize = 24;

/// A name for a node, chosen by the app: usually the variant of an enum cast to a number
/// (`Id::new(Field::Search as u16)`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub u16);

impl Id {
    /// The id `n`.
    #[must_use]
    pub const fn new(n: u16) -> Self {
        Self(n)
    }
}

/// The names that are in use and the nodes they point at.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Names {
    list: [Option<(Id, NodeId)>; MAX_IDS],
}

impl Names {
    pub(crate) const fn new() -> Self {
        Self {
            list: [None; MAX_IDS],
        }
    }
}

impl<const N: usize> Tree<N> {
    /// Gives node `node` the name `id`. A name belongs to one node: giving it to another takes it from the first, and a
    /// node has one name: naming it again changes it. Returns false when the node is gone or all [`MAX_IDS`] names are
    /// in use by nodes that are still there.
    pub fn set_id(&mut self, node: NodeId, id: Id) -> bool {
        if self.slot(node).is_none() {
            return false;
        }
        // The same name, or the same node, or a name whose node has gone, in that order of preference.
        let at = self
            .names
            .list
            .iter()
            .position(|e| e.is_some_and(|(i, _)| i == id))
            .or_else(|| {
                self.names
                    .list
                    .iter()
                    .position(|e| e.is_some_and(|(_, n)| n == node))
            })
            .or_else(|| {
                self.names
                    .list
                    .iter()
                    .position(|e| e.is_none_or(|(_, n)| self.slot(n).is_none()))
            });
        let Some(at) = at else { return false };
        // The node's old name, if it had another slot, is dropped.
        for (k, e) in self.names.list.iter_mut().enumerate() {
            if k != at && e.is_some_and(|(_, n)| n == node) {
                *e = None;
            }
        }
        self.names.list[at] = Some((id, node));
        true
    }

    /// The node called `id`, if there is one still in the tree.
    #[must_use]
    pub fn find(&self, id: Id) -> Option<NodeId> {
        self.names
            .list
            .iter()
            .flatten()
            .find(|(i, _)| *i == id)
            .map(|&(_, n)| n)
            .filter(|&n| self.slot(n).is_some())
    }

    /// The name of node `node`, if it has one.
    #[must_use]
    pub fn id_of_node(&self, node: NodeId) -> Option<Id> {
        self.slot(node)?;
        self.names
            .list
            .iter()
            .flatten()
            .find(|(_, n)| *n == node)
            .map(|&(i, _)| i)
    }

    /// Gives the focus to the node called `id`; false when there is none or it cannot take it.
    pub fn focus_id(&mut self, id: Id) -> bool {
        self.find(id).is_some_and(|n| self.focus(n))
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::Rect;
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::Style;

    const THEME: Theme = crate::theme::BITMAP_TEST;

    fn page() -> (Tree<64>, NodeId, [NodeId; 3]) {
        let mut t = Tree::<64>::new();
        let root = t.column(None, Style::new().gap(4)).unwrap();
        let ids = [
            t.text_input(Some(root), "a").unwrap(),
            t.button(Some(root), "Go", ButtonVariant::Primary).unwrap(),
            t.checkbox(Some(root), "Opt", false).unwrap(),
        ];
        let s: VelloCpu = fusuma_render_vello_cpu::new(8, 8).unwrap();
        t.update(&s, &THEME, Rect::new(0, 0, 200, 200));
        (t, root, ids)
    }

    #[test]
    fn a_name_finds_its_node_and_a_node_finds_its_name() {
        let (mut t, _, [field, go, _]) = page();
        assert!(t.set_id(field, Id::new(1)) && t.set_id(go, Id::new(2)));
        assert_eq!(t.find(Id::new(1)), Some(field));
        assert_eq!(t.find(Id::new(2)), Some(go));
        assert_eq!(t.find(Id::new(3)), None);
        assert_eq!(t.id_of_node(go), Some(Id::new(2)));
        assert!(t.focus_id(Id::new(1)));
        assert_eq!(t.focused(), Some(field));
        assert!(!t.focus_id(Id::new(3)), "no such name");
    }

    #[test]
    fn a_name_belongs_to_one_node_and_a_node_has_one_name() {
        let (mut t, _, [field, go, check]) = page();
        t.set_id(field, Id::new(7));
        // Giving the name to another node moves it.
        t.set_id(go, Id::new(7));
        assert_eq!(t.find(Id::new(7)), Some(go));
        assert_eq!(t.id_of_node(field), None);
        // Naming a node again changes its name; the old one is free.
        t.set_id(go, Id::new(8));
        assert_eq!((t.find(Id::new(8)), t.find(Id::new(7))), (Some(go), None));
        t.set_id(check, Id::new(7));
        assert_eq!(t.find(Id::new(7)), Some(check));
    }

    #[test]
    fn a_rebuilt_node_takes_the_name_again_and_a_removed_one_loses_it() {
        let (mut t, root, [field, go, _]) = page();
        t.set_id(field, Id::new(1));
        t.set_id(go, Id::new(2));
        t.remove(go);
        assert_eq!(t.find(Id::new(2)), None, "gone with the node");
        assert_eq!(t.id_of_node(go), None);
        let again = t.button(Some(root), "Go", ButtonVariant::Primary).unwrap();
        assert_ne!(again, go, "a rebuilt node is a new handle");
        assert!(t.set_id(again, Id::new(2)));
        assert_eq!(
            t.find(Id::new(2)),
            Some(again),
            "the name is stable, the handle is not"
        );
        assert!(
            !t.set_id(go, Id::new(9)),
            "a handle that is gone cannot be named"
        );
    }

    #[test]
    fn the_table_is_full_only_of_nodes_that_are_still_there() {
        let mut t = Tree::<64>::new();
        let root = t.column(None, Style::new()).unwrap();
        let nodes: Vec<NodeId> = (0..MAX_IDS)
            .map(|_| t.button(Some(root), "x", ButtonVariant::Primary).unwrap())
            .collect();
        for (k, n) in nodes.iter().enumerate() {
            assert!(t.set_id(*n, Id::new(k as u16)));
        }
        let extra = t.button(Some(root), "y", ButtonVariant::Primary).unwrap();
        assert!(!t.set_id(extra, Id::new(500)), "full of live nodes");
        // One goes: its slot is free.
        t.remove(nodes[3]);
        assert!(t.set_id(extra, Id::new(500)));
        assert_eq!(t.find(Id::new(500)), Some(extra));
        assert_eq!(
            t.find(Id::new(4)),
            Some(nodes[4]),
            "the others are untouched"
        );
    }
}
