use thiserror::Error;

use super::{Axis, EPSILON, Side, is_free};
use crate::geometry::{Point, distance};
use crate::layout::{MAX_ZONES, MINIMUM_ZONE_SIZE, UnitRect, Zone};

/// "Add" never creates a zone larger than this in either dimension.
const ADD_SIZE_CAP: f64 = 0.5;

/// The zone drawn by dragging from `from` to `to` on empty canvas.
///
/// The dragged rectangle itself when it's free; otherwise the largest free rectangle
/// that keeps a corner at `from` and stays inside the dragged area.
pub fn draw(zones: &[Zone], from: Point, to: Point) -> Option<UnitRect> {
    if !from.is_finite() || !to.is_finite() {
        return None;
    }
    let anchor = clamp_to_canvas(from);
    let corner = clamp_to_canvas(to);
    let dragged = rect_between(anchor, corner);
    if dragged.is_valid() && is_free(dragged, zones, None) {
        return Some(dragged);
    }

    // Each axis can stop at the drag's end or at the near edge of any zone in the way.
    let stops = |axis: Axis, anchor: f64, end: f64| -> Vec<f64> {
        let near_side = if end >= anchor { Side::Start } else { Side::End };
        let (low, high) = (anchor.min(end) - EPSILON, anchor.max(end) + EPSILON);
        std::iter::once(end)
            .chain(zones.iter().map(|zone| zone.rect.edge(axis, near_side)))
            .filter(|stop| (low..=high).contains(stop))
            .collect()
    };
    let xs = stops(Axis::X, anchor.x, corner.x);
    let ys = stops(Axis::Y, anchor.y, corner.y);

    xs.iter()
        .flat_map(|&x| ys.iter().map(move |&y| rect_between(anchor, Point::new(x, y))))
        .filter(|rect| rect.is_valid() && is_free(*rect, zones, None))
        .fold(None, |best: Option<UnitRect>, rect| match best {
            Some(current) if current.area() >= rect.area() => Some(current),
            _ => Some(rect),
        })
}

/// Where "Add" puts a new zone: the largest empty area, centered and capped.
pub fn add_rect(zones: &[Zone]) -> Option<UnitRect> {
    let largest = empty_areas(zones).fold(None, |best: Option<UnitRect>, area| match best {
        Some(current) if current.area() >= area.area() => Some(current),
        _ => Some(area),
    })?;
    let width = largest.width.min(ADD_SIZE_CAP);
    let height = largest.height.min(ADD_SIZE_CAP);
    Some(UnitRect::new(
        largest.x + (largest.width - width) / 2.0,
        largest.y + (largest.height - height) / 2.0,
        width,
        height,
    ))
}

/// Free rectangles of the canvas: for every column between two vertical edges, the
/// vertical runs that no zone crossing the column occupies.
fn empty_areas(zones: &[Zone]) -> impl Iterator<Item = UnitRect> + '_ {
    let mut edges: Vec<f64> = [0.0, 1.0]
        .into_iter()
        .chain(zones.iter().flat_map(|zone| [zone.rect.x, zone.rect.right()]))
        .collect();
    edges.sort_by(f64::total_cmp);
    edges.dedup();

    let columns: Vec<(f64, f64)> = edges
        .iter()
        .enumerate()
        .flat_map(|(i, &left)| edges[i + 1..].iter().map(move |&right| (left, right)))
        .filter(|(left, right)| right - left >= MINIMUM_ZONE_SIZE)
        .collect();

    columns.into_iter().flat_map(move |(left, right)| {
        let mut blockers: Vec<(f64, f64)> = zones
            .iter()
            .filter(|zone| right.min(zone.rect.right()) - left.max(zone.rect.x) > EPSILON)
            .map(|zone| (zone.rect.y, zone.rect.bottom()))
            .collect();
        blockers.sort_by(|a, b| a.0.total_cmp(&b.0));

        let mut runs = Vec::new();
        let mut top = 0.0_f64;
        for (start, end) in blockers {
            runs.push(UnitRect::new(left, top, right - left, start - top));
            top = top.max(end);
        }
        runs.push(UnitRect::new(left, top, right - left, 1.0 - top));
        runs.into_iter().filter(|run| run.is_valid())
    })
}

