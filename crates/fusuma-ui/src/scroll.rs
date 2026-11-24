//! Tests of the scroll container: layout, input, painting and damage.

use fusuma_render::{Point, Probe, Rect, Renderer};
use fusuma_render_vello_cpu::VelloCpu;

use crate::event::Pointer;
use crate::focus::Key;
use crate::theme::{ButtonVariant, Theme};
use crate::tree::{ActionKind, CrossAlign, Length, NodeId, Style, Tree};

const THEME: Theme = crate::theme::BITMAP_TEST;
const W: u32 = 200;
const H: u32 = 300;
/// The scroller's viewport height.
const VIEW: u32 = 100;

struct Ui {
    t: Tree<32>,
    scroll: NodeId,
    items: [NodeId; 10],
    below: NodeId,
}

fn screen() -> VelloCpu {
    fusuma_render_vello_cpu::new(W, H).unwrap()
}

/// A column holding a scroller of ten list rows (44 px each, 4 apart: 476 px) in a 100 px window, and a
/// button under it.
fn ui() -> Ui {
    let mut t = Tree::<32>::new();
    let root = t
        .column(None, Style::new().gap(8).align(CrossAlign::Stretch))
        .unwrap();
    let scroll = t
        .scroll(
            Some(root),
            Style::new()
                .height(Length::Fixed(VIEW))
                .gap(4)
                .align(CrossAlign::Stretch),
        )
        .unwrap();
    let items: Vec<NodeId> = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"]
        .iter()
        .map(|name| t.list_item(Some(scroll), name, false).unwrap())
        .collect();
    let items: [NodeId; 10] = items.try_into().unwrap();
    let below = t
        .button(Some(root), "Below", ButtonVariant::Primary)
        .unwrap();
    t.update(&screen(), &THEME, Rect::new(0, 0, W, H));
    Ui {
        t,
        scroll,
        items,
        below,
    }
}

fn relayout(u: &mut Ui) {
    u.t.update(&screen(), &THEME, Rect::new(0, 0, W, H));
}

fn mid(u: &Ui, id: NodeId) -> Point {
    let r = u.t.rect(id).unwrap();
    Point::new(r.x + (r.w / 2) as i32, r.y + (r.h / 2) as i32)
}

#[test]
fn the_content_is_measured_and_the_scroll_range_is_what_does_not_fit() {
    let u = ui();
    let r = u.t.rect(u.scroll).unwrap();
    assert_eq!(r.h, VIEW, "a fixed height is kept");
    assert_eq!(u.t.scroll_offset(u.scroll), Some(0));
    assert_eq!(
        u.t.scroll_max(u.scroll),
        Some(10 * 44 + 9 * 4 - VIEW as i32)
    );
    // The rows keep their natural height and are laid out one after the other.
    assert_eq!(u.t.rect(u.items[0]).unwrap().y, r.y);
    assert_eq!(u.t.rect(u.items[1]).unwrap().y, r.y + 48);
    assert_eq!(u.t.rect(u.items[9]).unwrap().h, 44);
}

#[test]
fn scrolling_moves_the_children_and_is_clamped_at_both_ends() {
    let mut u = ui();
    let top = u.t.rect(u.scroll).unwrap().y;
    u.t.scroll_to(u.scroll, 60);
    relayout(&mut u);
    assert_eq!(u.t.scroll_offset(u.scroll), Some(60));
    assert_eq!(u.t.rect(u.items[0]).unwrap().y, top - 60);
    assert_eq!(u.t.rect(u.items[2]).unwrap().y, top + 96 - 60);
    u.t.scroll_to(u.scroll, 10_000);
    relayout(&mut u);
    assert_eq!(u.t.scroll_offset(u.scroll), u.t.scroll_max(u.scroll));
    let last = u.t.rect(u.items[9]).unwrap();
    assert_eq!(
        last.bottom(),
        top + VIEW as i32,
        "the last row ends at the bottom edge"
    );
    u.t.scroll_to(u.scroll, -50);
    assert_eq!(u.t.scroll_offset(u.scroll), Some(0));
}

