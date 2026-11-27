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

use crate::Surfaces;

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
    bufs: Bufs,
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
            bufs: Bufs::default(),
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
        self.frame_with(shell, now_ms, &mut Surfaces::default())
    }

    /// Like [`frame`](Self::frame), with `hosts` for the surfaces of the app's layer plan (see `fusuma::LayerPlan`). After the
    /// interface is drawn, the plan is gone through back to front: a surface the frame drew around or over (or that lies
    /// under one that was just filled) is filled and put in the picture inside the part of it that can be seen, and what
    /// the app put inside it is drawn over it. Last, dialogs, menus and toasts are drawn again over the places the host
    /// filled, since its pixels covered them. The app is told the host fills its surfaces (`App::host_surfaces`), so what is
    /// inside a surface is not drawn under the host's pixels. Pixels the host writes are opaque; a texture is stretched over
    /// the surface (GPU backend only).
    pub fn frame_with<A: App>(
        &mut self,
        shell: &mut Shell<A>,
        now_ms: u64,
        hosts: &mut Surfaces,
    ) -> Option<Rect> {
        shell.app.host_surfaces(hosts.any());
        let (width, frame) = (self.width, &mut self.frame);
        let bufs = &mut self.bufs;
        match &mut self.surface {
            Surface::Cpu(r) => {
                let r: &mut VelloCpu = r;
                r.with_scene(vello_cpu::Scene::reset);
                let drawn = log_err(shell.frame(now_ms, r))?;
                // Only what changed is rasterised: the toolkit announced each separate rectangle itself, and a
                // single one is announced here.
                let single = shell.frame_rects().len() < 2;
                let region = if hosts.any() {
                    compose(shell, r, drawn, single, |r, id, rect| {
                        hosts
                            .pixels
                            .as_mut()
                            .is_some_and(|f| put_pixels(r, bufs, f, id, rect))
                    })
                } else {
                    drawn
                };
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
                let drawn = log_err(shell.frame(now_ms, r))?;
                // The scene keeps its picture, draws only what changed over it and reads back only that.
                let single = shell.frame_rects().len() < 2;
                let region = if hosts.any() {
                    let Surfaces { pixels, textures } = hosts;
                    compose(shell, r, drawn, single, |r, id, rect| {
                        if let Some(t) = textures.as_mut() {
                            let (device, queue) = r.with_scene(|s| s.device());
                            if let Some(texture) = t(id, rect, &device, &queue) {
                                let source = r.with_scene(|s| s.external_texture(&texture));
                                let size = (texture.width(), texture.height());
                                return r.draw_source(source, size, rect, true).is_ok();
                            }
                        }
                        pixels
                            .as_mut()
                            .is_some_and(|f| put_pixels(r, bufs, f, id, rect))
                    })
                } else {
                    drawn
                };
                r.with_scene(|s| {
                    if single {
                        vello_gpu::Scene::set_damage(s, (region.x, region.y, region.w, region.h));
                    }
                    blit(s.pixels(), width, region, frame);
                });
                Some(region)
            }
        }
    }
}

/// The buffers a surface is filled through.
#[derive(Debug, Default)]
struct Bufs {
    pixels: Vec<u32>,
    rgba: Vec<u8>,
}

/// The most pixels a surface the host fills may have: a bigger one (scrolled far out of a long list, say) is left empty.
const MAX_SURFACE_PIXELS: usize = 1 << 26;

