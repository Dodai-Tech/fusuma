#![no_main]
//! Rectangle maths never panics and keeps its promises on any input.

use libfuzzer_sys::fuzz_target;
use tokonoma::{Point, Rect};

fn rect(c: &[u8]) -> Rect {
    let n = |i: usize| i32::from_le_bytes([c[i], c[i + 1], c[i + 2], c[i + 3]]);
    Rect::new(n(0), n(4), u32::from_le_bytes([c[8], c[9], c[10], c[11]]), u32::from_le_bytes([c[12], c[13], c[14], c[15]]))
}

fuzz_target!(|data: &[u8]| {
    for c in data.chunks_exact(32) {
        let (a, b) = (rect(&c[..16]), rect(&c[16..]));
        let i = a.intersect(&b);
        // The overlap lies in both rectangles.
        if !i.is_empty() {
            let corner = Point::new(i.x, i.y);
            assert!(a.contains(corner) && b.contains(corner));
        }
        assert_eq!(a.intersect(&b).is_empty(), b.intersect(&a).is_empty());
        let _ = a.translate(c[0] as i32 * 1_000_000, -(c[1] as i32) * 1_000_000);
        let _ = a.inset(u32::from(c[2]) << 24);
        let _ = (a.right(), a.bottom(), a.contains(Point::new(b.x, b.y)));
    }
});
