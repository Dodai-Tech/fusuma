#![no_main]
//! Random operations on a retained tree never panic and never break its links: adding under stale or
//! wrong parents, removing, changing text and style, disabling, pointer events in any order,
//! layout in absurd bounds, animation and painting.

use fusuma_ui::{BuildError, ButtonVariant, ChartStyle, CrossAlign, Key, Length, NodeId, Pointer, Style, TextRole, Theme, Tree};
use fusuma_render::{Point, Rect, Renderer};
use fusuma_render_integer::{Embedded, Frame};
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
    // The renderer that can move pixels, which a platform frame uses.
    let mut frame_screen = fusuma_render_vello_cpu::new(48, 40).unwrap();
    let theme = if data.first().is_some_and(|b| b & 1 == 1) { Theme::LIGHT } else { Theme::DARK };
    let mut it = data.iter().copied();
    while let (Some(op), Some(a), Some(b)) = (it.next(), it.next(), it.next()) {
        let pick = |ids: &[NodeId], k: u8| ids.get(usize::from(k) % ids.len().max(1)).copied();
        let point = Point::new(i32::from(a) * 3 - 30, i32::from(b) * 3 - 30);
        match op % 22 {
            0..=4 => {
                // The parent is sometimes none, sometimes stale, sometimes a leaf.
                let parent = if a % 5 == 0 { None } else { pick(&ids, a) };
                let added: Result<NodeId, BuildError> = match op % 22 {
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
                // A frame as a platform runs it: a fresh scene, then only what changed, shifting a scroll's pixels.
                frame_screen.with_scene(fusuma_render_vello_cpu::Scene::reset);
                let bounds = Rect::new(0, 0, 48, 40);
                tree.update(&frame_screen, &theme, bounds);
                if let Ok(Some(shown)) = tree.paint_frame(&mut frame_screen, &theme, bounds) {
                    assert!(shown.x >= 0 && shown.y >= 0 && shown.right() <= 48 && shown.bottom() <= 40, "{shown:?}");
                    let _ = fusuma_render::Probe::pixel(&frame_screen, 0, 0);
                }
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
            15 => {
                // The widgets with state, and the scroll container that holds them.
                let parent = if a % 5 == 0 { None } else { pick(&ids, a) };
                let added = match b % 7 {
                    0 => tree.scroll(parent, style(a, b)),
                    1 => tree.checkbox(parent, "c", a % 2 == 0),
                    2 => tree.switch(parent, "s", a % 2 == 0),
                    3 => tree.slider(parent, u16::from(a) * 7),
                    4 => tree.list_item(parent, "l", a % 2 == 0),
                    6 => tree.dialog(parent, style(a, b)),
                    5 if a % 3 == 0 => tree.text_input(parent, "héllo wörld"),
                    _ => tree.radio(parent, "r", a % 2 == 0),
                };
                if let Ok(id) = added {
                    ids.push(id);
                }
            }
            16 => {
                tree.pointer(Pointer::Wheel(point, i32::from(a) * 13 - 1500));
                if let Some(id) = pick(&ids, b) {
                    tree.scroll_to(id, i32::from(a) * i32::from(b) - 5000);
                    tree.set_slider(id, u16::from(a) * 9);
                    // Dialogs: opening and closing whatever is picked (most picks are not dialogs, which is a no-op).
                    if a % 2 == 0 {
                        tree.open_dialog(id);
                    } else {
                        tree.close_dialog(id);
                    }
                    let _ = tree.is_dialog_open(id);
                    let _ = (tree.scroll_offset(id), tree.scroll_max(id), tree.slider_value(id), tree.is_checked(id));
                }
            }
            17 => {
                if let Some(id) = pick(&ids, b) {
                    let _ = tree.focus(id);
                }
                tree.key([Key::Next, Key::Prev, Key::Up, Key::Down, Key::Activate, Key::Left, Key::Right, Key::Cancel][usize::from(a) % 8]);
            }
            18 => {
                // Typing, edit commands, an input method and a clipboard read, on whatever has the focus.
                let words = ["", "a", "é", "日本語", "x\ny\tz", " ", "héllo wörld, a longer piece of text to fill the field up"];
                if let Some(id) = pick(&ids, b) {
                    let _ = tree.focus(id);
                    tree.set_input_cursor(id, usize::from(a));
                    let _ = tree.input_cursor(id);
                }
                match a % 5 {
                    0 => tree.type_text(words[usize::from(b) % words.len()]),
                    1 => tree.set_preedit(words[usize::from(b) % words.len()]),
                    2 => {
                        use fusuma_ui::Edit::{Backspace, Delete, End, Home, Left, Right, SelectAll};
                        let all = [Backspace, Delete, Left(true), Left(false), Right(true), Right(false), Home(true), End(false), SelectAll];
                        tree.edit(all[usize::from(b) % all.len()]);
                    }
                    3 => {
                        let _ = (tree.selected_text(), tree.text_cursor(), tree.wants_text(), tree.preedit().len());
                    }
                    _ => {
                        if let Some(id) = pick(&ids, a) {
                            tree.set_text(id, words[usize::from(b) % words.len()]);
                        }
                    }
                }
            }
            19 => {
                // The newer nodes: wrapping rows and grids, charts, gauges, spinners, splitters, images, text areas, sheets.
                static PIXELS: [u8; 16] = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 9, 9, 9, 9];
                let parent = if a % 5 == 0 { None } else { pick(&ids, a) };
                let samples: Vec<u8> = (0..usize::from(b % 40)).map(|i| (i as u8).wrapping_mul(a)).collect();
                let added = match b % 11 {
                    0 => tree.flow(parent, style(a, b)),
                    1 => tree.grid(parent, a % 9, style(b, a)),
                    2 => tree.chart(parent, [ChartStyle::Line, ChartStyle::Area, ChartStyle::Bars][usize::from(a) % 3], &samples),
                    3 => tree.gauge(parent, u16::from(a) * 9, &"7".repeat(usize::from(b % 6))),
                    4 => tree.spinner(parent),
                    5 => tree.splitter(parent, u16::from(a) * 9),
                    6 => tree.image(parent, u32::from(a % 3) + 1, u32::from(b % 3) + 1, &PIXELS),
                    7 => tree.text_area(parent, "one\ntwo\nthree", a % 6),
                    8 => tree.bottom_sheet(parent, style(a, b)),
                    9 => tree.menu(parent, style(a, b)),
                    _ => tree.text_input(parent, "x"),
                };
                if let Ok(id) = added {
                    ids.push(id);
                }
            }
            20 => {
                // Interaction state that is set on whatever is picked: a drag and a drop, direction, charts and gauges changed.
                if let Some(id) = pick(&ids, a) {
                    tree.set_draggable(id, b % 2 == 0);
                    tree.set_drop_target(id, b % 3 == 0);
                    tree.set_gauge(id, u16::from(b) * 11);
                    tree.set_chart(id, ChartStyle::Bars, &[b, a, b]);
                    let _ = (tree.chart_samples(id), tree.gauge_value(id), tree.spinner_phase(id));
                }
                tree.set_rtl(a % 2 == 0);
                let _ = (tree.dragging(), tree.drag_over(), tree.drag_ghost());
                // A drag in any order: down, far moves, up, leave, escape.
                for e in [Pointer::Down(point), Pointer::Move(Point::new(point.x + 40, point.y + 30)), Pointer::Move(point), Pointer::Up(point)] {
                    tree.pointer(e);
                }
                if b % 4 == 0 {
                    tree.key(Key::Cancel);
                }
            }
            21 => {
                // Text areas: line breaks, moves between lines, on whatever has the focus.
                if let Some(id) = pick(&ids, b) {
                    let _ = tree.focus(id);
                    tree.set_input_cursor(id, usize::from(a));
                }
                use fusuma_ui::Edit::{Down, End, Home, Newline, SelectAll, Up};
                let all = [Newline, Up(false), Down(false), Up(true), Down(true), Home(false), End(true), SelectAll];
                tree.edit(all[usize::from(a) % all.len()]);
                tree.key([Key::Activate, Key::Up, Key::Down][usize::from(b) % 3]);
                tree.type_text(["a\nb", "\n\n", "é"][usize::from(a) % 3]);
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