/// Why copies can't be placed, as the editor explains it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PlaceRefusal {
    #[error("Copy zones first.")]
    NothingCopied,
    #[error("Overlapping zones can't be copied.")]
    Overlapping,
    #[error("A layout holds at most {MAX_ZONES} zones.")]
    TooManyZones,
    #[error("No free space for the copies.")]
    NoRoom,
}

/// Where same-size copies of `copied` go, keeping their arrangement, for Paste and Duplicate.
///
/// The copies land where the originals are when that space is free, so a cut zone pastes
/// back where it was and a zone copied from another layout keeps its place. Otherwise they
/// go to the nearest free spot that puts them beside where they were, beside another zone,
/// or against a canvas edge.
pub fn place_copies(copied: &[UnitRect], zones: &[Zone]) -> Result<Vec<UnitRect>, PlaceRefusal> {
    let bounds = bounding_rect(copied).ok_or(PlaceRefusal::NothingCopied)?;
    let overlapping = copied
        .iter()
        .enumerate()
        .any(|(i, rect)| copied[i + 1..].iter().any(|other| rect.overlaps(*other)));
    if overlapping {
        return Err(PlaceRefusal::Overlapping);
    }
    if zones.len() + copied.len() > MAX_ZONES {
        return Err(PlaceRefusal::TooManyZones);
    }

    let positions = |axis: Axis| -> Vec<f64> {
        let (start, end) = (bounds.edge(axis, Side::Start), bounds.edge(axis, Side::End));
        let length = end - start;
        let mut values = vec![start, end, start - length, 0.0, 1.0 - length];
        for zone in zones {
            values.extend([
                zone.rect.edge(axis, Side::End),
                zone.rect.edge(axis, Side::Start) - length,
            ]);
        }
        values.into_iter().map(|value| value - start).collect()
    };
    let (shifts_x, shifts_y) = (positions(Axis::X), positions(Axis::Y));

    let placed = |dx: f64, dy: f64| -> Option<Vec<UnitRect>> {
        copied
            .iter()
            .map(|rect| UnitRect::new(rect.x + dx, rect.y + dy, rect.width, rect.height))
            .map(|rect| (rect.is_valid() && is_free(rect, zones, None)).then_some(rect))
            .collect()
    };
    shifts_x
        .iter()
        .flat_map(|&dx| shifts_y.iter().map(move |&dy| (dx, dy)))
        .filter_map(|(dx, dy)| placed(dx, dy).map(|rects| (distance(dx, dy), rects)))
        .min_by(|(a, _), (b, _)| a.total_cmp(b))
        .map(|(_, rects)| rects)
        .ok_or(PlaceRefusal::NoRoom)
}

/// The smallest rectangle holding all of `rects`; `None` for none.
pub(crate) fn bounding_rect(rects: &[UnitRect]) -> Option<UnitRect> {
    let first = *rects.first()?;
    Some(rects.iter().skip(1).fold(first, |bounds, rect| {
        let (left, top) = (bounds.x.min(rect.x), bounds.y.min(rect.y));
        let (right, bottom) = (
            bounds.right().max(rect.right()),
            bounds.bottom().max(rect.bottom()),
        );
        UnitRect::new(left, top, right - left, bottom - top)
    }))
}

fn clamp_to_canvas(point: Point) -> Point {
    Point::new(point.x.clamp(0.0, 1.0), point.y.clamp(0.0, 1.0))
}