#[test]
fn the_wheel_scrolls_the_container_under_it_and_nothing_else() {
    let mut u = ui();
    let inside = mid(&u, u.items[0]);
    u.t.pointer(Pointer::Wheel(inside, 30));
    assert_eq!(u.t.scroll_offset(u.scroll), Some(30));
    u.t.pointer(Pointer::Wheel(inside, -100));
    assert_eq!(u.t.scroll_offset(u.scroll), Some(0), "clamped");
    let outside = mid(&u, u.below);
    u.t.pointer(Pointer::Wheel(outside, 30));
    assert_eq!(
        u.t.scroll_offset(u.scroll),
        Some(0),
        "the wheel over something else does nothing"
    );
    u.t.pointer(Pointer::Wheel(Point::new(-50, -50), 30));
    assert_eq!(u.t.scroll_offset(u.scroll), Some(0));
}

#[test]
fn a_row_scrolled_out_of_the_window_cannot_be_hit_and_does_not_cover_what_is_under_it() {
    let mut u = ui();
    relayout(&mut u);
    // Row "e" (index 4) starts at 176, below the window (it ends at 100 + the top).
    let hidden = mid(&u, u.items[4]);
    assert!(hidden.y > u.t.rect(u.scroll).unwrap().bottom());
    // The button is placed under the scroller, outside it; tap where a hidden row would be if it
    // were not clipped: that is the button's area or empty space, never a row.
    let b = mid(&u, u.below);
    u.t.pointer(Pointer::Down(b));
    u.t.pointer(Pointer::Up(b));
    let a = u.t.pop_action().unwrap();
    assert_eq!((a.source, a.kind), (u.below, ActionKind::Clicked));
    assert_ne!(
        u.t.hit(hidden),
        Some(usize::from(u.items[4].index)),
        "a hidden row is not hit"
    );
}

#[test]
fn a_tap_on_a_visible_row_chooses_it() {
    let mut u = ui();
    let p = mid(&u, u.items[1]);
    u.t.pointer(Pointer::Down(p));
    u.t.pointer(Pointer::Up(p));
    assert!(u.t.is_checked(u.items[1]));
    assert_eq!(u.t.pop_action().unwrap().kind, ActionKind::Selected);
}

#[test]
fn a_drag_scrolls_without_clicking_and_follows_the_finger() {
    let mut u = ui();
    let start = mid(&u, u.items[1]);
    u.t.pointer(Pointer::Down(start));
    u.t.pointer(Pointer::Move(Point::new(start.x, start.y - 5)));
    assert_eq!(
        u.t.scroll_offset(u.scroll),
        Some(0),
        "under the slop it is still a press"
    );
    u.t.pointer(Pointer::Move(Point::new(start.x, start.y - 30)));
    assert_eq!(u.t.scroll_offset(u.scroll), Some(30));
    u.t.pointer(Pointer::Move(Point::new(start.x, start.y - 50)));
    assert_eq!(u.t.scroll_offset(u.scroll), Some(50));
    u.t.pointer(Pointer::Move(Point::new(start.x, start.y + 10)));
    assert_eq!(
        u.t.scroll_offset(u.scroll),
        Some(0),
        "dragging back, clamped at the top"
    );
    u.t.pointer(Pointer::Up(Point::new(start.x, start.y + 10)));
    assert_eq!(u.t.pop_action(), None, "a scroll is not a click");
    assert!(!u.t.is_checked(u.items[1]));
}

