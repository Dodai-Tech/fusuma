//! A few separate rectangles of damage, so a hover and a status line far apart do not repaint everything between
//! them. Fixed storage: when a fifth would not fit, the two that waste least when joined are joined.

use fusuma_render::Rect;

/// Most separate rectangles kept.
pub const MAX_PIECES: usize = 4;

/// Up to [`MAX_PIECES`] rectangles that together cover what changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pieces {
    list: [Rect; MAX_PIECES],
    len: usize,
}

fn area(r: &Rect) -> u128 {
    u128::from(r.w) * u128::from(r.h)
}

/// Whether two rectangles overlap or touch.
fn touches(a: &Rect, b: &Rect) -> bool {
    i64::from(a.x) <= i64::from(b.right())
        && i64::from(b.x) <= i64::from(a.right())
        && i64::from(a.y) <= i64::from(b.bottom())
        && i64::from(b.y) <= i64::from(a.bottom())
}

/// Area of `a` and `b` joined less the area of both: what a join would draw for nothing.
fn waste(a: &Rect, b: &Rect) -> u128 {
    area(&a.union(b)).saturating_sub(area(a) + area(b))
}

impl Pieces {
    /// No rectangles.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            list: [Rect::new(0, 0, 0, 0); MAX_PIECES],
            len: 0,
        }
    }

    /// The rectangles.
    #[must_use]
    pub fn as_slice(&self) -> &[Rect] {
        &self.list[..self.len]
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Forgets them all.
    pub const fn clear(&mut self) {
        self.len = 0;
    }

    /// The smallest rectangle that holds them all.
    #[must_use]
    pub fn bounds(&self) -> Option<Rect> {
        let mut it = self.as_slice().iter();
        let first = *it.next()?;
        Some(it.fold(first, |a, b| a.union(b)))
    }

    /// Adds `rect`. It joins any piece it overlaps, touches or sits so near that joining wastes at most a quarter of
    /// their area; when the storage is full the cheapest join is made.
    pub fn add(&mut self, rect: Rect) {
        if rect.is_empty() {
            return;
        }
        let mut cur = rect;
        // Joining can make the result touch another piece, so go on until nothing joins.
        'again: loop {
            for i in 0..self.len {
                let p = self.list[i];
                if touches(&p, &cur) || waste(&p, &cur) * 4 <= area(&p) + area(&cur) {
                    cur = cur.union(&p);
                    self.list[i] = self.list[self.len - 1];
                    self.len -= 1;
                    continue 'again;
                }
            }
            break;
        }
        if self.len < MAX_PIECES {
            self.list[self.len] = cur;
            self.len += 1;
            return;
        }
        // Full: join the new one with the piece it wastes least with, or two pieces if that is cheaper.
        let (mut best, mut at) = (u128::MAX, None);
        for i in 0..self.len {
            let w = waste(&self.list[i], &cur);
            if w < best {
                (best, at) = (w, Some((i, None)));
            }
            for j in i + 1..self.len {
                let w = waste(&self.list[i], &self.list[j]);
                if w < best {
                    (best, at) = (w, Some((i, Some(j))));
                }
            }
        }
        match at {
            Some((i, None)) => self.list[i] = self.list[i].union(&cur),
            Some((i, Some(j))) => {
                self.list[i] = self.list[i].union(&self.list[j]);
                self.list[j] = cur;
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: u32, h: u32) -> Rect {
        Rect::new(x, y, w, h)
    }

    /// Whether every pixel of `inner` is in some piece.
    fn covered(p: &Pieces, inner: Rect) -> bool {
        (inner.y..inner.bottom()).all(|y| {
            (inner.x..inner.right()).all(|x| {
                p.as_slice()
                    .iter()
                    .any(|q| q.contains(fusuma_render::Point::new(x, y)))
            })
        })
    }

    #[test]
    fn far_apart_rectangles_stay_apart_and_near_ones_join() {
        let mut p = Pieces::new();
        assert!(p.is_empty() && p.bounds().is_none());
        p.add(r(0, 0, 40, 20));
        p.add(r(0, 400, 40, 20));
        assert_eq!(p.as_slice().len(), 2, "a hover and a status line stay two");
        p.add(r(30, 10, 40, 20));
        assert_eq!(p.as_slice().len(), 2, "an overlapping one joins the first");
        assert!(p.as_slice().contains(&r(0, 0, 70, 30)));
        p.add(r(0, 30, 70, 10));
        assert!(p.as_slice().contains(&r(0, 0, 70, 40)), "touching joins");
        assert_eq!(p.bounds(), Some(r(0, 0, 70, 420)));
        p.clear();
        assert!(p.is_empty());
        p.add(r(5, 5, 0, 10));
        assert!(p.is_empty(), "an empty rectangle is nothing");
    }

    #[test]
    fn a_chain_of_joins_ends_in_one_piece() {
        let mut p = Pieces::new();
        p.add(r(0, 0, 10, 10));
        p.add(r(30, 0, 10, 10));
        p.add(r(60, 0, 10, 10));
        assert_eq!(p.as_slice().len(), 3);
        // A bar across all three joins them.
        p.add(r(0, 5, 70, 10));
        assert_eq!(p.as_slice(), &[r(0, 0, 70, 15)]);
    }

    #[test]
    fn a_full_list_joins_the_cheapest_pair_and_never_loses_a_pixel() {
        let mut p = Pieces::new();
        let all = [
            r(0, 0, 10, 10),
            r(100, 0, 10, 10),
            r(0, 100, 10, 10),
            r(100, 100, 10, 10),
            r(12, 112, 10, 10),
            r(50, 50, 5, 5),
            r(200, 0, 10, 10),
        ];
        for q in all {
            p.add(q);
            assert!(p.as_slice().len() <= MAX_PIECES);
        }
        for q in all {
            assert!(covered(&p, q), "{q:?} is lost in {p:?}");
        }
    }

    #[test]
    fn a_random_run_of_additions_always_covers_all_of_them_within_four_pieces() {
        let mut seed = 7u64;
        let mut next = |n: u32| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((seed >> 33) as u32) % n
        };
        for _ in 0..40 {
            let mut p = Pieces::new();
            let mut added = Vec::new();
            for _ in 0..12 {
                let q = r(
                    next(200) as i32,
                    next(200) as i32,
                    1 + next(30),
                    1 + next(30),
                );
                added.push(q);
                p.add(q);
                assert!(p.as_slice().len() <= MAX_PIECES);
            }
            for q in added {
                assert!(covered(&p, q), "{q:?} lost in {p:?}");
            }
        }
    }
}
