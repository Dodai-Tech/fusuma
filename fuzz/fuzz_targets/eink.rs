#![no_main]
//! The e-paper pipeline never panics and keeps its promises: the planner only gives regions on the panel in whole bytes, packing is
//! the size it says, and the one-bit drawing never touches the padding bits of a row.

use fusuma_platform_eink::{byte_region, gray_level, pack_gray4, pack_mono, Dither, Planner, Refresh, RefreshPolicy};
use fusuma_render::{Point, Rect};
use fusuma_render_integer::{Embedded, Frame, MonoFrame};
use libfuzzer_sys::fuzz_target;

fn n(c: &[u8], i: usize) -> i32 {
    i32::from(i16::from_le_bytes([c[i], c[i + 1]])) * 40
}

fuzz_target!(|data: &[u8]| {
    let (w, h) = (122u32, 250u32);
    let mut planner = Planner::new((w, h), RefreshPolicy {
        min_interval_ms: u32::from(data.first().copied().unwrap_or(0)) * 10,
        settle_ms: u32::from(data.get(1).copied().unwrap_or(0)),
        full_every: u32::from(data.get(2).copied().unwrap_or(0) % 8),
        dither: if data.get(3).is_some_and(|b| b & 1 == 1) { Dither::Ordered } else { Dither::Threshold(data.get(3).copied().unwrap_or(128)) },
    });
    let mut frame = Embedded::new(MonoFrame::new(122, 250, vec![0xFFu8; 16 * 250], Dither::Threshold(170)).unwrap());
    let mut now = 0u64;
    for c in data.get(4..).unwrap_or(&[]).chunks_exact(12) {
        now += u64::from(c[8]) * 50;
        let r = Rect::new(n(c, 0), n(c, 2), u32::from(c[4]) * 9, u32::from(c[5]) * 9);
        match c[9] % 4 {
            0 => planner.damage(r, now),
            1 => {
                if let Some(Refresh::Partial(p)) = planner.take(now) {
                    assert!(p.x >= 0 && p.y >= 0 && p.x % 8 == 0 && p.w % 8 == 0, "{p:?}");
                    assert!(p.right() <= 128 && p.bottom() <= 250, "{p:?}");
                }
            }
            2 => {
                if let Some(b) = byte_region(r, w, h) {
                    assert!(b.x % 8 == 0 && b.w % 8 == 0 && b.w > 0 && b.h > 0, "{b:?}");
                    let mut out = Vec::new();
                    // Pack from a colour surface of the panel's size.
                    struct Pix;
                    impl fusuma_platform_embedded::Pixels for Pix {
                        fn row(&mut self, y: u32, x: u32, out: &mut [u16]) {
                            for (i, p) in out.iter_mut().enumerate() {
                                *p = ((x + i as u32 + y) * 977) as u16;
                            }
                        }
                    }
                    pack_mono(&mut Pix, (w, h), b, Dither::Ordered, &mut out);
                    assert_eq!(out.len(), (b.w as usize / 8) * b.h as usize);
                    // The same region in four grays: two bits a pixel, every level in range.
                    pack_gray4(&mut Pix, (w, h), b, Dither::Ordered, &mut out);
                    assert_eq!(out.len(), (b.w as usize / 4) * b.h as usize);
                    for l in [0u8, 85, 128, 255] {
                        assert!(gray_level(Dither::Ordered, u32::from(c[6]), u32::from(c[7]), l) < 4);
                    }
                }
            }
            _ => {
                use fusuma_render::{Color, Renderer};
                let colour = Color::rgb(c[6], c[7], c[10]);
                let _ = frame.fill_rect(r, u32::from(c[11]), colour);
                let _ = frame.fill_circle(Point::new(n(c, 0) / 40, n(c, 2) / 40), u32::from(c[4]), colour);
                for y in 0..250usize {
                    assert_eq!(frame.target().bytes()[y * 16 + 15] & 0b0011_1111, 0b0011_1111, "padding row {y}");
                }
            }
        }
    }
    let _ = Frame::new(1, 1, vec![0u16; 1]);
});
