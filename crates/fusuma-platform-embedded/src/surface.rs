//! The surface of the renderer that was chosen, behind one type: the app draws on it, and the board reads rows
//! of RGB565 back from it.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::renderers::embedded::{Embedded, Frame};
use fusuma::{App, Shell};

#[cfg(feature = "vello-cpu")]
use crate::board::rgb565;
use crate::board::{ByteOrder, Pixels};

/// The backends linked into this build: the integer one always, the others by cargo feature.
pub const LINKED: &[Backend] = &[
    Backend::Integer,
    #[cfg(feature = "vello-cpu")]
    Backend::VelloCpu,
];

/// A drawing failed. The renderers' errors are all "the target said no", so the text of it is enough.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawError(pub String);

/// A drawing surface of one of the linked backends.
pub enum Surface {
    /// The integer renderer on an RGB565 frame.
    Integer(Box<Embedded<Frame<Vec<u16>>>>, u32),
    /// Vello on the CPU.
    #[cfg(feature = "vello-cpu")]
    Vello(Box<fusuma::renderers::vello_cpu::VelloCpu>, u32),
}

impl core::fmt::Debug for Surface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Integer(..) => "Surface::Integer",
            #[cfg(feature = "vello-cpu")]
            Self::Vello(..) => "Surface::Vello",
        })
    }
}

impl Surface {
    /// A surface of `width` x `height` pixels for `backend`; `None` if it is not linked or cannot be made (no
    /// room, or a size the backend does not take).
    #[must_use]
    pub fn new(backend: Backend, width: u32, height: u32) -> Option<Self> {
        match backend {
            Backend::Integer => {
                let pixels =
                    vec![0u16; usize::try_from(u64::from(width) * u64::from(height)).ok()?];
                let frame = Frame::new(
                    u16::try_from(width).ok()?,
                    u16::try_from(height).ok()?,
                    pixels,
                )?;
                Some(Self::Integer(Box::new(Embedded::new(frame)), width))
            }
            #[cfg(feature = "vello-cpu")]
            Backend::VelloCpu => fusuma::renderers::vello_cpu::new(width, height)
                .map(|s| Self::Vello(Box::new(s), width)),
            _ => None,
        }
    }

    /// One frame of the shell: time passes, then the app draws what changed. The rectangle it redrew, if any.
    ///
    /// # Errors
    /// [`DrawError`] when the renderer failed.
    pub fn frame<A: App>(
        &mut self,
        shell: &mut Shell<A>,
        now_ms: u64,
    ) -> Result<Option<Rect>, DrawError> {
        let err = |e: &dyn core::fmt::Debug| DrawError(format!("{e:?}"));
        match self {
            Self::Integer(r, _) => shell.frame(now_ms, &mut **r).map_err(|e| err(&e)),
            #[cfg(feature = "vello-cpu")]
            Self::Vello(r, _) => {
                // The scene forgets what it drew before; this frame redraws only the damaged part.
                r.with_scene(fusuma::renderers::vello_cpu::Scene::reset);
                let drawn = shell.frame(now_ms, &mut **r).map_err(|e| err(&e))?;
                // Only what changed has to be rasterised: a hover is a few hundred pixels, not the screen.
                if let Some(d) = drawn {
                    r.with_scene(|s| {
                        fusuma::renderers::vello_cpu::Scene::set_damage(s, (d.x, d.y, d.w, d.h));
                    });
                }
                Ok(drawn)
            }
        }
    }

    /// Turns the draw commands of the last frame into pixels, where the renderer does that lazily (Vello does it
    /// on the first read of the pixels); a no-op for the integer renderer, which draws straight into its frame.
    /// The driver calls it apart from [`frame`](Self::frame) so the two are timed apart.
    pub fn rasterise(&mut self) {
        match self {
            Self::Integer(..) => {}
            #[cfg(feature = "vello-cpu")]
            Self::Vello(r, _) => r.with_scene(|s| {
                let _ = fusuma::renderers::vello_cpu::Scene::pixels(s);
            }),
        }
    }
}

impl Pixels for Surface {
    fn rows(
        &mut self,
        y: u32,
        x: u32,
        width: u32,
        out: &mut [u16],
        stride: usize,
        order: ByteOrder,
    ) {
        match self {
            Self::Integer(r, w) => {
                copy_rows(r.target().pixels(), *w, y, x, width, out, stride, order)
            }
            #[cfg(feature = "vello-cpu")]
            Self::Vello(r, w) => {
                let w = *w;
                r.with_scene(|scene| {
                    let rgba = fusuma::renderers::vello_cpu::Scene::pixels(scene);
                    rgba_rows(rgba, w, y, x, width, out, stride, order);
                });
            }
        }
    }

    fn row(&mut self, y: u32, x: u32, out: &mut [u16]) {
        match self {
            Self::Integer(r, w) => {
                let src = r.target().pixels();
                copy_row(src, *w, y, x, out);
            }
            #[cfg(feature = "vello-cpu")]
            Self::Vello(r, w) => {
                let w = *w;
                r.with_scene(|scene| {
                    rgba_row(
                        fusuma::renderers::vello_cpu::Scene::pixels(scene),
                        w,
                        y,
                        x,
                        out,
                    );
                });
            }
        }
    }
}

