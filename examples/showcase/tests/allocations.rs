#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

//! How many times the showcase asks the allocator for memory while it runs, once everything is built.
//!
//! The toolkit is `no_std` with no allocator, so a frame, a click or a scroll should need none: a chip with no heap to
//! spare (an ESP32-C3) must be able to run it. This counts the allocations of a scripted session on both layouts and
//! fails if there is one, so a change that adds a `String` or a `Vec` to the hot path shows up at once.

use allocation_counter::measure;
use fusuma::render::Point;
use fusuma::renderers::embedded::{Embedded, Frame};
use fusuma::{App, Pointer};
use fusuma_showcase::{Page, Showcase};

type Screen = Embedded<Frame<Vec<u16>>>;

fn screen(w: u16, h: u16) -> Screen {
    Embedded::new(Frame::new(w, h, vec![0u16; usize::from(w) * usize::from(h)]).unwrap())
}

fn press(app: &mut Showcase, at: Point) {
    for e in [Pointer::Down(at), Pointer::Up(at), Pointer::Leave] {
        app.pointer(e);
    }
}

/// A scripted session on page `page` of an app of `w` x `h`: frames, hovers, a click, a wheel scroll and time passing.
fn session(w: u16, h: u16, page: Page) -> (usize, &'static str) {
    let mut app = Showcase::new(u32::from(w), u32::from(h));
    app.set_start(Some(page.name()), None, None, true, false);
    let mut r = screen(w, h);
    // Warm up: the first frames build fonts caches and lay everything out.
    for _ in 0..3 {
        App::tick(&mut app, 16);
        app.frame(&mut r).unwrap();
    }
    let middle = Point::new(i32::from(w) / 2, i32::from(h) / 2);
    let n = measure(|| {
        for k in 0..6 {
            app.pointer(Pointer::Move(Point::new(middle.x + k * 7, middle.y)));
            App::tick(&mut app, 16);
            app.frame(&mut r).unwrap();
        }
        press(&mut app, middle);
        App::tick(&mut app, 16);
        app.frame(&mut r).unwrap();
        for _ in 0..4 {
            app.pointer(Pointer::Wheel(middle, 18));
            App::tick(&mut app, 16);
            app.frame(&mut r).unwrap();
        }
        // Time passes with nothing to do.
        for _ in 0..3 {
            App::tick(&mut app, 100);
            let _ = app.frame(&mut r).unwrap();
        }
    });
    (n.count_total as usize, page.name())
}

#[test]
fn a_running_showcase_asks_the_allocator_for_nothing() {
    let mut report = Vec::new();
    for (w, h) in [(410u16, 502u16), (1024, 600)] {
        for page in Page::ALL {
            let (n, name) = session(w, h, page);
            report.push(format!("{w}x{h} {name}: {n}"));
        }
    }
    let worst: usize = report
        .iter()
        .filter_map(|l| l.rsplit(": ").next()?.parse().ok())
        .max()
        .unwrap();
    println!("{}", report.join("\n"));
    assert_eq!(worst, 0, "allocations per session:\n{}", report.join("\n"));
}
