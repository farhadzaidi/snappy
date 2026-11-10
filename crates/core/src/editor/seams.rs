use std::collections::BTreeSet;

use super::{Axis, EPSILON, ResizeHandle, Side, Span, is_separated, is_sound};
use crate::layout::{UnitRect, Zone};

/// How many halvings the clamp search uses before settling on the furthest valid edge.
const CLAMP_STEPS: usize = 36;

/// The zones that move together when one zone's edge is dragged.
///
/// An edge no other zone faces moves alone. Otherwise every zone with an edge on the
/// same line, connected along it (including T-junctions), moves with it.
pub fn seam_members(zones: &[Zone], id: &str, axis: Axis, side: Side) -> BTreeSet<String> {
    let Some(selected) = zones.iter().find(|zone| zone.id == id) else {
        return BTreeSet::new();
    };
    let line = selected.rect.edge(axis, side);
    let mut members = BTreeSet::from([id.to_owned()]);

    let faced = zones.iter().any(|other| {
        other.id != id
            && on_line(other.rect.edge(axis, side.opposite()), line)
            && other
                .rect
                .cross_span(axis)
                .shares_length(selected.rect.cross_span(axis))
    });
    if !faced {
        return members;
    }

    let mut reach = vec![selected.rect.cross_span(axis)];
    while let Some(next) = zones.iter().find(|zone| {
        !members.contains(&zone.id)
            && touches_line(zone.rect, axis, line)
            && connects(&reach, zone.rect.cross_span(axis))
    }) {
        members.insert(next.id.clone());
        reach.push(next.rect.cross_span(axis));
    }
    members
}

/// Drag one edge of a zone to `to`, carrying its seam and trimming the neighbors it
/// grows into. A drag past what's allowed stops at the furthest valid position.
pub fn move_edge(zones: &[Zone], id: &str, axis: Axis, side: Side, to: f64) -> Vec<Zone> {
    let Some(zone) = zones.iter().find(|zone| zone.id == id) else {
        return zones.to_vec();
    };
    if !to.is_finite() || !is_separated(zones) {
        return zones.to_vec();
    }
    let from = zone.rect.edge(axis, side);
    let members = seam_members(zones, id, axis, side);
    let attempt =
        |line: f64| Some(place_seam(zones, &members, axis, from, line)).filter(|zones| is_sound(zones));

    attempt(to.clamp(0.0, 1.0)).unwrap_or_else(|| furthest_valid(zones, from, to.clamp(0.0, 1.0), attempt))
}

/// Resize a zone to `target` by dragging the edges named by `handle`.
pub fn resize_zone(zones: &[Zone], id: &str, target: UnitRect, handle: ResizeHandle) -> Vec<Zone> {
    let Some(zone) = zones.iter().find(|zone| zone.id == id) else {
        return zones.to_vec();
    };
    if !zone.rect.is_valid() {
        return zones.to_vec();
    }
    let mut result = zones.to_vec();
    for axis in [Axis::X, Axis::Y] {
        if let Some(side) = handle.side(axis) {
            result = move_edge(&result, id, axis, side, target.edge(axis, side));
        }
    }
    result
}

/// Move every seam member's edge from `from` to `line`, then trim the zones they grew into.
fn place_seam(zones: &[Zone], members: &BTreeSet<String>, axis: Axis, from: f64, line: f64) -> Vec<Zone> {
    let mut result = zones.to_vec();
    for zone in result.iter_mut().filter(|zone| members.contains(&zone.id)) {
        let side = if on_line(zone.rect.edge(axis, Side::Start), from) {
            Side::Start
        } else {
            Side::End
        };
        zone.rect.set_edge(axis, side, line);
    }

    for (before, after) in zones
        .iter()
        .zip(result.clone())
        .filter(|(zone, _)| members.contains(&zone.id))
    {
        let grew_end = after.rect.edge(axis, Side::End) > before.rect.edge(axis, Side::End) + EPSILON;
        let grew_start = after.rect.edge(axis, Side::Start) < before.rect.edge(axis, Side::Start) - EPSILON;
        let across = before.rect.cross_span(axis);

        for neighbor in result.iter_mut().filter(|zone| !members.contains(&zone.id)) {
            if !neighbor.rect.cross_span(axis).shares_length(across) {
                continue;
            }
            let start = neighbor.rect.edge(axis, Side::Start);
            let end = neighbor.rect.edge(axis, Side::End);
            if grew_end && start >= before.rect.edge(axis, Side::End) - EPSILON && start < line {
                neighbor.rect.set_edge(axis, Side::Start, line);
            }
            if grew_start && end <= before.rect.edge(axis, Side::Start) + EPSILON && end > line {
                neighbor.rect.set_edge(axis, Side::End, line);
            }
        }
    }
    result
}

/// Binary-search the furthest line between `from` and `to` that `attempt` accepts.
fn furthest_valid(
    zones: &[Zone],
    from: f64,
    to: f64,
    attempt: impl Fn(f64) -> Option<Vec<Zone>>,
) -> Vec<Zone> {
    let (mut reachable, mut blocked) = (0.0, 1.0);
    let mut best = zones.to_vec();
    for _ in 0..CLAMP_STEPS {
        let fraction = (reachable + blocked) / 2.0;
        match attempt(from + (to - from) * fraction) {
            Some(result) => {
                reachable = fraction;
                best = result;
            }
            None => blocked = fraction,
        }
    }
    best
}

