#![no_main]
//! Random operations on a retained tree never panic and never break its links: adding under stale or
//! wrong parents, removing, changing text and style, disabling, pointer events in any order,
//! layout in absurd bounds, animation and painting.

use fusuma::{BuildError, ButtonVariant, CrossAlign, Length, NodeId, Pointer, Style, TextRole, Theme, Tree};
use fusuma_render::{Point, Rect, Renderer};
use fusuma_render_embedded::{Embedded, Frame};
use libfuzzer_sys::fuzz_target;

const N: usize = 12;

fn screen_renderer() -> Embedded<Frame<Vec<u16>>> {
    Embedded::new(Frame::new(48, 40, vec![0u16; 48 * 40]).unwrap())
}

fn length(a: u8, b: u8) -> Length {
    match a % 3 {
        0 => Length::Content,
        1 => Length::Fixed(u32::from(b) << (a % 25)),
        _ => Length::Fill(u16::from(b)),
    }
}

fn style(a: u8, b: u8) -> Style {
    Style::new()
        .width(length(a, b))
        .height(length(b, a))
        .padding(u32::from(a) << (b % 26))
        .gap(u32::from(b) << (a % 26))
        .align([CrossAlign::Start, CrossAlign::Center, CrossAlign::End, CrossAlign::Stretch][usize::from(a >> 4) % 4])
}

fn check<const M: usize>(t: &Tree<M>, ids: &[NodeId]) {
    assert!(t.len() <= M);
    for &id in ids {
        for c in t.children(id) {
            assert_eq!(t.parent(c), Some(id), "a child knows its parent");
        }
    }
}

fuzz_target!(|data: &[u8]| {
    let mut tree = Tree::<N>::new();
    let mut ids: Vec<NodeId> = Vec::new();
    let mut screen = Embedded::new(Frame::new(48, 40, vec![0u16; 48 * 40]).unwrap());
    let theme = if data.first().is_some_and(|b| b & 1 == 1) { Theme::LIGHT } else { Theme::DARK };
    let mut it = data.iter().copied();
    while let (Some(op), Some(a), Some(b)) = (it.next(), it.next(), it.next()) {
        let pick = |ids: &[NodeId], k: u8| ids.get(usize::from(k) % ids.len().max(1)).copied();
        let point = Point::new(i32::from(a) * 3 - 30, i32::from(b) * 3 - 30);
        match op % 15 {
            0..=4 => {
                // The parent is sometimes none, sometimes stale, sometimes a leaf.
                let parent = if a % 5 == 0 { None } else { pick(&ids, a) };
                let added: Result<NodeId, BuildError> = match op % 15 {
                    0 => tree.column(parent, style(a, b)),
                    1 => tree.row(parent, style(b, a)),
                    2 => tree.label(parent, &"éa".repeat(usize::from(b % 30)), [TextRole::Title, TextRole::Body, TextRole::Caption][usize::from(a) % 3]),
                    3 => tree.button(parent, "ok", [ButtonVariant::Primary, ButtonVariant::Secondary][usize::from(a) % 2]),
                    _ => tree.spacer(parent, style(a, b)),
                };
                if let Ok(id) = added {
                    ids.push(id);
                }
            }
            5 => {
                if let Some(id) = pick(&ids, a) {
                    tree.remove(id);
                }
            }
            6 => {
                if let Some(id) = pick(&ids, a) {
                    tree.set_text(id, &"x".repeat(usize::from(b)));
                }
            }
            7 => {
                if let Some(id) = pick(&ids, a) {
                    tree.set_style(id, style(a, b));
                }
            }
            8 => {
                if let Some(id) = pick(&ids, a) {
                    tree.set_enabled(id, b % 2 == 0);
                    tree.set_variant(id, ButtonVariant::Secondary);
                }
            }
            9 => tree.pointer([Pointer::Down(point), Pointer::Move(point), Pointer::Up(point), Pointer::Leave][usize::from(b) % 4]),
            10 => {
                let bounds = match b % 4 {
                    0 => Rect::new(0, 0, 48, 40),
                    1 => Rect::new(i32::from(a) - 100, i32::from(b), u32::from(a) * 40, u32::from(b) * 40),
                    2 => Rect::new(i32::MIN, i32::MAX, u32::MAX, u32::MAX),
                    _ => Rect::new(0, 0, 0, 0),
                };
                tree.update(&screen, &theme, bounds);
            }
            11 => {
                let _ = tree.advance(&theme, u32::from(a) << (b % 25));
            }
            12 => {
                let _ = screen.clear(theme.colors.background);
                let _ = tree.paint(&mut screen, &theme);
            }
            13 => {
                while tree.pop_action().is_some() {}
                // Damage: limited to the screen it always fits, and redrawing it never fails.
                let screen = Rect::new(0, 0, 48, 40);
                if let Some(d) = tree.take_damage_in(screen) {
                    assert!(d.x >= 0 && d.y >= 0 && d.right() <= 48 && d.bottom() <= 40, "{d:?} leaves the screen");
                    let _ = tree.paint_region(&mut screen_renderer(), &theme, d);
                }
                let _ = tree.take_damage();
            }
            _ => {
                if let Some(id) = pick(&ids, a) {
                    let _ = (tree.rect(id), tree.text(id), tree.is_hovered(id), tree.is_active(id), tree.is_disabled(id), tree.button_blend(id));
                }
                let _ = tree.needs_frame();
            }
        }
        check(&tree, &ids);
    }
});
