//! The drawing surface of the chosen backend and the persistent frame it fills.
//!
//! A Vello scene is drawn from nothing every frame, so after a partial redraw only the damaged region of its
//! pixels is valid. The canvas keeps the whole picture in `frame` and copies just that region into it; the
//! window then shows `frame`.

use fusuma::fonts::FontSet;
use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::renderers::vello_cpu::{self, Scene as _, VelloCpu};
use fusuma::renderers::vello_gpu::{self, VelloGpu};
use fusuma::{App, Shell};

/// A surface on one of the two Vello backends.
#[derive(Debug)]
enum Surface {
    /// Boxed, like the GPU one: a surface is made once, and this keeps the enum small.
    Cpu(Box<VelloCpu>),
    /// Boxed: a GPU scene is several kilobytes.
    Gpu(Box<VelloGpu>),
}

/// A surface and the picture drawn on it, as `0x00RRGGBB` pixels.
#[derive(Debug)]
pub struct Canvas {
    surface: Surface,
    width: u32,
    height: u32,
    frame: Vec<u32>,
    backend: Backend,
}

impl Canvas {
    /// A canvas of `width` x `height` pixels on `backend`, drawing the font roles with `fonts`; `None` if that
    /// surface cannot be made (no GPU, or a size Vello does not take).
    pub fn new(backend: Backend, width: u32, height: u32, fonts: FontSet) -> Option<Self> {
        let surface = match backend {
            Backend::VelloCpu => {
                let mut r = vello_cpu::new(width, height)?;
                r.set_fonts(fonts);
                Surface::Cpu(Box::new(r))
            }
            Backend::VelloGpu => match vello_gpu::new(width, height) {
                Ok(mut s) => {
                    s.set_fonts(fonts);
                    Surface::Gpu(Box::new(s))
                }
                Err(e) => {
                    log::warn!("no GPU surface: {e}");
                    return None;
                }
            },
            _ => return None,
        };
        Some(Self {
            surface,
            width,
            height,
            frame: vec![0; (width as usize) * (height as usize)],
            backend,
        })
    }

    /// The backend this canvas draws with.
    pub const fn backend(&self) -> Backend {
        self.backend
    }

    /// Width and height in pixels.
    pub const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The whole picture, row by row, `0x00RRGGBB`.
    pub fn pixels(&self) -> &[u32] {
        &self.frame
    }

    /// One frame of the shell: time passes, the app draws what changed, and that part of the picture is
    /// updated. Returns the redrawn rectangle, or `None` when nothing changed.
    pub fn frame<A: App>(&mut self, shell: &mut Shell<A>, now_ms: u64) -> Option<Rect> {
        let (width, frame) = (self.width, &mut self.frame);
        match &mut self.surface {
            Surface::Cpu(r) => {
                let r: &mut VelloCpu = r;
                r.with_scene(vello_cpu::Scene::reset);
                let region = log_err(shell.frame(now_ms, r))?;
                // Only what changed is rasterised: the toolkit announced each separate rectangle itself, and a
                // single one is announced here.
                let single = shell.frame_rects().len() < 2;
                r.with_scene(|s| {
                    if single {
                        vello_cpu::Scene::set_damage(s, (region.x, region.y, region.w, region.h));
                    }
                    blit(s.pixels(), width, region, frame);
                });
                Some(region)
            }
            Surface::Gpu(r) => {
                let r: &mut VelloGpu = r;
                r.with_scene(vello_gpu::Scene::reset);
                let region = log_err(shell.frame(now_ms, r))?;
                r.with_scene(|s| blit(s.pixels(), width, region, frame));
                Some(region)
            }
        }
    }
}

fn log_err<T, E: core::fmt::Debug>(r: Result<Option<T>, E>) -> Option<T> {
    match r {
        Ok(v) => v,
        Err(e) => {
            log::error!("draw failed: {e:?}");
            None
        }
    }
}