fn rect_between(a: Point, b: Point) -> UnitRect {
    UnitRect::new(a.x.min(b.x), a.y.min(b.y), (a.x - b.x).abs(), (a.y - b.y).abs())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{near, zone};
    use super::*;

    #[test]
    fn drawing_stops_at_zones_and_works_in_reverse() {
        let zones = vec![zone("a", 0.5, 0.0, 0.5, 1.0)];
        let drawn = draw(&zones, Point::new(0.1, 0.1), Point::new(0.9, 0.5)).unwrap();
        near(drawn.x, 0.1);
        near(drawn.right(), 0.5);
        let reverse = draw(&[], Point::new(0.6, 0.6), Point::new(0.2, 0.2)).unwrap();
        near(reverse.x, 0.2);
        near(reverse.width, 0.4);
    }

    #[test]
    fn tiny_drags_draw_nothing() {
        assert!(draw(&[], Point::new(0.1, 0.1), Point::new(0.11, 0.5)).is_none());
    }

    #[test]
    fn add_centers_and_caps_in_the_largest_space() {
        assert_eq!(add_rect(&[]), Some(UnitRect::new(0.25, 0.25, 0.5, 0.5)));
        let added = add_rect(&[zone("a", 0.0, 0.0, 0.5, 1.0)]).unwrap();
        near(added.x, 0.5);
        near(added.width, 0.5);
        assert!(add_rect(&[zone("a", 0.0, 0.0, 1.0, 1.0)]).is_none());
    }

    #[test]
    fn a_copy_lands_in_the_nearest_free_spot_when_its_own_is_taken() {
        let zones = vec![zone("a", 0.0, 0.0, 0.5, 1.0)];
        let copies = place_copies(&[zones[0].rect], &zones).unwrap();
        near(copies[0].x, 0.5);
        near(copies[0].width, 0.5);
        let full = [zone("f", 0.0, 0.0, 1.0, 1.0)];
        assert_eq!(place_copies(&[full[0].rect], &full), Err(PlaceRefusal::NoRoom));
    }

    #[test]
    fn copies_paste_back_where_they_were_when_that_space_is_free() {
        let copied = [UnitRect::new(0.5, 0.0, 0.5, 0.5)];
        let zones = vec![zone("a", 0.0, 0.0, 0.5, 1.0)];
        assert_eq!(place_copies(&copied, &zones), Ok(copied.to_vec()));
    }

    #[test]
    fn several_copies_keep_their_arrangement_and_move_together() {
        let zones = vec![
            zone("a", 0.0, 0.0, 0.25, 0.5),
            zone("b", 0.25, 0.0, 0.25, 0.5),
            zone("c", 0.0, 0.5, 0.5, 0.5),
        ];
        let copies = place_copies(&[zones[0].rect, zones[1].rect], &zones).unwrap();
        assert_eq!(copies.len(), 2);
        near(copies[0].x, 0.5);
        near(copies[0].y, 0.0);
        near(copies[1].x, 0.75);
        near(copies[1].y, 0.0);
        assert!(copies.iter().all(|rect| is_free(*rect, &zones, None)));
    }

    #[test]
    fn copies_of_nothing_of_overlapping_zones_or_that_would_overfill_the_layout_go_nowhere() {
        assert_eq!(place_copies(&[], &[]), Err(PlaceRefusal::NothingCopied));
        let overlapping = [
            UnitRect::new(0.0, 0.0, 0.5, 0.5),
            UnitRect::new(0.4, 0.0, 0.5, 0.5),
        ];
        assert_eq!(place_copies(&overlapping, &[]), Err(PlaceRefusal::Overlapping));
        let zones: Vec<Zone> = (0..MAX_ZONES)
            .map(|index| {
                let (column, row) = (index % 16, index / 16);
                zone(
                    &format!("z{index}"),
                    column as f64 * 0.05,
                    row as f64 * 0.05,
                    0.05,
                    0.05,
                )
            })
            .collect();
        let copied = [zones[0].rect];
        assert!(place_copies(&copied, &zones[..MAX_ZONES - 1]).is_ok());
        assert_eq!(place_copies(&copied, &zones), Err(PlaceRefusal::TooManyZones));
        assert_eq!(
            PlaceRefusal::TooManyZones.to_string(),
            "A layout holds at most 32 zones."
        );
    }
}
