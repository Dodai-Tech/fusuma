#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The layer plan: a surface is a node the host fills. It takes room, draws nothing, is listed in tree order, is left
//! out while hidden, and one that changed is part of the next frame.

use fusuma::{Host, LayerKind, Length, View, button, column, host_surface, row, spacer};
use fusuma_render::{Color, Probe, Rect, Renderer};
use fusuma_render_integer::{Embedded, Frame};
use fusuma_ui::{Style, Theme, Tree};

const W: u16 = 300;
const H: u16 = 200;
const BOUNDS: Rect = Rect::new(0, 0, W as u32, H as u32);
const THEME: Theme = Theme::DARK;

type Screen = Embedded<Frame<Vec<u16>>>;

fn screen() -> Screen {
    Embedded::new(Frame::new(W, H, vec![0u16; usize::from(W) * usize::from(H)]).unwrap())
}

fn view(wide: bool) -> impl View<()> {
    column((
        button("top"),
        row((
            host_surface(7).size(Length::Fixed(100), Length::Fixed(60)),
            host_surface(9).size(
                Length::Fixed(if wide { 150 } else { 50 }),
                Length::Fixed(40),
            ),
        )),
    ))
}

fn build(wide: bool) -> (Tree<64>, Host<(), 16>, Screen) {
    let mut tree = Tree::new();
    let mut host = Host::new();
    tree.column(None, Style::new()).unwrap();
    let root = tree.root_id();
    host.sync(&mut tree, root, &view(wide)).unwrap();
    let mut r = screen();
    tree.update(&r, &THEME, BOUNDS);
    tree.paint_frame(&mut r, &THEME, BOUNDS).unwrap();
    (tree, host, r)
}

#[test]
fn the_plan_has_the_interface_then_the_surfaces_in_tree_order() {
    let (tree, _, _) = build(false);
    let plan = tree.layers();
    let kinds: Vec<_> = plan.layers().iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        [LayerKind::Ui, LayerKind::Surface(7), LayerKind::Surface(9)]
    );
    let s = plan.surfaces().next().unwrap().rect;
    assert_eq!((s.w, s.h), (100, 60), "it took the room it asked for");
    let t = plan.surfaces().nth(1).unwrap().rect;
    assert!(t.x >= s.right(), "the second is to the right of the first");
    assert_eq!(plan.left_out(), 0);
}

#[test]
fn a_surface_draws_nothing_of_its_own() {
    let (mut tree, _, mut r) = build(false);
    // Nothing is left to draw after the first frame, and the surface made no damage of its own.
    assert!(tree.paint_frame(&mut r, &THEME, BOUNDS).unwrap().is_none());
}

#[test]
fn only_the_surfaces_a_frame_touched_are_stale() {
    let (tree, _, _) = build(false);
    let plan = tree.layers();
    let first = plan.surfaces().next().unwrap().rect;
    let stale: Vec<_> = plan.stale(first).map(|l| l.kind).collect();
    assert_eq!(stale, [LayerKind::Surface(7)]);
    assert_eq!(plan.stale(Rect::new(0, 190, 5, 5)).count(), 0);
}

#[test]
fn a_surface_that_changed_is_part_of_the_next_frame() {
    let (mut tree, _, mut r) = build(false);
    let want = tree.layers().surfaces().nth(1).unwrap().rect;
    tree.surface_changed(9);
    let drawn = tree.paint_frame(&mut r, &THEME, BOUNDS).unwrap().unwrap();
    assert!(drawn.intersect(&want).w > 0, "{drawn:?} covers {want:?}");
    let plan = tree.layers();
    assert!(plan.stale(drawn).any(|l| l.kind == LayerKind::Surface(9)));
    // A name that is not there does nothing.
    tree.surface_changed(1234);
    assert!(tree.paint_frame(&mut r, &THEME, BOUNDS).unwrap().is_none());
}

#[test]
fn a_hidden_surface_is_not_listed_and_a_rebuilt_one_follows_its_view() {
    let (mut tree, mut host, r) = build(false);
    let id = tree.layers().surfaces().nth(1).unwrap().rect;
    assert_eq!(id.w, 50);
    let root = tree.root_id();
    host.sync(&mut tree, root, &view(true)).unwrap();
    tree.update(&r, &THEME, BOUNDS);
    assert_eq!(tree.layers().surfaces().nth(1).unwrap().rect.w, 150);
    let col = tree.first_child(root.unwrap()).unwrap();
    let first = tree.first_child(col).unwrap();
    let row = tree.next_sibling(first).unwrap();
    tree.set_hidden(row, true);
    tree.update(&r, &THEME, BOUNDS);
    assert_eq!(tree.layers().surfaces().count(), 0);
}

#[test]
fn a_surface_that_does_not_fit_in_the_plan_is_counted() {
    let mut plan = fusuma::LayerPlan::ui(BOUNDS);
    for i in 0..(fusuma::MAX_SURFACES + 3) {
        plan.push_surface(
            i as u32,
            Rect::new(0, 0, 1, 1),
            Rect::new(0, 0, 1, 1),
            false,
            false,
        );
    }
    assert_eq!(plan.surfaces().count(), fusuma::MAX_SURFACES);
    assert_eq!(plan.left_out(), 3);
}

/// A player: a surface with a button inside it, at the bottom.
fn player() -> impl View<()> {
    host_surface(3)
        .size(Length::Fixed(200), Length::Fixed(120))
        .over((spacer(), button("Play")))
}

