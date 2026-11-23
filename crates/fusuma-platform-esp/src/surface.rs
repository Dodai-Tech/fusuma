//! The surface of the renderer that was chosen: draw the app, then show what changed.

use fusuma::App;
use fusuma_platform::Backend;
use fusuma_render::Rect;
use fusuma_render_vello_cpu::{Scene, VelloCpu};

use crate::rgb565;

/// A drawing surface. Vello on the CPU is the only backend on a board: there is no GPU `wgpu` can use.
pub struct Surface(VelloCpu);

impl Surface {
    /// A surface of `width` x `height` pixels for `backend`; `None` if it is not one a board can make.
    pub fn new(backend: Backend, width: u32, height: u32) -> Option<Self> {
        match backend {
            Backend::VelloCpu => fusuma_render_vello_cpu::new(width, height).map(Self),
            Backend::VelloGpu => None,
        }
    }

    /// Has the app draw what changed; the rectangle it redrew, if any.
    pub fn draw<A: App>(&mut self, app: &mut A) -> Option<Rect> {
        // The scene forgets what it drew before; this frame redraws only the damaged part.
        self.0.with_scene(Scene::reset);
        match app.frame(&mut self.0) {
            Ok(region) => region,
            Err(e) => {
                log::error!("draw failed: {e:?}");
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
        self.0.with_scene(|scene| {
            let rgba = scene.pixels();
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    screen[y * w + x] = rgb565(rgba[i], rgba[i + 1], rgba[i + 2]);
                }
            }
        });
    }
}
