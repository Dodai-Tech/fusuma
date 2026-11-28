#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The desktop host fills the surfaces of an app's layer plan and draws what the app put inside them over the host's picture.

use fusuma::Length;
use fusuma::fonts::FontSet;
use fusuma::platform::Backend;
use fusuma::render::Rect;
use fusuma::ui::{ButtonVariant, Style, Tree};
use fusuma::{App, Pointer, Shell};
use fusuma_platform_desktop::{Canvas, Surfaces};

/// A tiny app whose tree a test builds by hand.
struct Rig {
    tree: Tree<64>,
    theme: fusuma::ui::Theme,
    bounds: Rect,
}

impl Rig {
    fn new(build: impl FnOnce(&mut Tree<64>)) -> Self {
        let mut tree = Tree::new();
        tree.column(None, Style::new()).unwrap();
        build(&mut tree);
        Self {
            tree,
            theme: fusuma::ui::Theme::DARK,
            bounds: Rect::new(0, 0, 320, 240),
        }
    }

    /// A surface (`id`, `w` by `h`) with a spacer and a button inside it, so the button is at the bottom.
    fn player() -> Self {
        Self::new(|t| {
            let root = t.root_id();
            let size = Style::new()
                .width(Length::Fixed(200))
                .height(Length::Fixed(120));
            let s = t.surface(root, 5, size).unwrap();
            t.spacer(Some(s), Style::new().height(Length::Fill(1)))
                .unwrap();
            t.button(Some(s), "Play", ButtonVariant::Primary).unwrap();
            t.button(root, "Other", ButtonVariant::Primary).unwrap();
        })
    }
}

impl App for Rig {
    fn pointer(&mut self, _: Pointer) {}
    fn key(&mut self, _: fusuma::Key) {}
    fn tick(&mut self, dt: u32) -> bool {
        self.tree.advance(&self.theme, dt)
    }
    fn frame<R: fusuma::render::Renderer>(&mut self, r: &mut R) -> Result<Option<Rect>, R::Error> {
        self.tree.update(r, &self.theme, self.bounds);
        self.tree.paint_frame(r, &self.theme, self.bounds)
    }
    fn frame_rects(&self) -> &[Rect] {
        self.tree.frame_rects()
    }
    fn redraw_all(&mut self) {
        self.tree.invalidate();
    }

    fn host_surfaces(&mut self, on: bool) {
        self.tree.set_host_surfaces(on);
    }
    fn paint_over<R: fusuma::render::Renderer>(
        &mut self,
        r: &mut R,
        id: u32,
        region: Rect,
    ) -> Result<(), R::Error> {
        self.tree.paint_over(r, &self.theme, id, region)
    }
    fn paint_overlays<R: fusuma::render::Renderer>(
        &mut self,
        r: &mut R,
        region: Rect,
    ) -> Result<(), R::Error> {
        self.tree.paint_overlays(r, &self.theme, region)
    }
    fn paint_topmost<R: fusuma::render::Renderer>(
        &mut self,
        r: &mut R,
        region: Rect,
    ) -> Result<(), R::Error> {
        self.tree.paint_topmost(r, &self.theme, region)
    }
    fn layers(&self) -> Option<fusuma::LayerPlan> {
        Some(self.tree.layers())
    }
}

fn rgb(c: u32) -> (u32, u32, u32) {
    ((c >> 16) & 255, (c >> 8) & 255, c & 255)
}

type Cell<T> = std::rc::Rc<std::cell::Cell<T>>;

fn cell<T>(v: T) -> Cell<T> {
    std::rc::Rc::new(std::cell::Cell::new(v))
}

fn canvas(backend: Backend) -> Option<Canvas> {
    let c = Canvas::new(backend, 320, 240, FontSet::default());
    if c.is_none() {
        eprintln!("{} is not available here, skipped", backend.name());
    }
    c
}

fn at(canvas: &Canvas, x: i32, y: i32) -> u32 {
    canvas.pixels()[y as usize * 320 + x as usize]
}

