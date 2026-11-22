//! The public API as a user of the crate sees it.

use tokonoma::{Color, Point, Rect, Size, triangle};

#[test]
fn types_compose() {
    let r = Rect::new(0, 0, 10, 10).intersect(&Rect::new(5, 5, 10, 10));
    assert_eq!(r, Rect::new(5, 5, 5, 5));
    assert_eq!(Size::new(3, 4).w, 3);
    assert_eq!(Color::from_rgb565(Color::rgb(255, 0, 0).to_rgb565()).r, 255);
    assert_eq!(
        triangle(Point::new(0, 0), Point::new(1, 0), Point::new(0, 1)).len(),
        4
    );
}
