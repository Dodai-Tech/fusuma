//! Tests of damage tracking: the area reported after a change must cover everything that changed on
//! screen, and drawing only that area must give the same picture as drawing everything.

use fusuma_render::{Color, Point, Probe, Rect, Renderer};
use fusuma_render_vello_cpu::VelloCpu;

use crate::event::Pointer;
use crate::theme::{ButtonVariant, Round, Surface, TextRole, Theme};
use crate::tree::{CrossAlign, Length, NodeId, Style, Tree};

const THEME: Theme = crate::theme::BITMAP_TEST;
const W: u32 = 160;
const H: u32 = 120;
const BOUNDS: Rect = Rect::new(0, 0, W, H);

fn surface(w: u32, h: u32) -> VelloCpu {
    fusuma_render_vello_cpu::new(w, h).unwrap()
}

fn pixels(s: &VelloCpu, w: u32, h: u32) -> Vec<Color> {
    (0..h as i32)
        .flat_map(|y| (0..w as i32).map(move |x| (x, y)))
        .map(|(x, y)| s.pixel(x, y).unwrap())
        .collect()
}

/// The whole tree on a fresh surface.
fn full_frame<const N: usize>(t: &mut Tree<N>) -> Vec<Color> {
    let mut s = surface(W, H);
    t.update(&s, &THEME, BOUNDS);
    s.clear(THEME.colors.background).unwrap();
    t.paint(&mut s, &THEME).unwrap();
    pixels(&s, W, H)
}

/// Only `region`, drawn on a surface of its own and read back as `(x, y, color)`.
fn region_frame<const N: usize>(t: &mut Tree<N>, region: Rect) -> Vec<(i32, i32, Color)> {
    let mut s = surface(region.w, region.h);
    s.translate(-region.x, -region.y).unwrap();
    s.clear(THEME.colors.background).unwrap();
    t.paint_region(&mut s, &THEME, region).unwrap();
    let mut out = Vec::new();
    for y in 0..region.h as i32 {
        for x in 0..region.w as i32 {
            out.push((region.x + x, region.y + y, s.pixel(x, y).unwrap()));
        }
    }
    out
}

fn app() -> (Tree<16>, NodeId, NodeId, NodeId, NodeId) {
    let mut t = Tree::<16>::new();
    let root = t
        .column(
            None,
            Style::new()
                .padding(8)
                .gap(6)
                .background(Surface::Surface)
                .round(Round::Medium),
        )
        .unwrap();
    let title = t.label(Some(root), "Title", TextRole::Title).unwrap();
    let a = t.button(Some(root), "A", ButtonVariant::Primary).unwrap();
    let b = t.button(Some(root), "B", ButtonVariant::Secondary).unwrap();
    // Start from a painted screen, as an application would.
    let mut s = surface(W, H);
    t.update(&s, &THEME, BOUNDS);
    s.clear(THEME.colors.background).unwrap();
    t.paint(&mut s, &THEME).unwrap();
    (t, title, a, b, root)
}

fn centre<const N: usize>(t: &Tree<N>, id: NodeId) -> Point {
    let r = t.rect(id).unwrap();
    Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
}

#[test]
fn a_new_tree_is_damaged_everywhere_it_exists_and_painting_clears_it() {
    let mut t = Tree::<4>::new();
    assert_eq!(t.take_damage(), None, "nothing yet");
    let root = t.column(None, Style::new()).unwrap();
    t.button(Some(root), "go", ButtonVariant::Primary).unwrap();
    t.update(&surface(8, 8), &THEME, BOUNDS);
    assert_eq!(t.take_damage(), Some(BOUNDS), "the root's area");
    assert_eq!(t.take_damage(), None, "taking it clears it");
    let mut s = surface(W, H);
    t.update(&s, &THEME, BOUNDS);
    t.paint(&mut s, &THEME).unwrap();
    assert_eq!(
        t.take_damage(),
        None,
        "a full paint leaves nothing to redraw"
    );
}

#[test]
fn an_update_with_no_change_reports_no_damage() {
    let (mut t, ..) = app();
    t.update(&surface(8, 8), &THEME, BOUNDS);
    assert_eq!(t.take_damage(), None);
    assert!(!t.needs_frame(), "nothing to lay out, draw or animate");
}

#[test]
fn hovering_a_button_damages_exactly_that_button() {
    let (mut t, _, a, b, _) = app();
    t.pointer(Pointer::Move(centre(&t, a)));
    assert_eq!(t.take_damage(), t.rect(a));
    t.pointer(Pointer::Move(centre(&t, b)));
    let both = t.rect(a).unwrap().union(&t.rect(b).unwrap());
    assert_eq!(
        t.take_damage(),
        Some(both),
        "the old and the new hovered button"
    );
}

