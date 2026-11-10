//! Canvas gestures composed from `snappy-core` editor operations, so the settings app
//! never computes zone geometry itself. Positions and tolerances are canvas fractions.

use serde::Serialize;
use snappy_core::editor::{
    Axis, ResizeHandle, Side, draw, guides, move_edge, move_zone, resize_rect, resize_zone, seam_members,
    snap, snapped_move, split,
};
use snappy_core::geometry::{Point, Size};
use snappy_core::layout::{MINIMUM_ZONE_SIZE, UnitRect, Zone};

use crate::wire::Field;

/// Guide values closer than this to an edge count as lined up with it.
const ALIGNED: f64 = 1e-7;

/// Guide lines to draw while a gesture is under way.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Guides {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
}

/// The zones after one step of a gesture, and the guides it lines up with.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Edit {
    pub zones: Vec<Zone>,
    pub guides: Guides,
}

/// A zone being drawn on empty canvas.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Drawn {
    pub rect: Option<UnitRect>,
    pub guides: Guides,
}

/// Slide a zone by `delta` from where it started, snapping its edges and center.
pub fn drag_move(zones: &[Zone], id: &str, delta: Point, tolerance: Size) -> Edit {
    let Some(rect) = rect_of(zones, id) else {
        return Edit::unchanged(zones);
    };
    let requested = UnitRect::new(rect.x + delta.x, rect.y + delta.y, rect.width, rect.height);
    let target = snapped_move(requested, zones, id, tolerance);
    let moved = move_zone(zones, id, Point::new(target.x, target.y));

    let guides = rect_of(&moved, id).map_or_else(Guides::default, |result| Guides {
        x: aligned(
            &anchors(result, Axis::X),
            &guides(&moved, Some(id), Axis::X, true),
        ),
        y: aligned(
            &anchors(result, Axis::Y),
            &guides(&moved, Some(id), Axis::Y, true),
        ),
    });
    Edit { zones: moved, guides }
}

/// Resize a selected zone by dragging `handle` by `delta`, snapping the moving edges.
pub fn drag_resize(zones: &[Zone], id: &str, handle: ResizeHandle, delta: Point, tolerance: Size) -> Edit {
    let Some(rect) = rect_of(zones, id) else {
        return Edit::unchanged(zones);
    };
    let mut target = resize_rect(rect, delta.x, delta.y, handle);
    for axis in [Axis::X, Axis::Y] {
        if let Some(side) = handle.side(axis) {
            let targets = guides(zones, Some(id), axis, false);
            let snapped = snap(edge(target, axis, side), &targets, along(tolerance, axis));
            target = with_edge(target, axis, side, snapped);
        }
    }
    let resized = resize_zone(zones, id, target, handle);

    let moving_edges = |result: UnitRect, axis: Axis| -> Vec<f64> {
        let Some(side) = handle.side(axis) else {
            return Vec::new();
        };
        aligned(
            &[edge(result, axis, side)],
            &guides(&resized, Some(id), axis, false),
        )
    };
    let guides = rect_of(&resized, id).map_or_else(Guides::default, |result| Guides {
        x: moving_edges(result, Axis::X),
        y: moving_edges(result, Axis::Y),
    });
    Edit {
        zones: resized,
        guides,
    }
}

/// Drag the divider on the end edge of `zone_id` by `delta` from `line`.
pub fn drag_divider(
    zones: &[Zone],
    zone_id: &str,
    axis: Axis,
    line: f64,
    delta: f64,
    tolerance: f64,
) -> Edit {
    let members = seam_members(zones, zone_id, axis, Side::End);
    let others: Vec<Zone> = zones
        .iter()
        .filter(|zone| !members.contains(&zone.id))
        .cloned()
        .collect();
    let targets = guides(&others, None, axis, false);
    let moved = move_edge(
        zones,
        zone_id,
        axis,
        Side::End,
        snap(line + delta, &targets, tolerance),
    );

    let lined_up = rect_of(&moved, zone_id)
        .map(|result| aligned(&[edge(result, axis, Side::End)], &targets))
        .unwrap_or_default();
    let guides = match axis {
        Axis::X => Guides {
            x: lined_up,
            y: Vec::new(),
        },
        Axis::Y => Guides {
            x: Vec::new(),
            y: lined_up,
        },
    };
    Edit { zones: moved, guides }
}

