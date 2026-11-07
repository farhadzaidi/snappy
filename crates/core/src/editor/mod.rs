//! Layout editing: every operation the editor canvas performs on a layout's zones.
//!
//! All operations take the current zones and return new ones; none mutate in place.
//! Results never contain overlaps or zones below the minimum size, except that drafts
//! loaded with overlaps stay as they are until the user repairs them.

mod creating;
mod guides;
mod handles;
mod moving;
#[cfg(test)]
mod properties;
mod repair;
mod seams;

pub use creating::{add_rect, draw, duplicate_rect, split};
pub use guides::{GUIDE_FRACTIONS, guides, snap, snapped_move};
pub use handles::{ResizeHandle, handle_at, handle_center, resize_rect};
pub use moving::move_zone;
pub use repair::{ALIGN_TOLERANCE, align_edges};
pub use seams::{Divider, dividers, move_edge, resize_zone, seam_members};

use crate::layout::{UnitRect, Zone};

/// Values closer than this are the same edge.
pub(crate) const EPSILON: f64 = 1e-7;

/// The axis an edge moves along: `X` for left and right edges, `Y` for top and bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    X,
    Y,
}

/// Which edge of a zone along an axis: the left or top edge, or the right or bottom edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Start,
    End,
}

impl Side {
    pub fn opposite(self) -> Side {
        match self {
            Side::Start => Side::End,
            Side::End => Side::Start,
        }
    }
}

/// A closed interval along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    pub start: f64,
    pub end: f64,
}

impl Span {
    /// Length of the shared part; negative when the spans are apart.
    pub fn overlap(self, other: Span) -> f64 {
        self.end.min(other.end) - self.start.max(other.start)
    }

    /// True when the spans share more than a point.
    pub fn shares_length(self, other: Span) -> bool {
        self.overlap(other) > EPSILON
    }
}

impl UnitRect {
    pub(crate) fn edge(self, axis: Axis, side: Side) -> f64 {
        match (axis, side) {
            (Axis::X, Side::Start) => self.x,
            (Axis::X, Side::End) => self.right(),
            (Axis::Y, Side::Start) => self.y,
            (Axis::Y, Side::End) => self.bottom(),
        }
    }

    /// The rectangle's extent along `axis`.
    pub(crate) fn span(self, axis: Axis) -> Span {
        Span {
            start: self.edge(axis, Side::Start),
            end: self.edge(axis, Side::End),
        }
    }

    /// The rectangle's extent across `axis`, the direction its edges run along.
    pub(crate) fn cross_span(self, axis: Axis) -> Span {
        self.span(axis.cross())
    }

    /// Move one edge while keeping the other fixed.
    pub(crate) fn set_edge(&mut self, axis: Axis, side: Side, value: f64) {
        match (axis, side) {
            (Axis::X, Side::Start) => {
                self.width += self.x - value;
                self.x = value;
            }
            (Axis::X, Side::End) => self.width = value - self.x,
            (Axis::Y, Side::Start) => {
                self.height += self.y - value;
                self.y = value;
            }
            (Axis::Y, Side::End) => self.height = value - self.y,
        }
    }

    pub(crate) fn overlaps(self, other: UnitRect) -> bool {
        self.span(Axis::X).shares_length(other.span(Axis::X))
            && self.span(Axis::Y).shares_length(other.span(Axis::Y))
    }
}

impl Axis {
    pub fn cross(self) -> Axis {
        match self {
            Axis::X => Axis::Y,
            Axis::Y => Axis::X,
        }
    }
}

/// True when no two zones overlap.
pub fn is_separated(zones: &[Zone]) -> bool {
    zones
        .iter()
        .enumerate()
        .all(|(i, zone)| zones[i + 1..].iter().all(|other| !zone.rect.overlaps(other.rect)))
}

/// True when `rect` overlaps none of `zones`, ignoring the zone with `except` as its ID.
pub(crate) fn is_free(rect: UnitRect, zones: &[Zone], except: Option<&str>) -> bool {
    zones
        .iter()
        .filter(|zone| Some(zone.id.as_str()) != except)
        .all(|zone| !rect.overlaps(zone.rect))
}

/// Every zone valid and none overlapping: the editor's invariant.
pub(crate) fn is_sound(zones: &[Zone]) -> bool {
    zones.iter().all(|zone| zone.rect.is_valid()) && is_separated(zones)
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::layout::{UnitRect, Zone};

    pub fn zone(id: &str, x: f64, y: f64, width: f64, height: f64) -> Zone {
        Zone::new(id, UnitRect::new(x, y, width, height))
    }

    pub fn rect(zones: &[Zone], id: &str) -> UnitRect {
        zones
            .iter()
            .find(|zone| zone.id == id)
            .map(|zone| zone.rect)
            .unwrap_or_default()
    }

    pub fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    pub fn grid() -> Vec<Zone> {
        vec![
            zone("a", 0.0, 0.0, 0.5, 0.5),
            zone("b", 0.5, 0.0, 0.5, 0.5),
            zone("c", 0.0, 0.5, 0.5, 0.5),
            zone("d", 0.5, 0.5, 0.5, 0.5),
        ]
    }
}
