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

use crate::board::Pixels;
#[cfg(any(feature = "skia", feature = "vello-cpu"))]
use crate::board::rgb565;

/// The backends linked into this build: the integer one always, the others by cargo feature.
pub const LINKED: &[Backend] = &[
    Backend::Integer,
    #[cfg(feature = "skia")]
    Backend::Skia,
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
    /// `tiny-skia`.
    #[cfg(feature = "skia")]
    Skia(Box<fusuma::renderers::skia::Skia>, u32),
    /// Vello on the CPU.
    #[cfg(feature = "vello-cpu")]
    Vello(Box<fusuma::renderers::vello_cpu::VelloCpu>, u32),
}

impl core::fmt::Debug for Surface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Integer(..) => "Surface::Integer",
            #[cfg(feature = "skia")]
            Self::Skia(..) => "Surface::Skia",
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
            #[cfg(feature = "skia")]
            Backend::Skia => fusuma::renderers::skia::Skia::new(width, height)
                .map(|s| Self::Skia(Box::new(s), width)),
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
            #[cfg(feature = "skia")]
            Self::Skia(r, _) => shell.frame(now_ms, &mut **r).map_err(|e| err(&e)),
            #[cfg(feature = "vello-cpu")]
            Self::Vello(r, _) => {
                // The scene forgets what it drew before; this frame redraws only the damaged part.
                r.with_scene(fusuma::renderers::vello_cpu::Scene::reset);
                let drawn = shell.frame(now_ms, &mut **r).map_err(|e| err(&e))?;
                // Only what changed has to be rasterised: a hover is a few hundred pixels, not the screen.
                if let Some(d) = drawn {
                    r.with_scene(|s| {
                        fusuma::renderers::vello_cpu::Scene::set_damage(s, (d.x, d.y, d.w, d.h));
                        // Vello rasterises lazily, on the first read of the pixels. Do it here, so the time is
                        // counted as drawing and reading rows is only a conversion.
                        let _ = fusuma::renderers::vello_cpu::Scene::pixels(s);
                    });
                }
                Ok(drawn)
            }
        }
    }
}

impl Pixels for Surface {
    fn row(&mut self, y: u32, x: u32, out: &mut [u16]) {
        match self {
            Self::Integer(r, w) => {
                let src = r.target().pixels();
                copy_row(src, *w, y, x, out);
            }
            #[cfg(feature = "skia")]
            Self::Skia(r, w) => rgba_row(r.pixmap().data(), *w, y, x, out),
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

/// Converts part of row `y` of a `w` wide RGBA8 picture to RGB565.
#[cfg(any(feature = "skia", feature = "vello-cpu"))]
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