/// The zone a drag from `from` to `to` on empty canvas draws, with both ends snapped.
pub fn draw_zone(zones: &[Zone], from: Point, to: Point, tolerance: Size) -> Drawn {
    let snap_point = |point: Point| {
        Point::new(
            snap(point.x, &guides(zones, None, Axis::X, false), tolerance.width),
            snap(point.y, &guides(zones, None, Axis::Y, false), tolerance.height),
        )
    };
    let rect = draw(zones, snap_point(from), snap_point(to));

    let guides = rect.map_or_else(Guides::default, |rect| Guides {
        x: aligned(&[rect.x, rect.right()], &guides(zones, None, Axis::X, false)),
        y: aligned(&[rect.y, rect.bottom()], &guides(zones, None, Axis::Y, false)),
    });
    Drawn { rect, guides }
}

/// Where a cut through zone `id` at `at` lands along `axis`, snapped to guides.
pub fn cut_line(zones: &[Zone], id: &str, axis: Axis, at: f64, tolerance: f64) -> f64 {
    snap(at, &guides(zones, Some(id), axis, false), tolerance)
}

/// Cut zone `id` at `at`. The second part takes `new_id` and follows the first in zone order.
pub fn cut(zones: &[Zone], id: &str, axis: Axis, at: f64, tolerance: f64, new_id: &str) -> Option<Vec<Zone>> {
    let index = zones.iter().position(|zone| zone.id == id)?;
    let zone = &zones[index];
    let (start, end) = (
        edge(zone.rect, axis, Side::Start),
        edge(zone.rect, axis, Side::End),
    );
    let line = cut_line(zones, id, axis, at, tolerance);
    let (first, second) = split(zone, (line - start) / (end - start), axis, new_id)?;

    let mut result = zones.to_vec();
    result[index] = first;
    result.insert(index + 1, second);
    Some(result)
}

/// Apply a committed coordinate field: X and Y move the zone with the same
/// blocking as dragging; Width and Height resize from the right or bottom edge.
pub fn set_field(zones: &[Zone], id: &str, field: Field, value: f64) -> Vec<Zone> {
    let Some(rect) = rect_of(zones, id) else {
        return zones.to_vec();
    };
    if !value.is_finite() {
        return zones.to_vec();
    }
    match field {
        Field::X => move_zone(zones, id, Point::new(value, rect.y)),
        Field::Y => move_zone(zones, id, Point::new(rect.x, value)),
        Field::Width => {
            let target = UnitRect {
                width: value.max(MINIMUM_ZONE_SIZE),
                ..rect
            };
            resize_zone(zones, id, target, ResizeHandle::East)
        }
        Field::Height => {
            let target = UnitRect {
                height: value.max(MINIMUM_ZONE_SIZE),
                ..rect
            };
            resize_zone(zones, id, target, ResizeHandle::South)
        }
    }
}

impl Edit {
    fn unchanged(zones: &[Zone]) -> Self {
        Self {
            zones: zones.to_vec(),
            guides: Guides::default(),
        }
    }
}

fn rect_of(zones: &[Zone], id: &str) -> Option<UnitRect> {
    zones.iter().find(|zone| zone.id == id).map(|zone| zone.rect)
}

fn along(size: Size, axis: Axis) -> f64 {
    match axis {
        Axis::X => size.width,
        Axis::Y => size.height,
    }
}

fn edge(rect: UnitRect, axis: Axis, side: Side) -> f64 {
    match (axis, side) {
        (Axis::X, Side::Start) => rect.x,
        (Axis::X, Side::End) => rect.right(),
        (Axis::Y, Side::Start) => rect.y,
        (Axis::Y, Side::End) => rect.bottom(),
    }
}

/// `rect` with one edge moved to `value` and the opposite edge kept in place.
fn with_edge(rect: UnitRect, axis: Axis, side: Side, value: f64) -> UnitRect {
    let mut result = rect;
    match (axis, side) {
        (Axis::X, Side::Start) => {
            result.x = value;
            result.width = rect.right() - value;
        }
        (Axis::X, Side::End) => result.width = value - rect.x,
        (Axis::Y, Side::Start) => {
            result.y = value;
            result.height = rect.bottom() - value;
        }
        (Axis::Y, Side::End) => result.height = value - rect.y,
    }
    result
}

