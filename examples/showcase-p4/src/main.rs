//! The Fusuma components showcase on the Waveshare ESP32-P4 7" board.
//!
//! The app is `fusuma-showcase`, the same code the host tests draw. Here a loop polls the GT911
//! touch panel, ticks the app, and has it drawn on `vello_cpu`; only the part that changed is
//! converted to RGB565 and written into the DSI frame buffer.

mod hw;

use std::time::{Duration, Instant};

use fusuma::Pointer;
use fusuma_render::Point;
use fusuma_render_vello_cpu::Scene;
use fusuma_showcase::Showcase;

use hw::{HEIGHT, Touch, TouchTracker, WIDTH};

/// RGBA8 (premultiplied; every pixel here is opaque) to RGB565.
fn rgb565(px: &[u8]) -> u16 {
    ((u16::from(px[0]) & 0xF8) << 8) | ((u16::from(px[1]) & 0xFC) << 3) | (u16::from(px[2]) >> 3)
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("Fusuma showcase, {WIDTH}x{HEIGHT}");

    hw::init()?;
    hw::set_backlight(80);
    let fb = hw::framebuffer();
    anyhow::ensure!(!fb.is_null(), "no frame buffer");
    // SAFETY: the DSI driver owns WIDTH * HEIGHT RGB565 pixels at `fb` for the whole program, and
    // nothing else writes to it.
    let screen = unsafe { std::slice::from_raw_parts_mut(fb, WIDTH * HEIGHT) };

    let mut app = Showcase::new(WIDTH as u32, HEIGHT as u32);
    let mut surface = fusuma_render_vello_cpu::new(WIDTH as u32, HEIGHT as u32)
        .ok_or_else(|| anyhow::anyhow!("no surface"))?;
    let mut touch = TouchTracker::default();
    let mut last = Instant::now();
    let mut frames = 0u32;
    let mut report = Instant::now();

    loop {
        let at = |x: u16, y: u16| Point::new(i32::from(x), i32::from(y));
        match touch.poll() {
            Some(Touch::Down { x, y }) => app.pointer(Pointer::Down(at(x, y))),
            Some(Touch::Move { x, y }) => app.pointer(Pointer::Move(at(x, y))),
            Some(Touch::Up { x, y }) => {
                app.pointer(Pointer::Up(at(x, y)));
                app.pointer(Pointer::Leave);
            }
            None => {}
        }
        let now = Instant::now();
        let dt = now.duration_since(last).as_millis() as u32;
        last = now;
        app.tick(dt);

        // The scene forgets what it drew before; this frame redraws only the damaged part.
        surface.with_scene(Scene::reset);
        let started = Instant::now();
        match app.frame(&mut surface) {
            Ok(Some(region)) => {
                let drawn = started.elapsed();
                surface.with_scene(|scene| {
                    let rgba = scene.pixels();
                    let y_end = region.bottom().clamp(0, HEIGHT as i32) as usize;
                    let x_end = region.right().clamp(0, WIDTH as i32) as usize;
                    let (x0, y0) = (region.x.max(0) as usize, region.y.max(0) as usize);
                    for y in y0..y_end {
                        for x in x0..x_end {
                            let i = (y * WIDTH + x) * 4;
                            screen[y * WIDTH + x] = rgb565(&rgba[i..i + 4]);
                        }
                    }
                    hw::flush_rows(y0 as u32, y_end as u32);
                });
                frames += 1;
                log::info!(
                    "frame {}x{} at ({}, {}): drew {} ms, total {} ms",
                    region.w,
                    region.h,
                    region.x,
                    region.y,
                    drawn.as_millis(),
                    started.elapsed().as_millis()
                );
            }
            Ok(None) => {}
            Err(e) => log::error!("draw failed: {e:?}"),
        }
        if report.elapsed() > Duration::from_secs(10) {
            report = Instant::now();
            log::info!("alive, {frames} frames so far");
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}
