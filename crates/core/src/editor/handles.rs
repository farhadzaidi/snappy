use super::{Axis, Side};
use crate::geometry::{Point, Rect, Size, distance};
use crate::layout::{MINIMUM_ZONE_SIZE, UnitRect};

/// How far handles sit inside the zone's corners and edges, at most.
const HANDLE_INSET: f64 = 7.0;
/// Presses this close to a handle grab it.
const HANDLE_HIT_RADIUS: f64 = 14.0;
/// Presses this close to an edge grab that edge.
const EDGE_BAND: f64 = 9.0;
/// The center region is reserved for moving, however small the zone is.
const MOVE_ONLY_INSET: f64 = 28.0;
const MOVE_ONLY_FRACTION: f64 = 0.3;

/// The eight resize handles of a selected zone, by compass direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResizeHandle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

impl ResizeHandle {
    pub const ALL: [ResizeHandle; 8] = [
        ResizeHandle::NorthWest,
        ResizeHandle::North,
        ResizeHandle::NorthEast,
        ResizeHandle::East,
        ResizeHandle::SouthEast,
        ResizeHandle::South,
        ResizeHandle::SouthWest,
        ResizeHandle::West,
    ];

    /// The edge this handle moves along `axis`, if any.
    pub fn side(self, axis: Axis) -> Option<Side> {
        use ResizeHandle::*;
        match axis {
            Axis::X => match self {
                NorthWest | West | SouthWest => Some(Side::Start),
                NorthEast | East | SouthEast => Some(Side::End),
                North | South => None,
            },
            Axis::Y => match self {
                NorthWest | North | NorthEast => Some(Side::Start),
                SouthWest | South | SouthEast => Some(Side::End),
                West | East => None,
            },
        }
    }
}

/// Where a handle sits on a zone drawn at `size`, relative to its top-left corner.
pub fn handle_center(handle: ResizeHandle, size: Size) -> Point {
    let position = |side: Option<Side>, length: f64| {
        let inset = HANDLE_INSET.min(length / 4.0);
        match side {
            Some(Side::Start) => inset,
            None => length / 2.0,
            Some(Side::End) => length - inset,
        }
    };
    Point::new(
        position(handle.side(Axis::X), size.width),
        position(handle.side(Axis::Y), size.height),
    )
}

/// The handle under a press at `point` on a zone drawn at `size`, or `None` to move it.
pub fn handle_at(point: Point, size: Size) -> Option<ResizeHandle> {
    if !point.is_finite()
        || !(size.width.is_finite() && size.height.is_finite())
        || size.width <= 0.0
        || size.height <= 0.0
    {
        return None;
    }
    let body = Rect::new(0.0, 0.0, size.width, size.height).inset(
        MOVE_ONLY_INSET.min(size.width * MOVE_ONLY_FRACTION),
        MOVE_ONLY_INSET.min(size.height * MOVE_ONLY_FRACTION),
    );
    if body.contains(point) {
        return None;
    }

    let distance_to = |handle: ResizeHandle| {
        let center = handle_center(handle, size);
        distance(point.x - center.x, point.y - center.y)
    };
    let nearest = ResizeHandle::ALL
        .into_iter()
        .min_by(|a, b| distance_to(*a).total_cmp(&distance_to(*b)))?;
    if distance_to(nearest) <= HANDLE_HIT_RADIUS {
        return Some(nearest);
    }

    if point.x <= EDGE_BAND {
        Some(ResizeHandle::West)
    } else if point.x >= size.width - EDGE_BAND {
        Some(ResizeHandle::East)
    } else if point.y <= EDGE_BAND {
        Some(ResizeHandle::North)
    } else if point.y >= size.height - EDGE_BAND {
        Some(ResizeHandle::South)
    } else {
        None
    }
}

/// The rectangle a handle drag of (dx, dy) asks for, kept on the canvas and at least
/// the minimum size, with the opposite edges fixed.
pub fn resize_rect(rect: UnitRect, dx: f64, dy: f64, handle: ResizeHandle) -> UnitRect {
    if !rect.is_valid() || !dx.is_finite() || !dy.is_finite() {
        return rect;
    }
    let mut result = rect;
    for (axis, delta) in [(Axis::X, dx), (Axis::Y, dy)] {
        let (start, end) = (rect.edge(axis, Side::Start), rect.edge(axis, Side::End));
        match handle.side(axis) {
            Some(Side::Start) => result.set_edge(
                axis,
                Side::Start,
                (start + delta).clamp(0.0, end - MINIMUM_ZONE_SIZE),
            ),
            Some(Side::End) => result.set_edge(
                axis,
                Side::End,
                (end + delta).clamp(start + MINIMUM_ZONE_SIZE, 1.0),
            ),
            None => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn center_presses_always_move() {
        for size in [
            Size::new(20.0, 20.0),
            Size::new(80.0, 60.0),
            Size::new(600.0, 400.0),
        ] {
            let center = Point::new(size.width / 2.0, size.height / 2.0);
            assert_eq!(handle_at(center, size), None);
        }
    }

    #[test]
    fn corners_and_edges_resize() {
        let size = Size::new(300.0, 200.0);
        assert_eq!(
            handle_at(Point::new(3.0, 3.0), size),
            Some(ResizeHandle::NorthWest)
        );
        assert_eq!(
            handle_at(Point::new(298.0, 100.0), size),
            Some(ResizeHandle::East)
        );
        assert_eq!(
            handle_at(Point::new(60.0, 196.0), size),
            Some(ResizeHandle::South)
        );
    }

    #[test]
    fn resize_keeps_the_opposite_edge_and_canvas() {
        let rect = UnitRect::new(0.2, 0.2, 0.3, 0.3);
        let grown = resize_rect(rect, -0.5, 0.0, ResizeHandle::West);
        assert_eq!(grown.x, 0.0);
        assert!((grown.right() - 0.5).abs() < 1e-9);
        let shrunk = resize_rect(rect, 0.0, -0.5, ResizeHandle::South);
        assert!((shrunk.height - MINIMUM_ZONE_SIZE).abs() < 1e-9);
    }
}