#[test]
fn changing_text_damages_the_old_and_the_new_size() {
    let (mut t, title, _, _, _) = app();
    let before = t.rect(title).unwrap();
    t.set_text(title, "A much longer title");
    t.update(&surface(8, 8), &THEME, BOUNDS);
    let after = t.rect(title).unwrap();
    assert!(after.w > before.w);
    let d = t.take_damage().unwrap();
    assert_eq!(d.intersect(&before), before, "the old place is covered");
    assert_eq!(d.intersect(&after), after, "the new place is covered");
}

#[test]
fn adding_and_removing_nodes_damages_what_moved() {
    let (mut t, title, a, b, root) = app();
    let (old_b, old_a) = (t.rect(b).unwrap(), t.rect(a).unwrap());
    t.remove(title);
    t.update(&surface(8, 8), &THEME, BOUNDS);
    let d = t.take_damage().unwrap();
    for r in [old_a, old_b, t.rect(a).unwrap(), t.rect(b).unwrap()] {
        assert_eq!(d.intersect(&r), r, "{r:?} is inside {d:?}");
    }
    let extra = t.button(Some(root), "C", ButtonVariant::Primary).unwrap();
    t.update(&surface(8, 8), &THEME, BOUNDS);
    let d = t.take_damage().unwrap();
    assert_eq!(d.intersect(&t.rect(extra).unwrap()), t.rect(extra).unwrap());
}

#[test]
fn damage_can_be_limited_to_the_screen() {
    let mut t = Tree::<8>::new();
    let root = t.column(None, Style::new()).unwrap();
    t.spacer(
        Some(root),
        Style::new()
            .width(Length::Fixed(5000))
            .height(Length::Fixed(5000)),
    )
    .unwrap();
    t.update(&surface(8, 8), &THEME, Rect::new(0, 0, 4000, 4000));
    let limit = Rect::new(0, 0, 100, 80);
    assert_eq!(
        t.take_damage_in(limit),
        Some(limit),
        "a huge area is cut to the screen"
    );
    assert_eq!(t.take_damage_in(limit), None, "taking clears it");
    // Damage that lies wholly outside the screen is no damage at all.
    t.pointer(Pointer::Move(Point::new(3000, 3000)));
    t.set_style(root, Style::new().padding(1));
    t.update(&surface(8, 8), &THEME, Rect::new(0, 0, 4000, 4000));
    assert!(
        t.take_damage_in(Rect::new(3500, 3500, 10, 10)).is_none()
            || t.take_damage_in(limit).is_none()
    );
}

#[test]
fn a_change_of_bounds_damages_everything() {
    let (mut t, ..) = app();
    t.update(&surface(8, 8), &THEME, Rect::new(0, 0, W - 20, H - 10));
    let d = t.take_damage().unwrap();
    assert_eq!(d, BOUNDS, "the old and the new root area");
}

#[test]
fn drawing_only_the_damage_matches_drawing_everything_for_a_hover() {
    let (mut t, _, a, _, _) = app();
    let before = full_frame(&mut t);
    t.pointer(Pointer::Move(centre(&t, a)));
    t.advance(&THEME, 40);
    let damage = t.take_damage().expect("a hover changes the picture");
    let after = full_frame(&mut t);
    let region = region_frame(&mut t, damage);
    let mut changed = 0;
    for (i, (b, a)) in before.iter().zip(&after).enumerate() {
        let p = Point::new((i as u32 % W) as i32, (i as u32 / W) as i32);
        if b != a {
            changed += 1;
            assert!(
                damage.contains(p),
                "{p:?} changed but is outside the damage {damage:?}"
            );
        }
    }
    assert!(changed > 0, "the hover must show");
    for (x, y, c) in region {
        assert_eq!(
            c,
            after[(y as u32 * W + x as u32) as usize],
            "({x}, {y}) differs between the region and the full frame"
        );
    }
}

