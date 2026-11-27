//! Showing the picture in a window by copying only what the window's buffer is missing.
//!
//! A window system hands out buffers that are reused, each with an *age*: how many presents ago it last held the
//! picture (0: its content is unknown). A buffer of age `n` has every change except the last `n`, so it needs the union
//! of the damage of those `n` presents and nothing else. [`History`] remembers the damage of the last few presents and
//! answers that; [`copy`] puts a region of the picture into the buffer. The OS is then told the damage of this
//! present alone (`present_with_damage`), which a compositor can use to update less of the screen.

use fusuma::render::Rect;

/// How many presents are remembered. A buffer older than this is copied whole.
pub const DEPTH: usize = 8;

/// What a present changed in the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Only this rectangle.
    Part(Rect),
    /// Everything (a new size, an expose, a first picture).
    All,
}

impl Change {
    /// The change that both of `self` and `other` make.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::Part(a), Self::Part(b)) => Self::Part(a.union(&b)),
            _ => Self::All,
        }
    }
}

/// The damage of the last presents, newest first.
#[derive(Clone, Debug, Default)]
pub struct History {
    entries: [Option<Change>; DEPTH],
}

impl History {
    /// A history that knows nothing: every buffer is copied whole until presents have been made.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: [None; DEPTH],
        }
    }

    /// Records the present that is being made.
    pub fn push(&mut self, change: Change) {
        self.entries.copy_within(0..DEPTH - 1, 1);
        self.entries[0] = Some(change);
    }

    /// What a buffer of `age` is missing, counting the present just pushed: its content is that of `age` presents ago, so
    /// it lacks the last `age` changes. Age 0 (unknown), or older than the history, is everything.
    #[must_use]
    pub fn missing(&self, age: usize) -> Change {
        if age == 0 || age > DEPTH {
            return Change::All;
        }
        let mut total: Option<Change> = None;
        for entry in &self.entries[..age] {
            let Some(change) = *entry else {
                return Change::All;
            };
            total = Some(total.map_or(change, |t| t.union(change)));
        }
        total.unwrap_or(Change::All)
    }

    /// The change of the present just pushed.
    #[must_use]
    pub fn latest(&self) -> Change {
        self.entries[0].unwrap_or(Change::All)
    }
}

/// Copies `change` of `src` (`width` pixels per row) into `dst`, cut to both. Returns the pixels that were copied,
/// which is what a test checks and what a profile counts.
pub fn copy(src: &[u32], dst: &mut [u32], width: u32, change: Change) -> usize {
    let w = width as usize;
    if w == 0 {
        return 0;
    }
    let height = (src.len() / w).min(dst.len() / w);
    let (x0, y0, x1, y1) = match change {
        Change::All => (0, 0, w, height),
        Change::Part(r) => (
            r.x.max(0) as usize,
            r.y.max(0) as usize,
            (r.right().max(0) as usize).min(w),
            (r.bottom().max(0) as usize).min(height),
        ),
    };
    if x0 >= x1 {
        return 0;
    }
    let mut n = 0;
    for y in y0..y1 {
        let row = y * w;
        dst[row + x0..row + x1].copy_from_slice(&src[row + x0..row + x1]);
        n += x1 - x0;
    }
    n
}

