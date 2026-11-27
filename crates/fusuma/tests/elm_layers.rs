#![allow(clippy::unwrap_used, clippy::expect_used)]
//! The layer plan: a surface is a node the host fills. It takes room, draws nothing, is listed in tree order, is left
//! out while hidden, and one that changed is part of the next frame.

use fusuma::{Host, LayerKind, Length, View, button, column, host_surface, row};
use fusuma_render::Rect;
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
        plan.push_surface(i as u32, Rect::new(0, 0, 1, 1));
    }
    assert_eq!(plan.surfaces().count(), fusuma::MAX_SURFACES);
    assert_eq!(plan.left_out(), 3);
}