fn near(c: u32, want: u32) -> bool {
    let (a, b) = (rgb(c), rgb(want));
    a.0.abs_diff(b.0) < 8 && a.1.abs_diff(b.1) < 8 && a.2.abs_diff(b.2) < 8
}

/// A host that paints every surface one colour (by id) and counts how often it is asked.
fn colours(colour: &Cell<u32>, calls: &Cell<u32>) -> Surfaces {
    let (c, n) = (colour.clone(), calls.clone());
    Surfaces {
        pixels: Some(Box::new(move |_, _, px| {
            n.set(n.get() + 1);
            px.fill(c.get());
        })),
        textures: None,
    }
}

/// The host's picture is in the window, with the app's button drawn over it, and a change of the surface follows.
fn a_surface_shows_the_hosts_picture_with_controls_over_it(backend: Backend) {
    let Some(mut canvas) = canvas(backend) else {
        return;
    };
    let mut shell = Shell::new(Rig::player());
    let (colour, calls) = (cell(0x00C8_1EC8_u32), cell(0));
    let mut hosts = colours(&colour, &calls);
    canvas.frame_with(&mut shell, 0, &mut hosts).unwrap();
    assert_eq!(calls.get(), 1);
    let surface = shell.app.tree.layers().surfaces().next().unwrap().rect;
    // Where only the host draws: its colour, not the toolkit's background.
    let top = at(&canvas, surface.x + 100, surface.y + 5);
    assert!(near(top, 0xC81EC8), "{top:06x}");
    // The button inside the surface is drawn over it: not the host's colour.
    let btn = at(&canvas, surface.x + 8, surface.bottom() - 14);
    assert_ne!(btn, top, "the button is over the picture");
    assert_ne!(at(&canvas, 300, 5), top);
    // The host's picture changes by itself: tell the app, and the next frame shows it, the button still over it.
    colour.set(0x0020_C820);
    shell.app.tree.surface_changed(5);
    canvas.frame_with(&mut shell, 16, &mut hosts).unwrap();
    assert_eq!(calls.get(), 2);
    let top = at(&canvas, surface.x + 100, surface.y + 5);
    assert!(near(top, 0x20C820), "{top:06x}");
    assert_ne!(at(&canvas, surface.x + 8, surface.bottom() - 14), top);
    // A frame that touches nothing near the surface leaves it alone and does not ask the host.
    canvas.frame_with(&mut shell, 32, &mut hosts);
    assert_eq!(calls.get(), 2);
}

/// An open dialog over a surface is not covered by the host's picture, and is dimmed once, not once per frame.
fn a_dialog_stays_above_a_surface(backend: Backend) {
    let Some(mut canvas) = canvas(backend) else {
        return;
    };
    let mut shell = Shell::new(Rig::new(|t| {
        let root = t.root_id();
        t.surface(
            root,
            5,
            Style::new().width(Length::Fill(1)).height(Length::Fill(1)),
        )
        .unwrap();
        let d = t.dialog(root, Style::new().padding(12)).unwrap();
        t.button(Some(d), "Delete", ButtonVariant::Primary).unwrap();
        t.open_dialog(d);
    }));
    let (colour, calls) = (cell(0x00C8_1EC8_u32), cell(0));
    let mut hosts = colours(&colour, &calls);
    for step in 0..30 {
        shell.app.tick(30);
        canvas.frame_with(&mut shell, step * 30, &mut hosts);
    }
    // Away from the dialog: the host's picture with the scrim over it.
    let dim = at(&canvas, 4, 4);
    assert!(
        !near(dim, 0xC81EC8),
        "the scrim is over the picture: {dim:06x}"
    );
    // The dialog itself is over the picture: its middle is not the host's colour either.
    let middle = at(&canvas, 160, 120);
    assert!(!near(middle, 0xC81EC8), "{middle:06x}");
    // Another frame, the surface asked for again: the dimming does not pile up.
    shell.app.tree.surface_changed(5);
    canvas.frame_with(&mut shell, 2000, &mut hosts);
    assert_eq!(at(&canvas, 4, 4), dim, "dimmed once");
    assert_eq!(at(&canvas, 160, 120), middle);
}