/// Copies `region` of RGBA8 pixels (`width` wide) into `dst` as `0x00RRGGBB`. The region is cut to the
/// picture, and a source that is too short (a failed render) is left out instead of panicking.
pub fn blit(rgba: &[u8], width: u32, region: Rect, dst: &mut [u32]) {
    let w = width as usize;
    if w == 0 {
        return;
    }
    let height = (dst.len() / w).min(rgba.len() / 4 / w);
    let x0 = region.x.max(0) as usize;
    let y0 = region.y.max(0) as usize;
    let x1 = (region.right().max(0) as usize).min(w);
    let y1 = (region.bottom().max(0) as usize).min(height);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = y * w + x;
            let p = &rgba[i * 4..i * 4 + 4];
            dst[i] = (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fusuma::Pointer;
    use fusuma::render::Point;
    use fusuma_showcase::Showcase;

    fn rgba(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&f(x, y));
            }
        }
        v
    }

    #[test]
    fn blit_converts_rgba_to_0rgb_inside_the_region_only() {
        let src = rgba(4, 3, |x, y| [x as u8 * 10, y as u8 * 10, 7, 255]);
        let mut dst = vec![0xAAAAAA; 12];
        blit(&src, 4, Rect::new(1, 1, 2, 1), &mut dst);
        assert_eq!(dst[5], (10 << 16) | (10 << 8) | 7);
        assert_eq!(dst[6], (20 << 16) | (10 << 8) | 7);
        let touched = dst.iter().filter(|&&p| p != 0xAAAAAA).count();
        assert_eq!(touched, 2, "only the two pixels of the region");
    }

    #[test]
    fn blit_cuts_the_region_to_the_picture_and_survives_bad_input() {
        let src = rgba(4, 3, |_, _| [1, 2, 3, 255]);
        let mut dst = vec![0; 12];
        blit(&src, 4, Rect::new(-5, -5, 100, 100), &mut dst);
        assert!(
            dst.iter().all(|&p| p == 0x010203),
            "the whole picture, nothing outside it"
        );
        let mut dst = vec![0; 12];
        blit(&src[..8], 4, Rect::new(0, 0, 4, 3), &mut dst);
        assert_eq!(
            dst.iter().filter(|&&p| p != 0).count(),
            0,
            "a short source copies nothing"
        );
        blit(&src, 0, Rect::new(0, 0, 4, 3), &mut dst);
        blit(&src, 4, Rect::new(i32::MAX, i32::MAX, 5, 5), &mut dst);
        blit(&src, 4, Rect::new(i32::MIN, 0, 9, 9), &mut dst);
    }

    fn showcase_canvas(backend: Backend) -> Option<(Canvas, Shell<Showcase>)> {
        let canvas = Canvas::new(backend, 1024, 600, FontSet::default())?;
        let mut app = Showcase::new(1024, 600);
        // The Buttons page has the widgets these tests press.
        app.set_start(Some("Buttons"), None, None, true, false);
        Some((canvas, Shell::new(app)))
    }

    fn first_frame_and_a_click(backend: Backend) {
        let Some((mut canvas, mut shell)) = showcase_canvas(backend) else {
            eprintln!("{} is not available here, skipped", backend.name());
            return;
        };
        let region = canvas.frame(&mut shell, 0).unwrap();
        assert_eq!(region, Rect::new(0, 0, 1024, 600));
        let bg = shell.app.theme().colors.background;
        let want = (u32::from(bg.r) << 16) | (u32::from(bg.g) << 8) | u32::from(bg.b);
        assert_eq!(canvas.pixels()[0], want, "the corner is the page colour");
        assert_eq!(
            canvas.frame(&mut shell, 16),
            None,
            "nothing changed, nothing drawn"
        );
        // Switch the theme with a click: everything is redrawn in the other colours.
        let b = shell
            .app
            .rect_of(shell.app.theme_button().unwrap())
            .unwrap();
        let at = Point::new(b.x + 4, b.y + 4);
        shell.input(fusuma::Input::Pointer(Pointer::Down(at)));
        shell.input(fusuma::Input::Pointer(Pointer::Up(at)));
        // The dropdown opens: only its menu is drawn, over the page.
        let opened = canvas.frame(&mut shell, 32).unwrap();
        // (A backend that keeps no picture between frames, the GPU, is given the whole screen instead.)
        if backend == Backend::VelloCpu {
            assert!(opened.w * opened.h < 1024 * 600 / 2, "{opened:?}");
        }
        let light = shell.app.theme_row(1).unwrap();
        let r = shell.app.rect_of(light).unwrap();
        let at = Point::new(r.x + 4, r.y + 4);
        shell.input(fusuma::Input::Pointer(Pointer::Down(at)));
        shell.input(fusuma::Input::Pointer(Pointer::Up(at)));
        let region = canvas.frame(&mut shell, 48).unwrap();
        assert_eq!(region, Rect::new(0, 0, 1024, 600));
        assert_ne!(
            canvas.pixels()[0],
            want,
            "the light theme has another background"
        );
    }

    #[test]
    fn the_cpu_canvas_draws_the_showcase_and_redraws_on_a_click() {
        first_frame_and_a_click(Backend::VelloCpu);
    }

    #[test]
    fn the_gpu_canvas_does_the_same_where_there_is_a_gpu() {
        first_frame_and_a_click(Backend::VelloGpu);
    }

    #[test]
    fn a_partial_redraw_keeps_the_rest_of_the_picture() {
        let (mut canvas, mut shell) = showcase_canvas(Backend::VelloCpu).unwrap();
        canvas.frame(&mut shell, 0).unwrap();
        let before = canvas.pixels().to_vec();
        let w = shell.app.widgets().click_me.unwrap();
        let r = shell.app.rect_of(w).unwrap();
        let at = Point::new(r.x + 4, r.y + 4);
        shell.input(fusuma::Input::Pointer(Pointer::Move(at)));
        let region = canvas.frame(&mut shell, 16).unwrap();
        assert!(
            u64::from(region.w) * u64::from(region.h) < 1024 * 600 / 2,
            "a small part: {region:?}"
        );
        let changed = canvas
            .pixels()
            .iter()
            .zip(&before)
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 0, "the hover changed something");
        // Everything outside the region is exactly as it was.
        for y in 0..600usize {
            for x in 0..1024usize {
                let inside = (x as i32) >= region.x
                    && (x as i32) < region.right()
                    && (y as i32) >= region.y
                    && (y as i32) < region.bottom();
                if !inside {
                    assert_eq!(
                        canvas.pixels()[y * 1024 + x],
                        before[y * 1024 + x],
                        "({x}, {y})"
                    );
                }
            }
        }
    }

    #[test]
    fn sizes_vello_does_not_take_make_no_canvas() {
        assert!(Canvas::new(Backend::VelloCpu, 0, 10, FontSet::default()).is_none());
        assert!(Canvas::new(Backend::VelloCpu, 70_000, 10, FontSet::default()).is_none());
        assert!(
            Canvas::new(Backend::Integer, 10, 10, FontSet::default()).is_none(),
            "not a desktop backend"
        );
        let c = Canvas::new(Backend::VelloCpu, 8, 6, FontSet::default()).unwrap();
        assert_eq!(
            (c.size(), c.backend(), c.pixels().len()),
            ((8, 6), Backend::VelloCpu, 48)
        );
    }

    /// After clicks that change several separate parts (the old and the new sidebar row, the page), the GPU canvas
    /// still shows the whole picture, like the CPU one: no black gaps between the redrawn parts.
    #[test]
    fn the_gpu_picture_matches_the_cpu_picture_after_clicks() {
        let Some((mut gpu, mut gs)) = showcase_canvas(Backend::VelloGpu) else {
            eprintln!("no GPU here, skipped");
            return;
        };
        let (mut cpu, mut cs) = showcase_canvas(Backend::VelloCpu).unwrap();
        gpu.frame(&mut gs, 0).unwrap();
        cpu.frame(&mut cs, 0).unwrap();
        let mut now = 0;
        for k in [1usize, 3, 2, 5] {
            let page = fusuma_showcase::Page::ALL[k];
            for (canvas, shell) in [(&mut gpu, &mut gs), (&mut cpu, &mut cs)] {
                let row = shell.app.sidebar_item(page).unwrap();
                let r = shell.app.rect_of(row).unwrap();
                let at = Point::new(r.x + 4, r.y + 4);
                shell.input(fusuma::Input::Pointer(Pointer::Down(at)));
                shell.input(fusuma::Input::Pointer(Pointer::Up(at)));
                for step in 1..=4 {
                    canvas.frame(shell, now + step * 100);
                }
            }
            now += 400;
            let black = gpu
                .pixels()
                .iter()
                .zip(cpu.pixels())
                .filter(|(g, c)| **g == 0 && **c != 0)
                .count();
            assert_eq!(black, 0, "black pixels on the GPU canvas after row {k}");
        }
    }

    /// The two Vello backends draw the same picture: where they differ it is by anti-aliasing at the edges,
    /// not by a missing widget or wrong colour. Skipped where there is no GPU.
    #[test]
    fn the_gpu_picture_matches_the_cpu_picture() {
        let Some((mut gpu, mut gs)) = showcase_canvas(Backend::VelloGpu) else {
            eprintln!("no GPU here, skipped");
            return;
        };
        let (mut cpu, mut cs) = showcase_canvas(Backend::VelloCpu).unwrap();
        gpu.frame(&mut gs, 0).unwrap();
        cpu.frame(&mut cs, 0).unwrap();
        let far = |a: u32, b: u32| {
            [16, 8, 0]
                .iter()
                .any(|&sh| ((a >> sh) & 0xFF).abs_diff((b >> sh) & 0xFF) > 24)
        };
        let differing = gpu
            .pixels()
            .iter()
            .zip(cpu.pixels())
            .filter(|(a, b)| far(**a, **b))
            .count();
        let total = 1024 * 600;
        assert!(
            differing * 100 < total,
            "{differing} of {total} pixels differ clearly between the GPU and the CPU"
        );
    }
}
