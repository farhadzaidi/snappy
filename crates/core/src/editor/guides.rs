use super::{Axis, Side};
use crate::geometry::Size;
use crate::layout::{UnitRect, Zone};

/// Screen fractions every edit can snap to.
pub const GUIDE_FRACTIONS: [f64; 7] = [0.0, 0.25, 1.0 / 3.0, 0.5, 2.0 / 3.0, 0.75, 1.0];

/// Snap targets along an axis: the guide fractions plus every other zone's edges, and
/// the zones' centers when `centers` is set.
pub fn guides(zones: &[Zone], excluding: Option<&str>, axis: Axis, centers: bool) -> Vec<f64> {
    let mut targets = GUIDE_FRACTIONS.to_vec();
    for zone in zones.iter().filter(|zone| Some(zone.id.as_str()) != excluding) {
        let span = zone.rect.span(axis);
        targets.extend([span.start, span.end]);
        if centers {
            targets.push((span.start + span.end) / 2.0);
        }
    }
    targets
}

/// The target nearest `value` if it's within `tolerance`, otherwise `value` itself.
pub fn snap(value: f64, targets: &[f64], tolerance: f64) -> f64 {
    if !value.is_finite() || tolerance.is_nan() || tolerance < 0.0 {
        return value;
    }
    nearest_offset(targets.iter().map(|target| target - value), tolerance)
        .map_or(value, |offset| value + offset)
}

/// Shift a moving zone so its nearest edge or center lands on a guide, per axis.
pub fn snapped_move(rect: UnitRect, zones: &[Zone], id: &str, tolerance: Size) -> UnitRect {
    let mut result = rect;
    for (axis, limit) in [(Axis::X, tolerance.width), (Axis::Y, tolerance.height)] {
        let span = rect.span(axis);
        let anchors = [span.start, (span.start + span.end) / 2.0, span.end];
        let targets = guides(zones, Some(id), axis, true);
        let offsets = anchors
            .into_iter()
            .flat_map(|anchor| targets.iter().map(move |target| target - anchor));
        if let Some(offset) = nearest_offset(offsets, limit) {
            let length = span.end - span.start;
            result.set_edge(axis, Side::Start, span.start + offset);
            result.set_edge(axis, Side::End, span.start + offset + length);
        }
    }
    result
}

/// The smallest finite offset within `limit`, the earliest on ties.
fn nearest_offset(offsets: impl Iterator<Item = f64>, limit: f64) -> Option<f64> {
    offsets
        .filter(|offset| offset.is_finite() && offset.abs() <= limit)
        .fold(None, |best: Option<f64>, offset| match best {
            Some(current) if current.abs() <= offset.abs() => Some(current),
            _ => Some(offset),
        })
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{near, zone};
    use super::*;

    #[test]
    fn snap_uses_the_nearest_target_within_tolerance() {
        near(snap(0.496, &[0.0, 0.5, 1.0], 0.01), 0.5);
        near(snap(0.48, &[0.0, 0.5, 1.0], 0.01), 0.48);
    }

    #[test]
    fn moves_snap_edges_and_centers() {
        let zones = vec![zone("a", 0.0, 0.0, 0.4, 1.0)];
        let moved = snapped_move(
            UnitRect::new(0.402, 0.0, 0.2, 1.0),
            &zones,
            "b",
            Size::new(0.01, 0.01),
        );
        near(moved.x, 0.4);
        near(moved.width, 0.2);
        let centered = snapped_move(
            UnitRect::new(0.395, 0.3, 0.2, 0.2),
            &[],
            "b",
            Size::new(0.01, 0.01),
        );
        near(centered.x + centered.width / 2.0, 0.5);
    }
}
