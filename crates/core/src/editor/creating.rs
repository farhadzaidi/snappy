use super::{Axis, EPSILON, Side, is_free};
use crate::geometry::{Point, distance};
use crate::layout::{MINIMUM_ZONE_SIZE, UnitRect, Zone};

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

/// The nearest free spot for a same-size copy of `source`: beside the source,
/// beside another zone, or against a canvas edge.
pub fn duplicate_rect(source: UnitRect, zones: &[Zone]) -> Option<UnitRect> {
    let positions = |axis: Axis, length: f64| -> Vec<f64> {
        let start = source.edge(axis, Side::Start);
        let mut values = vec![
            source.edge(axis, Side::End),
            start - length,
            start,
            0.0,
            1.0 - length,
        ];
        for zone in zones {
            values.extend([
                zone.rect.edge(axis, Side::End),
                zone.rect.edge(axis, Side::Start) - length,
            ]);
        }
        values
    };
    let xs = positions(Axis::X, source.width);
    let ys = positions(Axis::Y, source.height);

    xs.iter()
        .flat_map(|&x| {
            ys.iter()
                .map(move |&y| UnitRect::new(x, y, source.width, source.height))
        })
        .filter(|rect| rect.is_valid() && is_free(*rect, zones, None))
        .fold(None, |best: Option<(UnitRect, f64)>, rect| {
            let away = distance(rect.x - source.x, rect.y - source.y);
            match best {
                Some((_, nearest)) if nearest <= away => best,
                _ => Some((rect, away)),
            }
        })
        .map(|(rect, _)| rect)
}

/// Cut a zone at `fraction` of its size along `axis`. The first part keeps the
/// zone's ID; the second takes `new_id`. `None` when either part would be too small.
pub fn split(zone: &Zone, fraction: f64, axis: Axis, new_id: impl Into<String>) -> Option<(Zone, Zone)> {
    let length = zone.rect.span(axis).end - zone.rect.span(axis).start;
    if !fraction.is_finite()
        || fraction * length < MINIMUM_ZONE_SIZE
        || (1.0 - fraction) * length < MINIMUM_ZONE_SIZE
    {
        return None;
    }
    let cut = zone.rect.edge(axis, Side::Start) + length * fraction;
    let mut first = zone.clone();
    first.rect.set_edge(axis, Side::End, cut);
    let mut second = Zone::new(new_id, zone.rect);
    second.rect.set_edge(axis, Side::Start, cut);
    (first.rect.is_valid() && second.rect.is_valid()).then_some((first, second))
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
    fn duplicates_land_in_the_nearest_free_spot() {
        let zones = vec![zone("a", 0.0, 0.0, 0.5, 1.0)];
        let copy = duplicate_rect(zones[0].rect, &zones).unwrap();
        near(copy.x, 0.5);
        assert!(
            duplicate_rect(
                UnitRect::new(0.0, 0.0, 1.0, 1.0),
                &[zone("f", 0.0, 0.0, 1.0, 1.0)]
            )
            .is_none()
        );
    }

    #[test]
    fn cuts_keep_coverage_and_the_first_id() {
        let original = zone("a", 0.0, 0.0, 1.0, 1.0);
        let (first, second) = split(&original, 0.3, Axis::X, "b").unwrap();
        assert_eq!(first.id, "a");
        assert_eq!(second.id, "b");
        near(first.rect.width, 0.3);
        near(second.rect.x, 0.3);
        near(first.rect.width + second.rect.width, 1.0);
        assert!(split(&original, 0.01, Axis::Y, "c").is_none());
    }
}