/// Asks `fill` for the pixels of surface `id` (`rect` is the whole of it) and draws them. Whether anything was drawn.
fn put_pixels<R: fusuma::render::Renderer>(
    r: &mut R,
    bufs: &mut Bufs,
    fill: &mut crate::FillSurface,
    id: u32,
    rect: Rect,
) -> bool {
    let (w, h) = (rect.w as usize, rect.h as usize);
    if w.saturating_mul(h) > MAX_SURFACE_PIXELS {
        return false;
    }
    bufs.pixels.clear();
    bufs.pixels.resize(w * h, 0);
    fill(id, rect, &mut bufs.pixels);
    bufs.rgba.clear();
    bufs.rgba.reserve(w * h * 4);
    bufs.rgba.extend(
        bufs.pixels
            .iter()
            .flat_map(|p| [(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]),
    );
    let Some(image) = fusuma::render::Image::new(rect.w, rect.h, &bufs.rgba) else {
        return false;
    };
    match r.draw_image(&image, fusuma::render::Point::new(rect.x, rect.y)) {
        Ok(()) => true,
        Err(e) => {
            log::error!("a surface could not be drawn: {e:?}");
            false
        }
    }
}

/// Goes through the layer plan back to front. `put` draws surface `id` (all of `rect`) with the host's picture and says
/// whether it did; it is called with the clip set to the part that can be seen. Returns the rectangle that changed: the
/// frame's and the surfaces'.
fn compose<A: App, R: fusuma::render::Renderer>(
    shell: &mut Shell<A>,
    r: &mut R,
    drawn: Rect,
    single: bool,
    mut put: impl FnMut(&mut R, u32, Rect) -> bool,
) -> Rect {
    let Some(plan) = shell.layers() else {
        return drawn;
    };
    let mut total = drawn;
    // What is new in the picture so far: the frame, and each surface put in. A surface under any of it is drawn again.
    let mut fresh = vec![drawn];
    let mut filled: Vec<Rect> = Vec::new();
    let mut last = None;
    for layer in plan.layers() {
        match layer.kind {
            fusuma::LayerKind::Ui => {}
            fusuma::LayerKind::Surface(id) => {
                last = None;
                let seen = layer.visible;
                if !fresh.iter().any(|f| !f.intersect(&seen).is_empty()) {
                    continue;
                }
                if !single {
                    r.will_draw(seen);
                }
                let _ = r.save();
                let put_it = r.clip(seen).is_ok() && put(r, id, layer.rect);
                let _ = r.restore();
                if put_it {
                    total = total.union(&seen);
                    fresh.push(seen);
                    filled.push(seen);
                    last = Some(id);
                }
            }
            fusuma::LayerKind::Over(id) => {
                if last == Some(id)
                    && let Err(e) = shell.paint_over(r, id, layer.visible)
                {
                    log::error!("what is over a surface could not be drawn: {e:?}");
                }
            }
        }
    }
    // The host's pixels covered the dialogs and toasts: they are drawn again, once over every place (a surface inside
    // another is inside a place already done).
    for (n, place) in filled.iter().enumerate() {
        if filled[..n].iter().any(|p| p.intersect(place) == *place) {
            continue;
        }
        if let Err(e) = shell.paint_overlays(r, *place) {
            log::error!("what is above a surface could not be drawn: {e:?}");
        }
    }
    total
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
            // Reading back only what changed must not leave stale pixels: the pictures agree but for anti-aliasing.
            let apart = gpu
                .pixels()
                .iter()
                .zip(cpu.pixels())
                .filter(|(g, c)| {
                    [16, 8, 0]
                        .iter()
                        .any(|s| ((**g >> s) & 255).abs_diff((**c >> s) & 255) > 40)
                })
                .count();
            assert!(
                apart * 100 < cpu.pixels().len(),
                "{apart} pixels apart after row {k}"
            );
        }
    }

    /// Closing a dialog: after every frame of the fade the window's picture is what the CPU draws, and at the end nothing of
    /// the dialog is left.
    fn closing_a_dialog_leaves_nothing(backend: Backend) {
        let Some((mut c, mut s)) = showcase_canvas(backend) else {
            return;
        };
        let (mut cpu, mut cs) = showcase_canvas(Backend::VelloCpu).unwrap();
        for (canvas, shell) in [(&mut c, &mut s), (&mut cpu, &mut cs)] {
            canvas.frame(shell, 0);
        }
        let mut now = 0;
        let mut step = |c: &mut Canvas,
                        s: &mut Shell<Showcase>,
                        cpu: &mut Canvas,
                        cs: &mut Shell<Showcase>,
                        n: u32| {
            for _ in 0..n {
                now += 16;
                c.frame(s, now);
                cpu.frame(cs, now);
            }
        };
        s.app.ask_delete();
        cs.app.ask_delete();
        step(&mut c, &mut s, &mut cpu, &mut cs, 30);
        let cancel = s.app.dialog_nodes()[1].unwrap();
        let r = s.app.rect_of(cancel).unwrap();
        let at = Point::new(r.x + 4, r.y + 4);
        for sh in [&mut s, &mut cs] {
            sh.input(fusuma::Input::Pointer(Pointer::Down(at)));
            sh.input(fusuma::Input::Pointer(Pointer::Up(at)));
        }
        for n in 0..30 {
            step(&mut c, &mut s, &mut cpu, &mut cs, 1);
            let apart = c
                .pixels()
                .iter()
                .zip(cpu.pixels())
                .filter(|(g, p)| {
                    [16, 8, 0]
                        .iter()
                        .any(|k| ((**g >> k) & 255).abs_diff((**p >> k) & 255) > 40)
                })
                .count();
            assert!(
                apart * 200 < cpu.pixels().len(),
                "{apart} pixels apart {n} frames into the close"
            );
        }
    }

    #[test]
    fn closing_a_dialog_leaves_nothing_on_the_cpu() {
        closing_a_dialog_leaves_nothing(Backend::VelloCpu);
    }

    #[test]
    fn closing_a_dialog_leaves_nothing_on_the_gpu() {
        closing_a_dialog_leaves_nothing(Backend::VelloGpu);
    }

    /// Scrolling on the GPU moves the kept picture and draws the strip that came in: the window shows what the CPU shows.
    #[test]
    fn scrolling_on_the_gpu_matches_the_cpu() {
        let Some((mut gpu, mut gs)) = showcase_canvas(Backend::VelloGpu) else {
            eprintln!("no GPU here, skipped");
            return;
        };
        let (mut cpu, mut cs) = showcase_canvas(Backend::VelloCpu).unwrap();
        gpu.frame(&mut gs, 0).unwrap();
        cpu.frame(&mut cs, 0).unwrap();
        let mut now = 0;
        for k in [0usize, 3, 5, 8] {
            let page = fusuma_showcase::Page::ALL[k];
            for (canvas, shell) in [(&mut gpu, &mut gs), (&mut cpu, &mut cs)] {
                let row = shell.app.sidebar_item(page).unwrap();
                let r = shell.app.rect_of(row).unwrap();
                let at = Point::new(r.x + 4, r.y + 4);
                shell.input(fusuma::Input::Pointer(Pointer::Down(at)));
                shell.input(fusuma::Input::Pointer(Pointer::Up(at)));
                for step in 1..=20 {
                    canvas.frame(shell, now + step * 50);
                }
            }
            now += 1000;
            let centre = Point::new(600, 300);
            for turn in 0..12 {
                let dy = if turn < 8 { 37 } else { -53 };
                for (canvas, shell) in [(&mut gpu, &mut gs), (&mut cpu, &mut cs)] {
                    shell.input(fusuma::Input::Pointer(Pointer::Wheel(centre, dy)));
                    for step in 1..=3 {
                        canvas.frame(shell, now + step * 16);
                    }
                }
                now += 100;
                let apart = gpu
                    .pixels()
                    .iter()
                    .zip(cpu.pixels())
                    .filter(|(g, c)| {
                        [16, 8, 0]
                            .iter()
                            .any(|s| ((**g >> s) & 255).abs_diff((**c >> s) & 255) > 40)
                    })
                    .count();
                assert!(
                    apart * 100 < cpu.pixels().len(),
                    "{apart} pixels apart on page {k}, turn {turn}"
                );
            }
        }
    }

    /// The GPU draws the whole screen for every frame, but hands on only what changed.

    #[test]
    fn a_gpu_frame_reports_only_what_changed() {
        let Some((mut gpu, mut gs)) = showcase_canvas(Backend::VelloGpu) else {
            eprintln!("no GPU here, skipped");
            return;
        };
        let first = gpu.frame(&mut gs, 0).unwrap();
        assert_eq!(
            first,
            Rect::new(0, 0, 1024, 600),
            "the first picture is whole"
        );
        let b = gs.app.sidebar_item(fusuma_showcase::Page::ALL[2]).unwrap();
        let r = gs.app.rect_of(b).unwrap();
        let at = Point::new(r.x + 4, r.y + 4);
        gs.input(fusuma::Input::Pointer(Pointer::Move(at)));
        let region = gpu.frame(&mut gs, 16).unwrap();
        assert!(
            u64::from(region.w) * u64::from(region.h) < 1024 * 600 / 2,
            "a hover on one row is not the whole screen: {region:?}"
        );
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
