//! Compares the CPU and the GPU Vello backends on a typical interface frame.
//!
//! `cargo run --release -p fusuma-render-vello --example bench`
//!
//! A frame is a grid of rounded cards with a circle and bars each, drawn after a clear. Time is
//! measured with pixels read back, because reading the picture is what a screenshot, a test or a
//! CPU-composited toolkit does; the GPU number therefore includes the copy back to the CPU.

use std::time::Instant;

use fusuma_render::{Color, Point, Probe, Rect, Renderer};

/// A drawing call that cannot fail on a surface that exists; a failure ends the run.
fn ok<E: core::fmt::Debug>(r: Result<(), E>) {
    if let Err(e) = r {
        eprintln!("drawing failed: {e:?}");
        std::process::exit(1);
    }
}

fn frame<R: Renderer>(r: &mut R, w: i32, h: i32, shift: i32) {
    ok(r.clear(Color::rgb(8, 8, 16)));
    let (cols, rows) = (w / 140 + 1, h / 140 + 1);
    for row in 0..rows {
        for col in 0..cols {
            let (x, y) = (col * 140 + 8 + shift % 7, row * 140 + 8);
            ok(r.fill_rect(Rect::new(x, y, 124, 124), 14, Color::rgb(33, 38, 41)));
            ok(r.fill_circle(Point::new(x + 62, y + 50), 22, Color::rgb(24, 162, 165)));
            ok(r.stroke_rect(
                Rect::new(x + 10, y + 90, 104, 24),
                8,
                2,
                Color::rgb(250, 200, 0),
            ));
        }
    }
}

fn measure<R: Renderer + Probe>(
    name: &str,
    make: impl Fn(u32, u32) -> Option<R>,
    sizes: &[(u32, u32)],
) {
    for &(w, h) in sizes {
        let Some(mut r) = make(w, h) else {
            println!("{name:<12} {w}x{h}: not available");
            continue;
        };
        for i in 0..3 {
            frame(&mut r, w as i32, h as i32, i);
            let _ = r.pixel(0, 0);
        }
        let n = 30;
        let start = Instant::now();
        for i in 0..n {
            frame(&mut r, w as i32, h as i32, i);
            let _ = r.pixel(0, 0);
        }
        let ms = start.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
        println!(
            "{name:<12} {w}x{h}: {ms:7.2} ms per frame ({:.0} fps)",
            1000.0 / ms
        );
    }
}

fn main() {
    let sizes = [(410, 502), (1920, 1080)];
    measure("vello-cpu", fusuma_render_vello_cpu::new, &sizes);
    measure(
        "vello-gpu",
        |w, h| fusuma_render_vello::new(w, h).ok(),
        &sizes,
    );
}
