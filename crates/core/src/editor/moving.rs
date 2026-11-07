use super::{Axis, Side, is_free, is_separated};
use crate::geometry::Point;
use crate::layout::{UnitRect, Zone};

/// Move a zone toward `target` (its new top-left corner).
///
/// The zone slides until it touches another zone; it never pushes, overlaps, or
/// jumps past one, and it stays on the canvas. A draft that already has overlaps only
/// lets the zone land where it overlaps nothing.
pub fn move_zone(zones: &[Zone], id: &str, target: Point) -> Vec<Zone> {
    let Some(index) = zones.iter().position(|zone| zone.id == id) else {
        return zones.to_vec();
    };
    if !target.is_finite() {
        return zones.to_vec();
    }

    let rect = zones[index].rect;
    let wanted = Point::new(
        target.x.clamp(0.0, 1.0 - rect.width),
        target.y.clamp(0.0, 1.0 - rect.height),
    );
    let others: Vec<UnitRect> = zones
        .iter()
        .filter(|zone| zone.id != id)
        .map(|zone| zone.rect)
        .collect();

    let moved = if is_separated(zones) {
        slide(rect, &others, wanted)
    } else {
        let jumped = UnitRect::new(wanted.x, wanted.y, rect.width, rect.height);
        if !is_free(jumped, zones, Some(id)) {
            return zones.to_vec();
        }
        jumped
    };

    let mut result = zones.to_vec();
    result[index].rect = moved;
    result
}

/// Slide horizontally first, then vertically, each stopping at the first obstacle.
fn slide(rect: UnitRect, others: &[UnitRect], wanted: Point) -> UnitRect {
    let mut moved = rect;
    moved.x += allowed_travel(moved, others, Axis::X, wanted.x - moved.x);
    moved.y += allowed_travel(moved, others, Axis::Y, wanted.y - moved.y);
    moved
}

/// How far `rect` can travel along `axis` toward `distance` before touching a zone
/// that shares its extent across the axis.
fn allowed_travel(rect: UnitRect, others: &[UnitRect], axis: Axis, distance: f64) -> f64 {
    let across = rect.cross_span(axis);
    others
        .iter()
        .filter(|other| other.cross_span(axis).shares_length(across))
        .fold(distance, |travel, other| {
            if travel > 0.0 && other.edge(axis, Side::Start) >= rect.edge(axis, Side::End) - super::EPSILON {
                travel.min((other.edge(axis, Side::Start) - rect.edge(axis, Side::End)).max(0.0))
            } else if travel < 0.0
                && other.edge(axis, Side::End) <= rect.edge(axis, Side::Start) + super::EPSILON
            {
                travel.max((other.edge(axis, Side::End) - rect.edge(axis, Side::Start)).min(0.0))
            } else {
                travel
            }
        })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{near, rect, zone};
    use super::*;

    #[test]
    fn moves_stop_at_occupied_space_without_tunneling() {
        let zones = vec![zone("a", 0.0, 0.0, 0.3, 0.3), zone("b", 0.5, 0.0, 0.3, 0.3)];
        near(rect(&move_zone(&zones, "a", Point::new(0.9, 0.0)), "a").x, 0.2);
        near(rect(&move_zone(&zones, "a", Point::new(0.0, 0.6)), "a").y, 0.6);
    }

    #[test]
    fn moves_stay_on_the_canvas() {
        let zones = vec![zone("a", 0.2, 0.2, 0.3, 0.3)];
        let moved = rect(&move_zone(&zones, "a", Point::new(-1.0, 5.0)), "a");
        near(moved.x, 0.0);
        near(moved.y, 0.7);
    }

    #[test]
    fn overlapping_drafts_only_jump_to_free_spots() {
        let zones = vec![zone("a", 0.0, 0.0, 0.5, 0.5), zone("b", 0.4, 0.0, 0.5, 0.5)];
        assert_eq!(move_zone(&zones, "a", Point::new(0.1, 0.0)), zones);
        near(rect(&move_zone(&zones, "a", Point::new(0.0, 0.5)), "a").y, 0.5);
    }
}