/// A moving zone's start edge, center, and end edge along `axis`.
fn anchors(rect: UnitRect, axis: Axis) -> [f64; 3] {
    let (start, end) = (edge(rect, axis, Side::Start), edge(rect, axis, Side::End));
    [start, (start + end) / 2.0, end]
}

/// The interior guides that one of `values` lies on, each listed once.
fn aligned(values: &[f64], targets: &[f64]) -> Vec<f64> {
    let mut lines: Vec<f64> = Vec::new();
    for &target in targets {
        let interior = target > ALIGNED && target < 1.0 - ALIGNED;
        let on_value = values.iter().any(|value| (value - target).abs() < ALIGNED);
        let listed = lines.iter().any(|line| (line - target).abs() < ALIGNED);
        if interior && on_value && !listed {
            lines.push(target);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(id: &str, x: f64, y: f64, width: f64, height: f64) -> Zone {
        Zone::new(id, UnitRect::new(x, y, width, height))
    }

    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    const TOLERANCE: Size = Size::new(0.01, 0.01);

    #[test]
    fn moves_match_core_and_report_guides() {
        let zones = vec![zone("a", 0.0, 0.0, 0.3, 0.3), zone("b", 0.5, 0.0, 0.3, 0.3)];
        let edit = drag_move(&zones, "a", Point::new(0.9, 0.0), TOLERANCE);
        assert_eq!(edit.zones, move_zone(&zones, "a", Point::new(0.7, 0.0)));
        near(edit.zones[0].rect.right(), 0.5);
        assert_eq!(edit.guides.x, vec![0.5]);
    }

    #[test]
    fn resize_snaps_the_moving_edge() {
        let zones = vec![zone("a", 0.1, 0.1, 0.3, 0.3)];
        let edit = drag_resize(&zones, "a", ResizeHandle::East, Point::new(0.098, 0.0), TOLERANCE);
        near(edit.zones[0].rect.right(), 0.5);
        near(edit.zones[0].rect.x, 0.1);
        assert_eq!(edit.guides.x, vec![0.5]);
        assert!(edit.guides.y.is_empty());
    }

    #[test]
    fn divider_drags_move_the_whole_seam() {
        let zones = vec![zone("a", 0.0, 0.0, 0.5, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)];
        let edit = drag_divider(&zones, "a", Axis::X, 0.5, 0.1, 0.01);
        assert_eq!(edit.zones, move_edge(&zones, "a", Axis::X, Side::End, 0.6));
        let snapped = drag_divider(&zones, "a", Axis::X, 0.5, 0.245, 0.01);
        near(snapped.zones[0].rect.right(), 0.75);
        assert_eq!(snapped.guides.x, vec![0.75]);
    }

    #[test]
    fn drawing_snaps_both_ends() {
        let drawn = draw_zone(&[], Point::new(0.003, 0.004), Point::new(0.498, 0.502), TOLERANCE);
        assert_eq!(drawn.rect, Some(UnitRect::new(0.0, 0.0, 0.5, 0.5)));
        assert_eq!(drawn.guides.x, vec![0.5]);
    }

    #[test]
    fn cuts_insert_the_second_part_after_the_first() {
        let zones = vec![zone("a", 0.0, 0.0, 1.0, 1.0)];
        let result = cut(&zones, "a", Axis::X, 0.333, 0.01, "c").unwrap();
        assert_eq!(result[0].id, "a");
        assert_eq!(result[1].id, "c");
        near(result[0].rect.width, 1.0 / 3.0);
        assert!(cut(&zones, "a", Axis::Y, 0.001, 0.0001, "c").is_none());
    }

    #[test]
    fn fields_move_with_blocking_and_resize_from_the_far_edge() {
        let zones = vec![zone("a", 0.0, 0.0, 0.3, 0.3), zone("b", 0.5, 0.0, 0.3, 0.3)];
        near(set_field(&zones, "a", Field::X, 0.9)[0].rect.x, 0.2);
        let wider = set_field(&zones, "a", Field::Width, 0.4);
        near(wider[0].rect.x, 0.0);
        near(wider[0].rect.width, 0.4);
        near(
            set_field(&zones, "a", Field::Height, 0.0)[0].rect.height,
            MINIMUM_ZONE_SIZE,
        );
    }
}