fn player_tree(host_fills: bool) -> (Tree<64>, Host<(), 16>, Screen) {
    let mut tree = Tree::new();
    let mut host = Host::<(), 16>::new();
    tree.column(None, Style::new()).unwrap();
    let root = tree.root_id();
    host.sync(&mut tree, root, &player()).unwrap();
    tree.set_host_surfaces(host_fills);
    let mut r = screen();
    tree.update(&r, &THEME, BOUNDS);
    tree.paint_frame(&mut r, &THEME, BOUNDS).unwrap();
    (tree, host, r)
}

fn button_rect(tree: &Tree<64>) -> Rect {
    let plan = tree.layers();
    let over = plan
        .overs()
        .next()
        .expect("the surface has something over it")
        .rect;
    // The button is the last child: at the bottom of the surface.
    Rect::new(over.x + 4, over.bottom() - 12, 8, 8)
}

#[test]
fn what_is_inside_a_surface_is_a_layer_over_it() {
    let (tree, _, _) = player_tree(true);
    let kinds: Vec<_> = tree.layers().layers().iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        [LayerKind::Ui, LayerKind::Surface(3), LayerKind::Over(3)]
    );
    // A surface with nothing inside has no layer over it.
    let (plain, _) = build_tree_only();
    assert_eq!(plain.layers().overs().count(), 0);
}

fn build_tree_only() -> (Tree<64>, Host<(), 16>) {
    let (t, h, _) = build(false);
    (t, h)
}

#[test]
fn without_a_host_what_is_inside_a_surface_is_drawn_with_the_rest() {
    let (tree, _, r) = player_tree(false);
    let at = button_rect(&tree);
    let bg = r.pixel(W as i32 - 2, H as i32 - 2).unwrap();
    assert_ne!(r.pixel(at.x + 2, at.y + 2).unwrap(), bg, "the button shows");
}

#[test]
fn with_a_host_it_is_drawn_after_the_host_fills_the_surface() {
    let (tree, _, mut r) = player_tree(true);
    let surface = tree.layers().surfaces().next().unwrap().rect;
    let at = button_rect(&tree);
    let bg = r.pixel(W as i32 - 2, H as i32 - 2).unwrap();
    // The interface left the surface and what is inside it alone.
    assert_eq!(
        r.pixel(at.x + 2, at.y + 2).unwrap(),
        bg,
        "not drawn under the host"
    );
    // The host puts its picture there ...
    let video = Color::rgb(200, 30, 200);
    r.fill_rect(surface, 0, video).unwrap();
    assert_ne!(r.pixel(surface.x + 1, surface.y + 1).unwrap(), bg);
    // ... and then the button is drawn over it.
    tree.paint_over(&mut r, &THEME, 3, surface).unwrap();
    let on = r.pixel(at.x + 2, at.y + 2).unwrap();
    assert_ne!(on, video, "the button is over the picture");
    assert_ne!(on, bg);
    // Nothing else of the picture was touched.
    assert_ne!(r.pixel(surface.x + 1, surface.y + 1).unwrap(), bg);
    assert_eq!(r.pixel(surface.right() + 5, surface.y + 1).unwrap(), bg);
}

#[test]
fn a_change_inside_a_surface_makes_it_stale() {
    let (mut tree, mut host, mut r) = player_tree(true);
    let surface = tree.layers().surfaces().next().unwrap().rect;
    let root = tree.root_id();
    // Rebuild with another label: the button changes, so the frame covers it and the surface is stale.
    host.sync(
        &mut tree,
        root,
        &host_surface(3)
            .size(Length::Fixed(200), Length::Fixed(120))
            .over((spacer(), button("Pause"))),
    )
    .unwrap();
    tree.update(&r, &THEME, BOUNDS);
    let drawn = tree
        .paint_frame(&mut r, &THEME, BOUNDS)
        .unwrap()
        .expect("the label changed");
    assert!(tree.layers().stale(drawn).any(|l| l.rect == surface));
}

#[test]
fn a_surface_can_hold_another_in_the_view() {
    let view = host_surface(1)
        .size(Length::Fixed(200), Length::Fixed(120))
        .over((
            host_surface(2).size(Length::Fixed(60), Length::Fixed(40)),
            button("Close"),
        ));
    let mut tree = Tree::<64>::new();
    let mut host = Host::<(), 16>::new();
    tree.column(None, Style::new()).unwrap();
    let root = tree.root_id();
    host.sync(&mut tree, root, &view).unwrap();
    tree.set_host_surfaces(true);
    let mut r = screen();
    tree.update(&r, &THEME, BOUNDS);
    tree.paint_frame(&mut r, &THEME, BOUNDS).unwrap();
    let kinds: Vec<_> = tree.layers().layers().iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        [
            LayerKind::Ui,
            LayerKind::Surface(1),
            LayerKind::Over(1),
            LayerKind::Surface(2)
        ]
    );
}

#[test]
fn a_surface_in_a_list_keeps_its_size_and_is_seen_only_inside_it() {
    let mut tree = Tree::<64>::new();
    tree.column(None, Style::new()).unwrap();
    let root = tree.root_id();
    let list = tree
        .scroll(root, Style::new().height(Length::Fixed(50)))
        .unwrap();
    tree.surface(
        Some(list),
        4,
        Style::new()
            .width(Length::Fixed(80))
            .height(Length::Fixed(200)),
    )
    .unwrap();
    let r = screen();
    tree.update(&r, &THEME, BOUNDS);
    let plan = tree.layers();
    let s = plan.surfaces().next().unwrap();
    assert_eq!((s.rect.w, s.rect.h), (80, 200));
    assert_eq!(s.visible.h, 50);
    assert!(plan.stale(Rect::new(0, 60, 80, 20)).next().is_none());
}