#[test]
fn a_press_that_barely_moves_is_a_click_and_a_drag_leaves_the_row_unpressed() {
    let mut u = ui();
    let p = mid(&u, u.items[1]);
    u.t.pointer(Pointer::Down(p));
    u.t.pointer(Pointer::Move(Point::new(p.x, p.y - 3)));
    u.t.pointer(Pointer::Up(Point::new(p.x, p.y - 3)));
    assert_eq!(u.t.scroll_offset(u.scroll), Some(0));
    assert!(u.t.is_checked(u.items[1]));
    // A drag lets the pressed row go at once.
    u.t.pointer(Pointer::Leave);
    let q = mid(&u, u.items[0]);
    u.t.pointer(Pointer::Down(q));
    u.t.pointer(Pointer::Move(Point::new(q.x, q.y - 40)));
    assert!(!u.t.nodes[usize::from(u.items[0].index)].active);
    u.t.pointer(Pointer::Up(Point::new(q.x, q.y - 40)));
}

#[test]
fn a_drag_that_starts_outside_any_scroller_scrolls_nothing() {
    let mut u = ui();
    let b = mid(&u, u.below);
    u.t.pointer(Pointer::Down(b));
    u.t.pointer(Pointer::Move(Point::new(b.x, b.y - 60)));
    assert_eq!(u.t.scroll_offset(u.scroll), Some(0));
    u.t.pointer(Pointer::Up(b));
}

#[test]
fn moving_the_focus_with_the_keys_scrolls_the_row_into_view() {
    let mut u = ui();
    u.t.focus(u.items[0]);
    for _ in 0..4 {
        u.t.key(Key::Next);
    }
    relayout(&mut u);
    let view = u.t.rect(u.scroll).unwrap();
    let focused = u.t.rect(u.t.focused().unwrap()).unwrap();
    assert!(
        focused.y >= view.y && focused.bottom() <= view.bottom(),
        "{focused:?} in {view:?}"
    );
    assert!(u.t.scroll_offset(u.scroll).unwrap() > 0);
    // And back to the top.
    u.t.focus(u.items[0]);
    relayout(&mut u);
    assert_eq!(u.t.scroll_offset(u.scroll), Some(0));
    // Tab from the last row wraps to the first and scrolls back.
    u.t.focus(u.items[9]);
    relayout(&mut u);
    assert_eq!(u.t.scroll_offset(u.scroll), u.t.scroll_max(u.scroll));
}

#[test]
fn content_that_fits_does_not_scroll() {
    let mut t = Tree::<8>::new();
    let root = t.column(None, Style::new()).unwrap();
    let s = t
        .scroll(Some(root), Style::new().height(Length::Fixed(200)))
        .unwrap();
    t.list_item(Some(s), "only", false).unwrap();
    t.update(&screen(), &THEME, Rect::new(0, 0, W, H));
    assert_eq!(t.scroll_max(s), Some(0));
    t.pointer(Pointer::Wheel(Point::new(10, 10), 50));
    assert_eq!(t.scroll_offset(s), Some(0));
    t.scroll_to(s, 20);
    assert_eq!(t.scroll_offset(s), Some(0));
}

#[test]
fn scroll_calls_on_other_nodes_are_harmless() {
    let mut u = ui();
    assert_eq!(u.t.scroll_offset(u.below), None);
    assert_eq!(u.t.scroll_max(u.items[0]), None);
    u.t.scroll_to(u.below, 10);
    let stale = u.below;
    u.t.remove(stale);
    assert_eq!(u.t.scroll_offset(stale), None);
    u.t.scroll_to(stale, 5);
}

fn paint(u: &mut Ui, s: &mut VelloCpu) {
    u.t.update(s, &THEME, Rect::new(0, 0, W, H));
    s.clear(THEME.colors.background).unwrap();
    u.t.paint(s, &THEME).unwrap();
}

