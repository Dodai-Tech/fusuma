//! The facade is the only crate an app adds: everything reachable from it by its documented paths.

use fusuma::render::{Color, Probe, Rect, Renderer};
use fusuma::{ButtonVariant, Pointer, Style, Theme, Tree};

#[test]
fn an_app_can_build_draw_and_click_using_only_the_facade() {
    let mut tree = Tree::<4>::new();
    let root = tree.column(None, Style::new()).unwrap();
    let ok = tree
        .button(Some(root), "OK", ButtonVariant::Primary)
        .unwrap();
    let mut screen = fusuma::renderers::vello_cpu::new(120, 80).unwrap();
    let theme = Theme::DARK;
    tree.update(&screen, &theme, Rect::new(0, 0, 120, 80));
    screen.clear(theme.colors.background).unwrap();
    tree.paint(&mut screen, &theme).unwrap();
    let r = tree.rect(ok).unwrap();
    let p = fusuma::render::Point::new(r.x + 2, r.y + 2);
    tree.pointer(Pointer::Down(p));
    tree.pointer(Pointer::Up(p));
    assert_eq!(tree.pop_action().unwrap().source, ok);
    assert_ne!(screen.pixel(r.x + 3, r.y + 3).unwrap(), Color::BLACK);
}

#[test]
fn the_other_crates_are_reachable_by_name() {
    use fusuma::anim::Fx;
    assert_eq!(Fx::HALF + Fx::HALF, Fx::ONE);
    let machine = fusuma::platform::Machine {
        screen: Some((100, 100)),
        ..Default::default()
    };
    assert!(fusuma::platform::automatic(&machine).is_some());
}