/// A surface inside another one's children is over it, and what is over the outer one is under the inner one.
fn a_surface_nests_in_another(backend: Backend) {
    let Some(mut canvas) = canvas(backend) else {
        return;
    };
    let mut shell = Shell::new(Rig::new(|t| {
        let root = t.root_id();
        let outer = t
            .surface(
                root,
                5,
                Style::new()
                    .width(Length::Fixed(200))
                    .height(Length::Fixed(120)),
            )
            .unwrap();
        t.surface(
            Some(outer),
            6,
            Style::new()
                .width(Length::Fixed(60))
                .height(Length::Fixed(40)),
        )
        .unwrap();
    }));
    let mut hosts = Surfaces {
        pixels: Some(Box::new(|id, _, px| {
            px.fill(if id == 5 { 0x00C8_0000 } else { 0x0000_C800 });
        })),
        textures: None,
    };
    canvas.frame_with(&mut shell, 0, &mut hosts).unwrap();
    let plan = shell.app.tree.layers();
    let ids: Vec<_> = plan.layers().iter().map(|l| l.kind).collect();
    assert_eq!(
        ids,
        [
            fusuma::LayerKind::Ui,
            fusuma::LayerKind::Surface(5),
            fusuma::LayerKind::Over(5),
            fusuma::LayerKind::Surface(6),
        ]
    );
    let outer = plan.surfaces().next().unwrap().rect;
    let inner = plan.surfaces().nth(1).unwrap().rect;
    assert!(near(
        at(&canvas, outer.right() - 4, outer.bottom() - 4),
        0xC80000
    ));
    let c = (inner.x + 20, inner.y + 10);
    assert!(
        near(at(&canvas, c.0, c.1), 0x00C800),
        "the inner one is on top"
    );
}

/// A surface in a list that scrolls is drawn only inside the list, but the host is given the whole of it.
fn a_surface_in_a_list_stays_in_the_list(backend: Backend) {
    let Some(mut canvas) = canvas(backend) else {
        return;
    };
    let mut seen = None;
    let mut shell = Shell::new(Rig::new(|t| {
        let root = t.root_id();
        let list = t
            .scroll(root, Style::new().height(Length::Fixed(80)))
            .unwrap();
        t.surface(
            Some(list),
            5,
            Style::new()
                .width(Length::Fixed(200))
                .height(Length::Fixed(300)),
        )
        .unwrap();
        t.button(root, "Below", ButtonVariant::Primary).unwrap();
    }));
    let size = std::rc::Rc::new(std::cell::Cell::new((0, 0)));
    let s = size.clone();
    let mut hosts = Surfaces {
        pixels: Some(Box::new(move |_, rect, px| {
            s.set((rect.w, rect.h));
            px.fill(0x00C8_1EC8);
        })),
        textures: None,
    };
    canvas.frame_with(&mut shell, 0, &mut hosts).unwrap();
    let layer = shell.app.tree.layers().surfaces().next().copied().unwrap();
    seen.replace(layer);
    assert_eq!(size.get(), (200, 300), "the host gets the whole surface");
    assert_eq!(layer.visible.h, 80, "only the list's height can be seen");
    // Just under the list: the button's row, not the host's picture.
    let below = at(&canvas, 4, layer.visible.bottom() + 2);
    assert!(!near(below, 0xC81EC8), "{below:06x}");
    assert!(near(at(&canvas, 100, layer.visible.y + 10), 0xC81EC8));
}