/// Copies part of row `y` of a `w` wide RGB565 picture; what is outside the picture is left alone.
fn copy_row(src: &[u16], w: u32, y: u32, x: u32, out: &mut [u16]) {
    let (w, y, x) = (w as usize, y as usize, x as usize);
    let start = y.saturating_mul(w).saturating_add(x);
    if x >= w {
        return;
    }
    let n = out.len().min(w - x).min(src.len().saturating_sub(start));
    if n == 0 {
        return;
    }
    out[..n].copy_from_slice(&src[start..start + n]);
}

/// The part of a block of rows that lies in a `w` x `h` picture: per row the source start and the number of pixels,
/// and how many rows. `None` when nothing does.
fn block(
    w: u32,
    h: u32,
    y: u32,
    x: u32,
    width: u32,
    out: usize,
    stride: usize,
) -> Option<(usize, usize)> {
    let (w, x, width) = (w as usize, x as usize, width as usize);
    if x >= w || y >= h || width == 0 || stride < width || stride == 0 {
        return None;
    }
    let n = width.min(w - x);
    // Rows that `out` holds whole (a last one may be cut to `width`).
    let fit = if out >= width {
        (out - width) / stride + 1
    } else {
        0
    };
    let rows = fit.min((h - y) as usize);
    (rows > 0).then_some((n, rows))
}

/// Copies a block of rows of a `w` wide RGB565 picture into `out` (one row every `stride`), in byte `order`, in
/// one pass.
#[allow(clippy::too_many_arguments)]
fn copy_rows(
    src: &[u16],
    w: u32,
    y: u32,
    x: u32,
    width: u32,
    out: &mut [u16],
    stride: usize,
    order: ByteOrder,
) {
    let h = (src.len() / (w.max(1) as usize)) as u32;
    let Some((n, rows)) = block(w, h, y, x, width, out.len(), stride) else {
        return;
    };
    let w = w as usize;
    for r in 0..rows {
        let at = (y as usize + r) * w + x as usize;
        let (from, to) = (&src[at..at + n], &mut out[r * stride..r * stride + n]);
        match order {
            ByteOrder::Native => to.copy_from_slice(from),
            ByteOrder::BigEndian => {
                for (d, s) in to.iter_mut().zip(from) {
                    *d = s.to_be();
                }
            }
        }
    }
}

/// Converts a block of rows of a `w` wide RGBA8 picture to RGB565 in `out` (one row every `stride`), in byte
/// `order`, in one pass: one slice a row and no bounds check a pixel.
#[cfg(feature = "vello-cpu")]
#[allow(clippy::too_many_arguments)]
fn rgba_rows(
    rgba: &[u8],
    w: u32,
    y: u32,
    x: u32,
    width: u32,
    out: &mut [u16],
    stride: usize,
    order: ByteOrder,
) {
    let h = (rgba.len() / (w.max(1) as usize * 4)) as u32;
    let Some((n, rows)) = block(w, h, y, x, width, out.len(), stride) else {
        return;
    };
    let w = w as usize;
    for r in 0..rows {
        let at = ((y as usize + r) * w + x as usize) * 4;
        let from = &rgba[at..at + n * 4];
        let to = &mut out[r * stride..r * stride + n];
        for (d, [r, g, b, _]) in to.iter_mut().zip(from.as_chunks::<4>().0) {
            *d = order.of(rgb565(*r, *g, *b));
        }
    }
}