#[test]
fn nothing_is_drawn_outside_the_window_and_the_scrollbar_shows_the_position() {
    let mut u = ui();
    let mut s = screen();
    paint(&mut u, &mut s);
    let view = u.t.rect(u.scroll).unwrap();
    // Rows c, d, e are below the window: the gap under it shows the background (the button is further down).
    let gap_y = view.bottom() + 3;
    assert_eq!(
        s.pixel(30, gap_y).unwrap(),
        THEME.colors.background,
        "clipped"
    );
    // The scrollbar is at the right edge, at the top while not scrolled.
    let bar_x = view.right() - 5;
    assert_eq!(
        s.pixel(bar_x, view.y + 6).unwrap(),
        THEME.colors.border_strong,
        "thumb at the top"
    );
    assert_ne!(
        s.pixel(bar_x, view.bottom() - 6).unwrap(),
        THEME.colors.border_strong
    );
    u.t.scroll_to(u.scroll, 10_000);
    paint(&mut u, &mut s);
    assert_eq!(
        s.pixel(bar_x, view.bottom() - 6).unwrap(),
        THEME.colors.border_strong,
        "thumb at the bottom"
    );
    assert_ne!(
        s.pixel(bar_x, view.y + 6).unwrap(),
        THEME.colors.border_strong
    );
}

#[test]
fn damage_from_a_row_is_cut_to_the_window_and_scrolling_redraws_the_window() {
    let mut u = ui();
    let mut s = screen();
    paint(&mut u, &mut s);
    assert_eq!(u.t.take_damage(), None);
    // A row that is mostly below the window changes: only the visible sliver can be damaged.
    u.t.set_text(u.items[2], "changed");
    let d = u.t.take_damage_in(Rect::new(0, 0, W, H));
    let view = u.t.rect(u.scroll).unwrap();
    assert!(
        d.is_none_or(|d| d.bottom() <= view.bottom()),
        "{d:?} within {view:?}"
    );
    paint(&mut u, &mut s);
    u.t.scroll_to(u.scroll, 40);
    u.t.update(&s, &THEME, Rect::new(0, 0, W, H));
    let d = u.t.take_damage_in(Rect::new(0, 0, W, H)).unwrap();
    assert!(
        d.y >= view.y && d.bottom() <= view.bottom(),
        "{d:?} within {view:?}"
    );
    assert!(d.h >= view.h - 4, "the whole window is redrawn: {d:?}");
}

#[test]
fn a_region_redraw_of_a_scrolled_window_matches_a_full_redraw() {
    let mut a = ui();
    let mut b = ui();
    for u in [&mut a, &mut b] {
        u.t.scroll_to(u.scroll, 70);
    }
    let (mut sa, mut sb) = (screen(), screen());
    paint(&mut a, &mut sa);
    // The same picture, drawn as the damage region of a first frame.
    b.t.update(&sb, &THEME, Rect::new(0, 0, W, H));
    sb.clear(THEME.colors.background).unwrap();
    let region = b.t.take_damage_in(Rect::new(0, 0, W, H)).unwrap();
    b.t.paint_region(&mut sb, &THEME, region).unwrap();
    for y in (0..H as i32).step_by(7) {
        for x in (0..W as i32).step_by(5) {
            assert_eq!(sa.pixel(x, y), sb.pixel(x, y), "({x}, {y})");
        }
    }
}