fn on_line(value: f64, line: f64) -> bool {
    (value - line).abs() < EPSILON
}

fn touches_line(rect: UnitRect, axis: Axis, line: f64) -> bool {
    on_line(rect.edge(axis, Side::Start), line) || on_line(rect.edge(axis, Side::End), line)
}

fn connects(reach: &[Span], span: Span) -> bool {
    reach.iter().any(|part| part.overlap(span) >= -EPSILON)
}

/// A shared edge between zones, dragged to resize the zones on both sides.
#[derive(Debug, Clone, PartialEq)]
pub struct Divider {
    /// A zone whose end edge lies on the divider; dragging moves that zone's seam.
    pub zone_id: String,
    pub axis: Axis,
    pub line: f64,
    /// Extent along the divider.
    pub span: (f64, f64),
}

impl Divider {
    pub fn id(&self) -> String {
        format!("{:?}:{}:{}", self.axis, self.zone_id, self.line)
    }
}

/// Every shared edge between zones, merged where segments meet. Gaps never form one.
pub fn dividers(zones: &[Zone]) -> Vec<Divider> {
    let mut segments: Vec<Divider> = Vec::new();
    for axis in [Axis::X, Axis::Y] {
        for zone in zones {
            let line = zone.rect.edge(axis, Side::End);
            for neighbor in zones.iter().filter(|other| other.id != zone.id) {
                let shared = zone.rect.cross_span(axis).overlap(neighbor.rect.cross_span(axis));
                if on_line(neighbor.rect.edge(axis, Side::Start), line) && shared > EPSILON {
                    let across_start = zone
                        .rect
                        .cross_span(axis)
                        .start
                        .max(neighbor.rect.cross_span(axis).start);
                    segments.push(Divider {
                        zone_id: zone.id.clone(),
                        axis,
                        line,
                        span: (across_start, across_start + shared),
                    });
                }
            }
        }
    }
    segments.sort_by(|a, b| a.span.0.total_cmp(&b.span.0));

    let mut merged: Vec<Divider> = Vec::new();
    for segment in segments {
        let joins = |existing: &&mut Divider| {
            existing.axis == segment.axis
                && on_line(existing.line, segment.line)
                && segment.span.0 <= existing.span.1 + EPSILON
                && segment.span.1 >= existing.span.0 - EPSILON
        };
        match merged.iter_mut().find(joins) {
            Some(existing) => {
                existing.span = (
                    existing.span.0.min(segment.span.0),
                    existing.span.1.max(segment.span.1),
                );
            }
            None => merged.push(segment),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{grid, near, rect, zone};
    use super::*;
    use crate::layout::MINIMUM_ZONE_SIZE;

    #[test]
    fn connected_seams_resize_both_rows_and_keep_far_edges() {
        let moved = move_edge(&grid(), "a", Axis::X, Side::End, 0.6);
        near(rect(&moved, "a").right(), 0.6);
        near(rect(&moved, "c").right(), 0.6);
        near(rect(&moved, "b").x, 0.6);
        near(rect(&moved, "b").right(), 1.0);
        assert!(is_sound(&moved));
    }

    #[test]
    fn growth_stops_at_the_neighbor_minimum() {
        let zones = vec![zone("a", 0.0, 0.0, 0.5, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)];
        let moved = move_edge(&zones, "a", Axis::X, Side::End, 0.99);
        near(rect(&moved, "b").width, MINIMUM_ZONE_SIZE);
        assert!(is_sound(&moved));
    }

    #[test]
    fn unshared_edges_move_alone() {
        let members = seam_members(&grid(), "a", Axis::Y, Side::Start);
        assert_eq!(members, BTreeSet::from(["a".to_owned()]));
        let members = seam_members(&grid(), "a", Axis::X, Side::End);
        assert_eq!(members.len(), 4, "both sides of the middle seam move together");
    }

    #[test]
    fn growing_across_a_gap_trims_the_neighbor() {
        let zones = vec![zone("a", 0.0, 0.0, 0.4, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)];
        let moved = move_edge(&zones, "a", Axis::X, Side::End, 0.7);
        near(rect(&moved, "a").right(), 0.7);
        near(rect(&moved, "b").x, 0.7);
    }

    #[test]
    fn resize_uses_the_handle_sides() {
        let zones = vec![zone("a", 0.2, 0.2, 0.3, 0.3)];
        let moved = resize_zone(
            &zones,
            "a",
            UnitRect::new(0.1, 0.2, 0.4, 0.5),
            ResizeHandle::SouthWest,
        );
        let a = rect(&moved, "a");
        near(a.x, 0.1);
        near(a.bottom(), 0.7);
        near(a.right(), 0.5);
    }

    #[test]
    fn dividers_merge_and_skip_gaps() {
        let found = dividers(&grid());
        assert_eq!(found.len(), 2);
        let vertical = found.iter().find(|d| d.axis == Axis::X).unwrap();
        assert_eq!(vertical.span, (0.0, 1.0));
        assert!(dividers(&[zone("a", 0.0, 0.0, 0.4, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)]).is_empty());
    }
}