/// The rectangle of `change` for the window system, cut to a picture of `width` x `height`; `None` when it is empty.
#[must_use]
pub fn damage_rect(change: Change, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let r = match change {
        Change::All => Rect::new(0, 0, width, height),
        Change::Part(r) => r.intersect(&Rect::new(0, 0, width, height)),
    };
    (!r.is_empty()).then_some((r.x as u32, r.y as u32, r.w, r.h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(x: i32, y: i32, w: u32, h: u32) -> Change {
        Change::Part(Rect::new(x, y, w, h))
    }

    #[test]
    fn a_buffer_of_unknown_age_is_copied_whole() {
        let mut h = History::new();
        h.push(part(0, 0, 2, 2));
        assert_eq!(h.missing(0), Change::All);
    }

    #[test]
    fn a_buffer_one_present_old_needs_only_this_presents_damage() {
        let mut h = History::new();
        h.push(Change::All);
        h.push(part(4, 4, 2, 2));
        assert_eq!(h.missing(1), part(4, 4, 2, 2));
    }

    #[test]
    fn an_older_buffer_needs_the_damage_of_every_present_it_missed() {
        let mut h = History::new();
        h.push(Change::All);
        h.push(part(0, 0, 2, 2));
        h.push(part(10, 10, 2, 2));
        assert_eq!(h.missing(2), part(0, 0, 12, 12));
    }

    #[test]
    fn a_buffer_older_than_what_is_remembered_is_copied_whole() {
        let mut h = History::new();
        h.push(part(0, 0, 1, 1));
        // Only one present is known: a buffer two presents old may lack one that is not remembered.
        assert_eq!(h.missing(2), Change::All);
        for _ in 0..DEPTH {
            h.push(part(0, 0, 1, 1));
        }
        assert_eq!(h.missing(DEPTH + 1), Change::All);
        assert_eq!(h.missing(DEPTH), part(0, 0, 1, 1));
    }

    #[test]
    fn a_whole_change_makes_every_union_whole() {
        let mut h = History::new();
        h.push(part(0, 0, 1, 1));
        h.push(Change::All);
        assert_eq!(h.missing(2), Change::All);
        assert_eq!(h.latest(), Change::All);
    }

    #[test]
    fn copy_moves_only_the_region_and_cuts_it_to_the_picture() {
        let src: Vec<u32> = (0..16).collect();
        let mut dst = vec![99; 16];
        assert_eq!(copy(&src, &mut dst, 4, part(1, 1, 2, 2)), 4);
        assert_eq!(&dst[5..7], &[5, 6]);
        assert_eq!(&dst[9..11], &[9, 10]);
        assert_eq!(dst[0], 99);
        assert_eq!(dst[7], 99);
        let mut dst = vec![99; 16];
        assert_eq!(copy(&src, &mut dst, 4, part(-5, -5, 100, 100)), 16);
        assert_eq!(dst, src);
        assert_eq!(copy(&src, &mut dst, 0, Change::All), 0);
        assert_eq!(copy(&src, &mut dst, 4, part(9, 9, 2, 2)), 0);
    }

    #[test]
    fn the_buffers_of_a_swap_chain_end_up_equal_to_the_picture() {
        // Three buffers handed out in turn, each with the age a window system reports: 0 the first time, else the
        // number of presents since it was last shown. A one-pixel change moves each present.
        let (w, h) = (8u32, 8u32);
        let mut picture = vec![0u32; (w * h) as usize];
        let mut bufs = vec![vec![0u32; (w * h) as usize]; 3];
        let mut last: [Option<usize>; 3] = [None; 3];
        let mut hist = History::new();
        for step in 0..30usize {
            let (x, y) = (step % 8, (step * 3) % 8);
            picture[y * 8 + x] = step as u32 + 1;
            hist.push(part(x as i32, y as i32, 1, 1));
            let b = step % 3;
            let age = last[b].map_or(0, |l| step - l);
            copy(&picture, &mut bufs[b], w, hist.missing(age));
            last[b] = Some(step);
            assert_eq!(bufs[b], picture, "step {step}");
        }
    }

    #[test]
    fn the_damage_for_the_window_system_is_cut_and_never_empty() {
        assert_eq!(damage_rect(Change::All, 10, 5), Some((0, 0, 10, 5)));
        assert_eq!(damage_rect(part(8, 3, 10, 10), 10, 5), Some((8, 3, 2, 2)));
        assert_eq!(damage_rect(part(20, 20, 3, 3), 10, 5), None);
    }
}
