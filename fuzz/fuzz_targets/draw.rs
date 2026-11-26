#![no_main]
//! Random drawing calls never panic on either backend, whatever the coordinates, and the save
//! stack, clip and translation stay consistent.

use libfuzzer_sys::fuzz_target;
use fusuma_render::{Align, Color, Image, PathCmd, Point, Probe, Rect, Renderer, TextStyle};
use fusuma_render_integer::{Dither, Embedded, Frame, MonoFrame};


struct Bytes<'a>(&'a [u8]);

impl Bytes<'_> {
    fn u8(&mut self) -> u8 {
        let (&b, rest) = self.0.split_first().unwrap_or((&0, &[]));
        self.0 = rest;
        b
    }
    /// Mostly small values, sometimes huge ones.
    fn i32(&mut self) -> i32 {
        let b = self.u8();
        match b % 8 {
            0 => i32::MIN + i32::from(self.u8()),
            1 => i32::MAX - i32::from(self.u8()),
            _ => i32::from(self.u8()) - 64 + (i32::from(self.u8() % 4) << 6),
        }
    }
    fn u32(&mut self) -> u32 {
        match self.u8() % 8 {
            0 => u32::MAX - u32::from(self.u8()),
            _ => u32::from(self.u8()) % 90,
        }
    }
    fn point(&mut self) -> Point {
        Point::new(self.i32(), self.i32())
    }
    fn rect(&mut self) -> Rect {
        Rect::new(self.i32(), self.i32(), self.u32(), self.u32())
    }
    fn color(&mut self) -> Color {
        Color::rgba(self.u8(), self.u8(), self.u8(), self.u8())
    }
}

fn run<R: Renderer + Probe>(r: &mut R, data: &[u8]) {
    let mut b = Bytes(data);
    let mut depth = 0usize;
    for _ in 0..64 {
        match b.u8() % 14 {
            0 => {
                if r.save().is_ok() {
                    depth += 1;
                }
            }
            1 => {
                if r.restore().is_ok() {
                    depth -= 1;
                }
            }
            2 => drop(r.clip(b.rect())),
            3 => drop(r.translate(b.i32(), b.i32())),
            4 => drop(r.clear(b.color())),
            5 => drop(r.fill_rect(b.rect(), b.u32(), b.color())),
            6 => drop(r.stroke_rect(b.rect(), b.u32(), b.u32(), b.color())),
            7 => drop(r.fill_circle(b.point(), b.u32(), b.color())),
            8 | 9 => {
                let mut path = Vec::new();
                for _ in 0..(b.u8() % 12) {
                    path.push(match b.u8() % 5 {
                        0 => PathCmd::MoveTo(b.point()),
                        1 => PathCmd::LineTo(b.point()),
                        2 => PathCmd::QuadTo(b.point(), b.point()),
                        3 => PathCmd::CubicTo(b.point(), b.point(), b.point()),
                        _ => PathCmd::Close,
                    });
                }
                let (w, c) = (b.u32(), b.color());
                if b.u8() % 2 == 0 {
                    drop(r.fill_path(&path, c));
                } else {
                    drop(r.stroke_path(&path, w, c));
                }
            }
            10 | 11 => {
                let text = ["", "Hi", "Hibari OS", "é€"][usize::from(b.u8() % 4)];
                let align = [Align::Left, Align::Center, Align::Right][usize::from(b.u8() % 3)];
                let style = TextStyle::new(b.u32(), b.color()).aligned(align);
                let at = b.point();
                let _ = r.measure_text(text, &style);
                drop(r.draw_text(text, at, &style));
            }
            12 => {
                let (w, h) = (u32::from(b.u8() % 5), u32::from(b.u8() % 5));
                let px: Vec<u8> = (0..w * h * 4).map(|_| b.u8()).collect();
                if let Some(img) = Image::new(w, h, &px) {
                    drop(r.draw_image(&img, b.point()));
                }
            }
            _ => {
                let _ = r.pixel(b.i32(), b.i32());
            }
        }
    }
    // Every save can be undone, and nothing remains.
    for _ in 0..depth {
        assert!(r.restore().is_ok());
    }
    assert!(r.restore().is_err());
}

fuzz_target!(|data: &[u8]| {
    let mut e = Embedded::new(Frame::new(48, 40, vec![0u16; 48 * 40]).unwrap());
    run(&mut e, data);
    let mut s = fusuma_render_vello_cpu::new(48, 40).unwrap();
    run(&mut s, data);
    // The one-bit frame (e-paper), both ways of turning colours to black and white; 45 wide so the last byte of a row is partly
    // padding, which must stay white.
    for dither in [Dither::Threshold(170), Dither::Ordered] {
        let mut m = Embedded::new(MonoFrame::new(45, 40, vec![0xFFu8; 6 * 40], dither).unwrap());
        run(&mut m, data);
        for y in 0..40 {
            assert_eq!(m.target().bytes()[y * 6 + 5] & 0b0000_0111, 0b0000_0111, "padding row {y}");
        }
    }
});