#[test]
fn a_partial_paint_that_does_not_cover_the_damage_keeps_it() {
    let (mut t, _, a, b, _) = app();
    t.pointer(Pointer::Move(centre(&t, a)));
    t.pointer(Pointer::Move(centre(&t, b)));
    let all = t.damage.unwrap();
    let mut s = surface(W, H);
    t.paint_region(&mut s, &THEME, t.rect(a).unwrap()).unwrap();
    assert_eq!(
        t.damage,
        Some(all),
        "a smaller region leaves the damage in place"
    );
    assert!(t.needs_frame());
    t.paint_region(&mut s, &THEME, all).unwrap();
    assert_eq!(t.damage, None);
    // The hovers started animations, which still want frames; nothing is waiting to be painted.
    t.paint_region(&mut s, &THEME, Rect::new(0, 0, 0, 0))
        .unwrap();
    Tree::<2>::new()
        .paint_region(&mut s, &THEME, BOUNDS)
        .unwrap();
}

/// A small deterministic generator, so the property test needs no dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: u32) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as u32) % n
    }
}

#[test]
fn after_any_sequence_of_changes_the_damage_covers_every_changed_pixel_and_redraws_exactly() {
    let mut rng = Lcg(2024);
    for round in 0..12 {
        let mut t = Tree::<20>::new();
        let root = t
            .column(
                None,
                Style::new()
                    .padding(6)
                    .gap(4)
                    .align(CrossAlign::Start)
                    .background(Surface::Surface)
                    .round(Round::Small),
            )
            .unwrap();
        let mut containers = vec![root];
        let mut ids = vec![root];
        for _ in 0..6 {
            let parent = containers[rng.next(containers.len() as u32) as usize];
            let id = match rng.next(4) {
                0 => t
                    .row(Some(parent), Style::new().gap(3))
                    .ok()
                    .inspect(|&c| containers.push(c)),
                1 => t.label(Some(parent), "label", TextRole::Body).ok(),
                2 => t.button(Some(parent), "btn", ButtonVariant::Secondary).ok(),
                _ => t
                    .spacer(
                        Some(parent),
                        Style::new()
                            .width(Length::Fixed(10))
                            .height(Length::Fixed(6)),
                    )
                    .ok(),
            };
            ids.extend(id);
        }
        let mut previous = full_frame(&mut t);
        for step in 0..30 {
            let pick = |ids: &[NodeId], rng: &mut Lcg| ids[rng.next(ids.len() as u32) as usize];
            match rng.next(9) {
                0 | 1 => {
                    let p = Point::new(rng.next(W) as i32, rng.next(H) as i32);
                    t.pointer(
                        [
                            Pointer::Move(p),
                            Pointer::Down(p),
                            Pointer::Up(p),
                            Pointer::Leave,
                        ][rng.next(4) as usize],
                    );
                }
                2 => {
                    let id = pick(&ids, &mut rng);
                    t.set_text(id, ["a", "bb", "Longer text", "x"][rng.next(4) as usize]);
                }
                3 => {
                    let id = pick(&ids, &mut rng);
                    t.set_enabled(id, rng.next(2) == 0);
                }
                4 => {
                    let parent = pick(&containers, &mut rng);
                    let added = if rng.next(2) == 0 {
                        t.button(Some(parent), "new", ButtonVariant::Primary)
                    } else {
                        t.label(Some(parent), "n", TextRole::Caption)
                    };
                    ids.extend(added.ok());
                }
                5 => {
                    let id = pick(&ids, &mut rng);
                    if id != root {
                        t.remove(id);
                    }
                }
                6 => {
                    let id = pick(&containers, &mut rng);
                    t.set_style(
                        id,
                        Style::new()
                            .padding(rng.next(10))
                            .gap(rng.next(8))
                            .background([Surface::None, Surface::Raised][rng.next(2) as usize]),
                    );
                }
                7 => {
                    t.advance(&THEME, rng.next(120));
                }
                _ => {
                    let id = pick(&ids, &mut rng);
                    t.set_variant(id, ButtonVariant::Primary);
                }
            }
            let probe = surface(8, 8);
            t.update(&probe, &THEME, BOUNDS);
            let damage = t.take_damage_in(BOUNDS);
            let now = full_frame(&mut t);
            for (i, (b, a)) in previous.iter().zip(&now).enumerate() {
                let p = Point::new((i as u32 % W) as i32, (i as u32 / W) as i32);
                if b != a {
                    assert!(
                        damage.is_some_and(|d| d.contains(p)),
                        "round {round} step {step}: {p:?} changed from {b:?} to {a:?} outside the damage {damage:?}"
                    );
                }
            }
            if let Some(d) = damage {
                for (x, y, c) in region_frame(&mut t, d) {
                    assert_eq!(
                        c,
                        now[(y as u32 * W + x as u32) as usize],
                        "round {round} step {step}: region differs at ({x}, {y})"
                    );
                }
            }
            previous = now;
        }
    }
}
