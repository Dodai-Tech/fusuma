//! The surface of the renderer that was chosen, behind one type: draw the app, then show what changed.

use fusuma::App;
use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::renderers::embedded::{Embedded, Frame};

use crate::rgb565;

/// A drawing surface of one of the linked backends.
pub enum Surface {
    Integer(Embedded<Frame<Vec<u16>>>),
    #[cfg(feature = "skia")]
    Skia(fusuma::renderers::skia::Skia),
    #[cfg(feature = "vello-cpu")]
    Vello(fusuma::renderers::vello_cpu::VelloCpu),
}

impl Surface {
    /// A surface of `width` x `height` pixels for `backend`; `None` if it is not linked or cannot be made.
    pub fn new(backend: Backend, width: u32, height: u32) -> Option<Self> {
        match backend {
            Backend::Integer => {
                let pixels = vec![0u16; (width * height) as usize];
                let frame = Frame::new(u16::try_from(width).ok()?, u16::try_from(height).ok()?, pixels)?;
                Some(Self::Integer(Embedded::new(frame)))
            }
            #[cfg(feature = "skia")]
            Backend::Skia => fusuma::renderers::skia::Skia::new(width, height).map(Self::Skia),
            #[cfg(feature = "vello-cpu")]
            Backend::VelloCpu => fusuma::renderers::vello_cpu::new(width, height).map(Self::Vello),
            _ => None,
        }
    }

    /// Has the app draw what changed; the rectangle it redrew, if any.
    pub fn draw<A: App>(&mut self, app: &mut A) -> Option<Rect> {
        let drawn = match self {
            Self::Integer(r) => app.frame(r).map_err(|e| format!("{e:?}")),
            #[cfg(feature = "skia")]
            Self::Skia(r) => app.frame(r).map_err(|e| format!("{e:?}")),
            #[cfg(feature = "vello-cpu")]
            Self::Vello(r) => {
                // The scene forgets what it drew before; this frame redraws only the damaged part.
                r.with_scene(fusuma::renderers::vello_cpu::Scene::reset);
                app.frame(r).map_err(|e| format!("{e:?}"))
            }
        };
        match drawn {
            Ok(region) => region,
            Err(e) => {
                log::error!("draw failed: {e}");
                None
            }
        }
    }

    /// Copies `region` of the surface into the RGB565 `screen` (the whole frame buffer, `WIDTH` wide).
    pub fn present(&mut self, region: Rect, screen: &mut [u16]) {
        let (w, h) = (crate::hw::WIDTH, crate::hw::HEIGHT);
        let (x0, y0) = (region.x.max(0) as usize, region.y.max(0) as usize);
        let x1 = region.right().clamp(0, w as i32) as usize;
        let y1 = region.bottom().clamp(0, h as i32) as usize;
        match self {
            Self::Integer(r) => {
                let src = r.target().pixels();
                for y in y0..y1 {
                    screen[y * w + x0..y * w + x1].copy_from_slice(&src[y * w + x0..y * w + x1]);
                }
            }
            #[cfg(feature = "skia")]
            Self::Skia(r) => {
                let src = r.pixmap().data();
                rgba_to_screen(src, x0, y0, x1, y1, screen);
            }
            #[cfg(feature = "vello-cpu")]
            Self::Vello(r) => r.with_scene(|scene| {
                rgba_to_screen(fusuma::renderers::vello_cpu::Scene::pixels(scene), x0, y0, x1, y1, screen);
            }),
        }
    }
}

/// Converts the RGBA8 rows of a region to RGB565 into the screen.
#[cfg(any(feature = "skia", feature = "vello-cpu"))]
fn rgba_to_screen(rgba: &[u8], x0: usize, y0: usize, x1: usize, y1: usize, screen: &mut [u16]) {
    let w = crate::hw::WIDTH;
    for y in y0..y1 {
        for x in x0..x1 {
            let i = (y * w + x) * 4;
            screen[y * w + x] = rgb565(rgba[i], rgba[i + 1], rgba[i + 2]);
        }
    }
}
