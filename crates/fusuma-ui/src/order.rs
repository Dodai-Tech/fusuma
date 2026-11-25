//! Keys and order: what a keyed list needs from the tree. A row is called by a key the app chooses; when the list
//! changes, rows that are still in it keep their node (and so their text, focus and scroll), new ones are made, gone
//! ones removed, and rows that moved are moved, not rebuilt.

use crate::tree::{Flags, NodeId, Tree};

impl<const N: usize> Tree<N> {
    /// Gives node `id` the key `key`.
    pub fn set_key(&mut self, id: NodeId, key: u32) {
        if let Some(i) = self.slot(id) {
            self.nodes[i].key = key;
        }
    }

    /// The key of node `id` (0 when it has none).
    #[must_use]
    pub fn key_of(&self, id: NodeId) -> u32 {
        self.slot(id).map_or(0, |i| self.nodes[i].key)
    }

    /// Remembers what node `id` (the root of a memoised view) was last made from: 16 bits, a version of the data it shows.
    pub fn set_memo(&mut self, id: NodeId, value: u16) {
        if let Some(i) = self.slot(id) {
            self.nodes[i].memo = value;
        }
    }

    /// What node `id` was last made from (0 when never set).
    #[must_use]
    pub fn memo_of(&self, id: NodeId) -> u16 {
        self.slot(id).map_or(0, |i| self.nodes[i].memo)
    }

    /// Whether `node` is `ancestor` or somewhere under it.
    #[must_use]
    pub fn is_within(&self, ancestor: NodeId, node: NodeId) -> bool {
        match (self.slot(ancestor), self.slot(node)) {
            (Some(a), Some(n)) => a == n || self.is_inside(n, a),
            _ => false,
        }
    }

    /// Moves `id` to just before its sibling `before` (to the end when `before` is `None`), in the same parent. Does
    /// nothing if they do not share a parent, or `id` is already there.
    pub fn move_before(&mut self, id: NodeId, before: Option<NodeId>) {
        let Some(i) = self.slot(id) else { return };
        let Some(parent) = self.nodes[i].parent.map(usize::from) else {
            return;
        };
        let before = match before {
            Some(b) => {
                let Some(bi) = self.slot(b) else { return };
                if self.nodes[bi].parent.map(usize::from) != Some(parent) || bi == i {
                    return;
                }
                Some(bi)
            }
            None => None,
        };
        // Already in place?
        let after = self.nodes[i].next.map(usize::from);
        if after == before {
            return;
        }
        // Unlink.
        let (prev, next) = (self.nodes[i].prev, self.nodes[i].next);
        match prev {
            Some(p) => self.nodes[usize::from(p)].next = next,
            None => self.nodes[parent].first = next,
        }
        match next {
            Some(n) => self.nodes[usize::from(n)].prev = prev,
            None => self.nodes[parent].last = prev,
        }
        // Link before `before`, or at the end.
        let (new_prev, new_next) = match before {
            Some(b) => (self.nodes[b].prev, Some(b as u16)),
            None => (self.nodes[parent].last, None),
        };
        self.nodes[i].prev = new_prev;
        self.nodes[i].next = new_next;
        match new_prev {
            Some(p) => self.nodes[usize::from(p)].next = Some(i as u16),
            None => self.nodes[parent].first = Some(i as u16),
        }
        match new_next {
            Some(n) => self.nodes[usize::from(n)].prev = Some(i as u16),
            None => self.nodes[parent].last = Some(i as u16),
        }
        let rect = self.nodes[parent].rect;
        self.add_damage(rect);
        self.request(parent, Flags::LAYOUT | Flags::PAINT);
    }
}

#[cfg(test)]
mod tests {
    use fusuma_render::Rect;
    use fusuma_render_vello_cpu::VelloCpu;

    use super::*;
    use crate::theme::{ButtonVariant, Theme};
    use crate::tree::Style;

    fn order(t: &Tree<16>, parent: NodeId) -> Vec<String> {
        t.children(parent)
            .map(|c| t.text(c).unwrap().to_owned())
            .collect()
    }

    fn page() -> (Tree<16>, NodeId, [NodeId; 4]) {
        let mut t = Tree::<16>::new();
        let root = t.column(None, Style::new().gap(2)).unwrap();
        let ids =
            ["a", "b", "c", "d"].map(|s| t.button(Some(root), s, ButtonVariant::Primary).unwrap());
        let r: VelloCpu = fusuma_render_vello_cpu::new(8, 8).unwrap();
        t.update(&r, &Theme::DARK, Rect::new(0, 0, 100, 200));
        (t, root, ids)
    }

    #[test]
    fn keys_belong_to_nodes_and_stale_handles_have_none() {
        let (mut t, _, [a, b, ..]) = page();
        t.set_key(a, 7);
        assert_eq!((t.key_of(a), t.key_of(b)), (7, 0));
        t.remove(a);
        assert_eq!(t.key_of(a), 0);
        t.set_key(a, 9);
        assert_eq!(t.key_of(a), 0);
    }

    #[test]
    fn a_node_moves_before_a_sibling_or_to_the_end_and_the_places_follow() {
        let (mut t, root, [a, b, c, d]) = page();
        t.move_before(d, Some(a));
        assert_eq!(order(&t, root), ["d", "a", "b", "c"]);
        t.move_before(d, None);
        assert_eq!(order(&t, root), ["a", "b", "c", "d"]);
        t.move_before(a, Some(c));
        assert_eq!(order(&t, root), ["b", "a", "c", "d"]);
        t.move_before(c, Some(b));
        assert_eq!(order(&t, root), ["c", "b", "a", "d"]);
        // Backwards too: the links agree both ways.
        let rev: Vec<_> = {
            let mut v = Vec::new();
            let mut cur = t.children(root).last();
            while let Some(n) = cur {
                v.push(t.text(n).unwrap().to_owned());
                cur = t.nodes_prev(n);
            }
            v
        };
        assert_eq!(rev, ["d", "a", "b", "c"]);
        let r: VelloCpu = fusuma_render_vello_cpu::new(8, 8).unwrap();
        t.update(&r, &Theme::DARK, Rect::new(0, 0, 100, 200));
        let ys: Vec<i32> = t.children(root).map(|n| t.rect(n).unwrap().y).collect();
        assert!(
            ys.windows(2).all(|w| w[0] < w[1]),
            "laid out in the new order: {ys:?}"
        );
        let _ = (a, b, c, d);
    }

    #[test]
    fn odd_moves_do_nothing() {
        let (mut t, root, [a, b, ..]) = page();
        t.move_before(a, Some(a));
        t.move_before(a, Some(b));
        assert_eq!(order(&t, root), ["a", "b", "c", "d"], "already there");
        // The root has no siblings to move among.
        t.move_before(root, Some(a));
        t.remove(b);
        t.move_before(a, Some(b));
        assert_eq!(order(&t, root), ["a", "c", "d"], "a stale handle");
    }
}