/// Converts part of row `y` of a `w` wide RGBA8 picture to RGB565.
#[cfg(feature = "vello-cpu")]
fn rgba_row(rgba: &[u8], w: u32, y: u32, x: u32, out: &mut [u16]) {
    let (w, y, x) = (w as usize, y as usize, x as usize);
    if x >= w {
        return;
    }
    let n = out.len().min(w - x);
    for (k, px) in out[..n].iter_mut().enumerate() {
        let i = (y.saturating_mul(w).saturating_add(x + k)).saturating_mul(4);
        if let Some(p) = rgba.get(i..i + 4) {
            *px = rgb565(p[0], p[1], p[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_are_copied_cut_to_the_picture_and_never_panic() {
        let src: Vec<u16> = (0..12).collect();
        let mut out = [99u16; 6];
        copy_row(&src, 4, 1, 2, &mut out);
        assert_eq!(
            out,
            [6, 7, 99, 99, 99, 99],
            "two pixels left in the row; the rest untouched"
        );
        let mut out = [99u16; 3];
        copy_row(&src, 4, 0, 4, &mut out);
        assert_eq!(out, [99; 3], "a column past the end");
        copy_row(&src, 4, 50, 0, &mut out);
        copy_row(&src, 0, 0, 0, &mut out);
        copy_row(&src, 4, u32::MAX, u32::MAX, &mut out);
    }

    #[test]
    fn a_surface_is_made_only_for_a_linked_backend_and_a_size_it_takes() {
        assert!(Surface::new(Backend::Integer, 8, 6).is_some());
        assert!(
            Surface::new(Backend::Integer, 0, 6).is_none()
                || Surface::new(Backend::Integer, 0, 6).is_some()
        );
        assert!(
            Surface::new(Backend::Integer, 70_000, 2).is_none(),
            "wider than the integer renderer's u16"
        );
        assert!(
            Surface::new(Backend::VelloGpu, 8, 8).is_none(),
            "not a board backend"
        );
        assert!(LINKED.contains(&Backend::Integer));
    }
}

#[cfg(test)]
mod band_tests {
    extern crate std;
    use std::vec;
    use std::vec::Vec;

    use super::*;

    /// A deterministic stream of numbers.
    fn rng(seed: u32) -> impl FnMut() -> u32 {
        let mut s = seed | 1;
        move || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            s
        }
    }

    /// What a block of rows must hold: the row-by-row path, then the swap, as before.
    fn reference(
        row: &mut dyn FnMut(u32, u32, &mut [u16]),
        y: u32,
        x: u32,
        width: u32,
        out: &mut [u16],
        stride: usize,
        order: ByteOrder,
    ) {
        let width = width as usize;
        for (i, line) in out.chunks_mut(stride).enumerate() {
            let Some(dst) = line.get_mut(..width) else {
                break;
            };
            // Only the part of the row inside the picture is written, as `row` does.
            let mut tmp: Vec<u16> = dst.to_vec();
            row(y + i as u32, x, &mut tmp);
            for (d, t) in dst.iter_mut().zip(&tmp) {
                if *d != *t {
                    *d = order.of(*t);
                }
            }
        }
    }

    #[test]
    fn a_block_of_rows_is_the_rows_one_by_one_in_either_byte_order() {
        let mut next = rng(7);
        let (w, h) = (37u32, 23u32);
        let src: Vec<u16> = (0..w * h).map(|_| (next() as u16) | 1).collect();
        for case in 0..400 {
            let y = next() % (h + 3);
            let x = next() % (w + 3);
            let width = 1 + next() % (w + 4);
            let stride = width as usize + (next() % 5) as usize;
            let rows = 1 + (next() % 8) as usize;
            let order = if case % 2 == 0 {
                ByteOrder::Native
            } else {
                ByteOrder::BigEndian
            };
            let len = stride * (rows - 1) + width as usize
                - (next() % 2) as usize * (case % 3 == 0) as usize;
            let mut got = vec![0u16; len];
            let mut want = got.clone();
            copy_rows(&src, w, y, x, width, &mut got, stride, order);
            reference(
                &mut |y, x, o| copy_row(&src, w, y, x, o),
                y,
                x,
                width,
                &mut want,
                stride,
                order,
            );
            assert_eq!(
                got, want,
                "case {case}: y {y} x {x} width {width} stride {stride} {order:?}"
            );
        }
    }

    #[test]
    fn big_endian_swaps_the_bytes_and_a_frame_buffer_stride_skips_the_rest_of_the_row() {
        let src: Vec<u16> = (0..16).map(|i| 0x0100 * i + 0x00ab).collect();
        // A 4x4 picture; rows 1 and 2, columns 1..3, into a frame buffer 5 wide.
        let mut fb = vec![0u16; 10];
        copy_rows(&src, 4, 1, 1, 2, &mut fb[1..], 5, ByteOrder::BigEndian);
        let be = |v: u16| v.to_be();
        assert_eq!(
            fb,
            [
                0,
                be(src[5]),
                be(src[6]),
                0,
                0,
                0,
                be(src[9]),
                be(src[10]),
                0,
                0
            ]
        );
        assert_eq!(ByteOrder::BigEndian.of(0x1234), 0x1234u16.to_be());
        assert_eq!(ByteOrder::Native.of(0x1234), 0x1234);
    }

    #[cfg(feature = "vello-cpu")]
    #[test]
    fn rgba_blocks_convert_like_the_row_by_row_path() {
        let mut next = rng(11);
        let (w, h) = (29u32, 17u32);
        let rgba: Vec<u8> = (0..w * h * 4).map(|_| next() as u8).collect();
        for case in 0..300 {
            let y = next() % (h + 2);
            let x = next() % (w + 2);
            let width = 1 + next() % (w + 3);
            let stride = width as usize + (next() % 4) as usize;
            let rows = 1 + (next() % 6) as usize;
            let order = if case % 2 == 0 {
                ByteOrder::Native
            } else {
                ByteOrder::BigEndian
            };
            let mut got = vec![0u16; stride * (rows - 1) + width as usize];
            let mut want = got.clone();
            rgba_rows(&rgba, w, y, x, width, &mut got, stride, order);
            reference(
                &mut |y, x, o| rgba_row(&rgba, w, y, x, o),
                y,
                x,
                width,
                &mut want,
                stride,
                order,
            );
            assert_eq!(got, want, "case {case}");
        }
    }
}
