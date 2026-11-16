use serde::{Deserialize, Serialize};

/// A point in global desktop coordinates: origin at the primary display's top-left, y down.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

/// A top-left-origin rectangle in the platform's window units.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

impl Rect {
    pub const ZERO: Rect = Rect::new(0.0, 0.0, 0.0, 0.0);

    pub const fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    pub fn from_origin_size(origin: Point, size: Size) -> Self {
        Self::new(origin.x, origin.y, size.width, size.height)
    }

    pub fn origin(self) -> Point {
        Point::new(self.x, self.y)
    }

    pub fn size(self) -> Size {
        Size::new(self.width, self.height)
    }

    pub fn min_x(self) -> f64 {
        self.x
    }

    pub fn min_y(self) -> f64 {
        self.y
    }

    pub fn max_x(self) -> f64 {
        self.x + self.width
    }

    pub fn max_y(self) -> f64 {
        self.y + self.height
    }

    pub fn mid_x(self) -> f64 {
        self.x + self.width / 2.0
    }

    pub fn mid_y(self) -> f64 {
        self.y + self.height / 2.0
    }

    pub fn center(self) -> Point {
        Point::new(self.mid_x(), self.mid_y())
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.width.is_finite() && self.height.is_finite()
    }

    /// Finite with at least `minimum` width and height.
    pub fn is_usable(self, minimum: f64) -> bool {
        self.is_finite() && self.width >= minimum && self.height >= minimum
    }

    pub fn area(self) -> f64 {
        self.width * self.height
    }

    /// Half-open containment: the max edges are outside, so adjacent rects never share a point.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min_x() && point.x < self.max_x() && point.y >= self.min_y() && point.y < self.max_y()
    }

    pub fn contains_rect(self, other: Rect) -> bool {
        other.min_x() >= self.min_x()
            && other.max_x() <= self.max_x()
            && other.min_y() >= self.min_y()
            && other.max_y() <= self.max_y()
    }

    /// The overlapping region, or `None` when the rectangles are apart. Touching edges
    /// produce a zero-area rectangle.
    pub fn intersection(self, other: Rect) -> Option<Rect> {
        let x = self.min_x().max(other.min_x());
        let y = self.min_y().max(other.min_y());
        let right = self.max_x().min(other.max_x());
        let bottom = self.max_y().min(other.max_y());
        (right >= x && bottom >= y).then(|| Rect::new(x, y, right - x, bottom - y))
    }

    pub fn intersection_area(self, other: Rect) -> f64 {
        self.intersection(other).map_or(0.0, Rect::area)
    }

    pub fn inset(self, dx: f64, dy: f64) -> Rect {
        Rect::new(
            self.x + dx,
            self.y + dy,
            self.width - 2.0 * dx,
            self.height - 2.0 * dy,
        )
    }

    pub fn with_origin(self, origin: Point) -> Rect {
        Rect::new(origin.x, origin.y, self.width, self.height)
    }

    /// Signed distance from `point` to the edge of this rectangle with its corners rounded
    /// by `radius`; negative inside. Rasterizers turn it into antialiased coverage.
    pub fn rounded_distance(self, point: Point, radius: f64) -> f64 {
        let half_w = self.width / 2.0;
        let half_h = self.height / 2.0;
        let radius = radius.min(half_w).min(half_h).max(0.0);
        let dx = (point.x - self.mid_x()).abs() - (half_w - radius);
        let dy = (point.y - self.mid_y()).abs() - (half_h - radius);
        let outside = dx.max(0.0).hypot(dy.max(0.0));
        outside + dx.max(dy).min(0.0) - radius
    }
}

/// Index of the area with the largest overlap with `rect`. The first entry wins ties.
pub fn largest_overlap(rect: Rect, areas: &[Rect]) -> Option<usize> {
    let mut best: Option<(usize, f64)> = None;
    for (index, area) in areas.iter().enumerate() {
        let overlap = rect.intersection_area(*area);
        if best.is_none_or(|(_, value)| overlap > value) {
            best = Some((index, overlap));
        }
    }
    best.map(|(index, _)| index)
}

/// Rounds half away from zero, the rule zone frames use.
pub fn round(value: f64) -> f64 {
    value.round()
}

pub fn distance(dx: f64, dy: f64) -> f64 {
    dx.hypot(dy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_distance_is_negative_inside_and_measured_from_the_corner_arc() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(rect.rounded_distance(Point::new(5.0, 2.0), 2.0), -2.0);
        assert_eq!(rect.rounded_distance(Point::new(5.0, 13.0), 2.0), 3.0);
        let past_corner = rect.rounded_distance(Point::new(-1.0, -1.0), 2.0);
        assert!((past_corner - (18f64.sqrt() - 2.0)).abs() < 1e-9);
    }

    #[test]
    fn containment_is_half_open() {
        let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(rect.contains(Point::new(0.0, 0.0)));
        assert!(!rect.contains(Point::new(10.0, 5.0)));
    }

    #[test]
    fn touching_rects_intersect_with_zero_area() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(10.0, 0.0, 10.0, 10.0);
        assert_eq!(a.intersection_area(b), 0.0);
        assert!(a.intersection(Rect::new(11.0, 0.0, 1.0, 1.0)).is_none());
    }

    #[test]
    fn largest_overlap_prefers_first_on_ties() {
        let areas = [Rect::new(0.0, 0.0, 10.0, 10.0), Rect::new(10.0, 0.0, 10.0, 10.0)];
        assert_eq!(largest_overlap(Rect::new(8.0, 0.0, 10.0, 10.0), &areas), Some(1));
        assert_eq!(largest_overlap(Rect::new(50.0, 0.0, 1.0, 1.0), &areas), Some(0));
    }
}
