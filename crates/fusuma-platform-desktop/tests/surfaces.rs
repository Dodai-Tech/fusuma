#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The desktop host fills the surfaces of an app's layer plan and draws what the app put inside them over the host's picture.

use fusuma::fonts::FontSet;
use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::{App, Pointer, Shell};
use fusuma_platform_desktop::Canvas;

/// A tiny app: a surface with a button over it at the bottom.
struct Player {
    tree: fusuma::ui::Tree<64>,
    host: fusuma::Host<(), 16>,
    theme: fusuma::ui::Theme,
    bounds: Rect,
}

impl Player {
    fn new() -> Self {
        let mut tree = fusuma::ui::Tree::new();
        let mut host = fusuma::Host::new();
        tree.column(None, fusuma::ui::Style::new()).unwrap();
        let root = tree.root_id();
        let view = fusuma::column((
            fusuma::host_surface(5)
                .size(fusuma::Length::Fixed(200), fusuma::Length::Fixed(120))
                .over((fusuma::spacer(), fusuma::button("Play"))),
            fusuma::button("Other"),
        ));
        host.sync(&mut tree, root, &view).unwrap();
        Self {
            tree,
            host,
            theme: fusuma::ui::Theme::DARK,
            bounds: Rect::new(0, 0, 320, 240),
        }
    }
}

impl App for Player {
    fn pointer(&mut self, _: Pointer) {}
    fn key(&mut self, _: fusuma::Key) {}
    fn tick(&mut self, _: u32) -> bool {
        false
    }
    fn frame<R: fusuma::render::Renderer>(&mut self, r: &mut R) -> Result<Option<Rect>, R::Error> {
        let _ = &self.host;
        self.tree.update(r, &self.theme, self.bounds);
        self.tree.paint_frame(r, &self.theme, self.bounds)
    }
    fn frame_rects(&self) -> &[Rect] {
        self.tree.frame_rects()
    }
    fn host_surfaces(&mut self, on: bool) {
        self.tree.set_host_surfaces(on);
    }
    fn paint_above<R: fusuma::render::Renderer>(
        &mut self,
        r: &mut R,
        region: Rect,
    ) -> Result<(), R::Error> {
        self.tree.paint_above(r, &self.theme, region)
    }
    fn layers(&self) -> Option<fusuma::LayerPlan> {
        Some(self.tree.layers())
    }
}

fn rgb(c: u32) -> (u32, u32, u32) {
    ((c >> 16) & 255, (c >> 8) & 255, c & 255)
}

/// The host's picture is in the window, with the app's button drawn over it, and a change of the surface follows.
fn a_surface_shows_the_hosts_picture_with_controls_over_it(backend: Backend) {
    let Some(mut canvas) = Canvas::new(backend, 320, 240, FontSet::default()) else {
        eprintln!("{} is not available here, skipped", backend.name());
        return;
    };
    let mut shell = Shell::new(Player::new());
    let colour = std::rc::Rc::new(std::cell::Cell::new(0x00C81EC8_u32));
    let calls = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let (c, n) = (colour.clone(), calls.clone());
    let mut fill: fusuma_platform_desktop::FillSurface = Box::new(move |id, rect, px| {
        assert_eq!(id, 5);
        assert_eq!((rect.w, rect.h), (200, 120));
        n.set(n.get() + 1);
        px.fill(c.get());
    });
    canvas.frame_with(&mut shell, 0, Some(&mut fill)).unwrap();
    assert_eq!(calls.get(), 1);
    let surface = shell.app.tree.layers().surfaces().next().unwrap().rect;
    let at = |canvas: &Canvas, x: i32, y: i32| canvas.pixels()[y as usize * 320 + x as usize];
    // Where only the host draws: its colour, not the toolkit's background.
    let top = at(&canvas, surface.x + 100, surface.y + 5);
    assert!(
        rgb(top).0.abs_diff(0xC8) < 6 && rgb(top).1.abs_diff(0x1E) < 6,
        "{top:06x}"
    );
    // The button inside the surface is drawn over it: not the host's colour.
    let btn = at(&canvas, surface.x + 8, surface.bottom() - 14);
    assert_ne!(btn, top, "the button is over the picture");
    // Outside the surface it is the interface.
    let outside = at(&canvas, 300, 5);
    assert_ne!(outside, top);
    // The host's picture changes by itself: tell the app, and the next frame shows it, the button still over it.
    colour.set(0x0020C820);
    shell.app.tree.surface_changed(5);
    canvas.frame_with(&mut shell, 16, Some(&mut fill)).unwrap();
    assert_eq!(calls.get(), 2);
    let top = at(&canvas, surface.x + 100, surface.y + 5);
    assert!(
        rgb(top).1.abs_diff(0xC8) < 6 && rgb(top).0 < 0x40,
        "{top:06x}"
    );
    assert_ne!(at(&canvas, surface.x + 8, surface.bottom() - 14), top);
    // A frame that touches nothing near the surface leaves it alone and does not ask the host.
    canvas.frame_with(&mut shell, 32, Some(&mut fill));
    assert_eq!(calls.get(), 2);
}

#[test]
fn a_surface_shows_the_hosts_picture_on_the_cpu() {
    a_surface_shows_the_hosts_picture_with_controls_over_it(Backend::VelloCpu);
}

#[test]
fn a_surface_shows_the_hosts_picture_on_the_gpu() {
    a_surface_shows_the_hosts_picture_with_controls_over_it(Backend::VelloGpu);
}