#[test]
fn scrollers_nest_and_a_deep_nest_is_still_balanced() {
    let mut t = Tree::<40>::new();
    let mut parent = t
        .column(None, Style::new().height(Length::Fixed(200)))
        .unwrap();
    for _ in 0..12 {
        parent = t
            .scroll(
                Some(parent),
                Style::new()
                    .height(Length::Fixed(150))
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        t.label(Some(parent), "x", crate::theme::TextRole::Body)
            .unwrap();
    }
    let mut s = screen();
    t.update(&s, &THEME, Rect::new(0, 0, W, H));
    s.clear(THEME.colors.background).unwrap();
    t.paint(&mut s, &THEME).unwrap();
    // The renderer's save/restore stack is balanced: a clip left open would hide this fill.
    s.fill_rect(Rect::new(0, 250, 10, 10), 0, THEME.colors.primary)
        .unwrap();
    assert_eq!(s.pixel(5, 255).unwrap(), THEME.colors.primary);
}

/// A regression found by the fuzzer: every widget painted at the edge of the coordinate range must not
/// overflow.
#[test]
fn every_widget_paints_at_the_edge_of_the_coordinate_range() {
    use crate::theme::TextRole;
    for (x, y) in [
        (i32::MAX - 30, i32::MAX - 30),
        (i32::MIN, i32::MIN),
        (i32::MAX, 0),
        (0, i32::MIN),
    ] {
        let mut t = Tree::<24>::new();
        let root = t.column(None, Style::new().gap(4)).unwrap();
        t.label(Some(root), "l", TextRole::Body).unwrap();
        t.button(Some(root), "b", ButtonVariant::Primary).unwrap();
        t.checkbox(Some(root), "c", true).unwrap();
        t.radio(Some(root), "r", true).unwrap();
        t.switch(Some(root), "s", true).unwrap();
        t.slider(Some(root), 700).unwrap();
        t.progress_bar(Some(root), 300).unwrap();
        let sc = t
            .scroll(Some(root), Style::new().height(Length::Fixed(40)))
            .unwrap();
        for _ in 0..4 {
            t.list_item(Some(sc), "i", true).unwrap();
        }
        let mut s = screen();
        t.update(&s, &THEME, Rect::new(x, y, 120, 400));
        s.clear(THEME.colors.background).unwrap();
        t.paint(&mut s, &THEME).unwrap();
        t.scroll_to(sc, 50);
        t.update(&s, &THEME, Rect::new(x, y, 120, 400));
        t.paint(&mut s, &THEME).unwrap();
    }
}

/// The pixels in `area` that are not `background`.
fn lit(s: &VelloCpu, area: Rect, background: fusuma_render::Color) -> usize {
    let mut n = 0;
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if s.pixel(x, y) != Some(background) {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn text_of_rows_outside_the_window_is_not_drawn() {
    // Nothing under the scroller, so anything lit below it is a row that escaped the clip.
    let mut t = Tree::<16>::new();
    let root = t
        .column(None, Style::new().align(CrossAlign::Stretch))
        .unwrap();
    let sc = t
        .scroll(
            Some(root),
            Style::new()
                .height(Length::Fixed(VIEW))
                .gap(4)
                .align(CrossAlign::Stretch),
        )
        .unwrap();
    for name in ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"] {
        t.list_item(Some(sc), name, false).unwrap();
    }
    let mut s = screen();
    t.update(&s, &THEME, Rect::new(0, 0, W, H));
    s.clear(THEME.colors.background).unwrap();
    t.paint(&mut s, &THEME).unwrap();
    let below = Rect::new(0, VIEW as i32 + 2, W, H - VIEW - 2);
    assert_eq!(
        lit(&s, below, THEME.colors.background),
        0,
        "text and rows are clipped to the window"
    );
    assert!(
        lit(&s, Rect::new(0, 0, W, VIEW), THEME.colors.background) > 0,
        "but the window shows rows"
    );
}

#[test]
fn text_outside_the_window_is_not_drawn_by_a_region_repaint_either() {
    for selected in [false, true] {
        let mut t = Tree::<16>::new();
        let root = t
            .column(None, Style::new().align(CrossAlign::Stretch))
            .unwrap();
        let sc = t
            .scroll(
                Some(root),
                Style::new()
                    .height(Length::Fixed(VIEW))
                    .gap(4)
                    .align(CrossAlign::Stretch),
            )
            .unwrap();
        for (i, name) in ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"]
            .iter()
            .enumerate()
        {
            t.list_item(Some(sc), name, selected && i == 0).unwrap();
        }
        let mut s = screen();
        t.update(&s, &THEME, Rect::new(0, 0, W, H));
        let region = t.take_damage_in(Rect::new(0, 0, W, H)).unwrap();
        s.save().unwrap();
        s.clip(region).unwrap();
        s.clear(THEME.colors.background).unwrap();
        t.paint_region(&mut s, &THEME, region).unwrap();
        s.restore().unwrap();
        let below = Rect::new(0, VIEW as i32 + 2, W, H - VIEW - 2);
        assert_eq!(
            lit(&s, below, THEME.colors.background),
            0,
            "selected: {selected}"
        );
    }
}