/// A GPU texture reaches the window with no pixels from the host.
#[test]
fn a_texture_is_shown_with_no_copy_through_the_cpu() {
    use fusuma::renderers::vello_gpu::wgpu;
    let Some(mut canvas) = canvas(Backend::VelloGpu) else {
        return;
    };
    let mut shell = Shell::new(Rig::player());
    let asked = cell(0);
    let a = asked.clone();
    let mut hosts = Surfaces {
        pixels: Some(Box::new(|_, _, _| {
            panic!("a texture was given: no pixels are asked for")
        })),
        textures: Some(Box::new(move |_, _, target| {
            a.set(a.get() + 1);
            let texture = target.texture(2, 2);
            let blue = [0u8, 0, 200, 255].repeat(4);
            target.queue().write_texture(
                texture.as_image_copy(),
                &blue,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(8),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 1,
                },
            );
            true
        })),
    };
    canvas.frame_with(&mut shell, 0, &mut hosts).unwrap();
    assert_eq!(asked.get(), 1);
    let surface = shell.app.tree.layers().surfaces().next().unwrap().rect;
    let top = at(&canvas, surface.x + 100, surface.y + 5);
    assert!(near(top, 0x0000C8), "{top:06x}");
    // The controls are over it, as with pixels.
    assert_ne!(at(&canvas, surface.x + 8, surface.bottom() - 14), top);
}

/// A surface inside an open dialog is above the dialog and the dimmed picture, and above a surface that lies under both.
fn a_surface_in_a_dialog_is_above_the_dialog(backend: Backend) {
    let Some(mut canvas) = canvas(backend) else {
        return;
    };
    let mut shell = Shell::new(Rig::new(|t| {
        let root = t.root_id();
        t.surface(
            root,
            1,
            Style::new().width(Length::Fill(1)).height(Length::Fill(1)),
        )
        .unwrap();
        let d = t.dialog(root, Style::new().padding(12)).unwrap();
        t.surface(
            Some(d),
            2,
            Style::new()
                .width(Length::Fixed(100))
                .height(Length::Fixed(60)),
        )
        .unwrap();
        t.button(Some(d), "Close", ButtonVariant::Primary).unwrap();
        t.open_dialog(d);
    }));
    let mut hosts = Surfaces {
        pixels: Some(Box::new(|id, _, px| {
            px.fill(if id == 1 { 0x00C8_0000 } else { 0x0000_C800 });
        })),
        textures: None,
    };
    for step in 0..30 {
        shell.app.tick(30);
        canvas.frame_with(&mut shell, step * 30, &mut hosts);
    }
    let plan = shell.app.tree.layers();
    let order: Vec<_> = plan.layers().iter().map(|l| (l.kind, l.above)).collect();
    assert_eq!(
        order,
        [
            (fusuma::LayerKind::Ui, false),
            (fusuma::LayerKind::Surface(1), false),
            (fusuma::LayerKind::Surface(2), true)
        ]
    );
    let inner = plan.surfaces().nth(1).unwrap().rect;
    let c = at(&canvas, inner.x + 50, inner.y + 30);
    assert!(
        near(c, 0x00C800),
        "the video in the dialog is not covered: {c:06x}"
    );
    // Under the dialog the first one is dimmed, not covered by the dialog's panel.
    let dim = at(&canvas, 4, 4);
    assert!(!near(dim, 0xC80000) && rgb(dim).0 > 0x40, "{dim:06x}");
    // It stays right when the first surface is drawn again.
    shell.app.tree.surface_changed(1);
    canvas.frame_with(&mut shell, 5000, &mut hosts);
    assert_eq!(at(&canvas, 4, 4), dim);
    assert!(near(at(&canvas, inner.x + 50, inner.y + 30), 0x00C800));
}

macro_rules! on_both {
    ($($name:ident),*) => {$(
        mod $name {
            use super::*;
            #[test]
            fn on_the_cpu() { super::$name(Backend::VelloCpu); }
            #[test]
            fn on_the_gpu() { super::$name(Backend::VelloGpu); }
        }
    )*};
}

on_both!(
    a_surface_shows_the_hosts_picture_with_controls_over_it,
    a_dialog_stays_above_a_surface,
    a_surface_nests_in_another,
    a_surface_in_a_list_stays_in_the_list,
    a_surface_in_a_dialog_is_above_the_dialog
);
