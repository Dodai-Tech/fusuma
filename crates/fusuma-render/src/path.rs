//! Vector paths as a slice of commands, so they need no allocation (a `const` array works).

use crate::geometry::Point;

/// One path command. Coordinates are in pixels; a path normally starts with `MoveTo`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathCmd {
    /// Starts a new sub-path.
    MoveTo(Point),
    /// A straight line to the point.
    LineTo(Point),
    /// A quadratic curve: control point, then end point.
    QuadTo(Point, Point),
    /// A cubic curve: two control points, then the end point.
    CubicTo(Point, Point, Point),
    /// Closes the current sub-path with a straight line to its start.
    Close,
}

/// The four commands of a closed triangle, as a convenience and a test fixture.
#[must_use]
pub const fn triangle(a: Point, b: Point, c: Point) -> [PathCmd; 4] {
    [
        PathCmd::MoveTo(a),
        PathCmd::LineTo(b),
        PathCmd::LineTo(c),
        PathCmd::Close,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triangle_is_closed() {
        let t = triangle(Point::new(0, 0), Point::new(4, 0), Point::new(2, 3));
        assert_eq!(t.len(), 4);
        assert_eq!(t[0], PathCmd::MoveTo(Point::new(0, 0)));
        assert_eq!(t[3], PathCmd::Close);
    }
}
