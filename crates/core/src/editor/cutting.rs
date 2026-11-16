//! Cutting zones in two along a straight line: one zone, or every zone the line crosses.

use super::guides::{guides, zone_guides};
use super::{Axis, Side};
use crate::layout::{MAX_ZONES, MINIMUM_ZONE_SIZE, Zone};

/// What a cut goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutScope<'a> {
    /// Only the zone with this ID.
    Zone(&'a str),
    /// The whole layout: a line from one canvas edge to the other, through every zone it crosses.
    Through,
}

impl CutScope<'_> {
    fn includes(self, zone: &Zone) -> bool {
        match self {
            CutScope::Zone(id) => zone.id == id,
            CutScope::Through => true,
        }
    }
}

/// Where a cut can snap along `axis`: the screen guides and the other zones' edges, plus the
/// halves, thirds, and quarters of the zone being cut. A cut through the layout snaps to every
/// zone's edges.
pub fn cut_guides(zones: &[Zone], scope: CutScope, axis: Axis) -> Vec<f64> {
    match scope {
        CutScope::Zone(id) => {
            let mut targets = guides(zones, Some(id), axis, false);
            if let Some(zone) = zones.iter().find(|zone| zone.id == id) {
                targets.extend(zone_guides(zone.rect, axis));
            }
            targets
        }
        CutScope::Through => guides(zones, None, axis, false),
    }
}

/// The zones a cut at `line` splits, in zone order. A zone the line misses, or would leave a
/// part smaller than the minimum in, stays whole.
pub fn cut_zones<'a>(zones: &'a [Zone], scope: CutScope, axis: Axis, line: f64) -> Vec<&'a Zone> {
    zones
        .iter()
        .filter(|zone| scope.includes(zone) && split(zone, axis, line, "").is_some())
        .collect()
}

/// Cut at `line`. Each split zone keeps its ID for its first part, and its second part takes the
/// next of `new_ids` and follows it in zone order. `None` when nothing would be split, there are
/// too few new IDs, or the layout would have more than the most zones allowed.
pub fn cut(zones: &[Zone], scope: CutScope, axis: Axis, line: f64, new_ids: &[String]) -> Option<Vec<Zone>> {
    let splitting = cut_zones(zones, scope, axis, line).len();
    if splitting == 0 || splitting > new_ids.len() || zones.len() + splitting > MAX_ZONES {
        return None;
    }

    let mut ids = new_ids.iter();
    let mut result = Vec::with_capacity(zones.len() + splitting);
    for zone in zones {
        match scope
            .includes(zone)
            .then(|| split(zone, axis, line, ""))
            .flatten()
        {
            Some((first, mut second)) => {
                second.id = ids.next()?.clone();
                result.extend([first, second]);
            }
            None => result.push(zone.clone()),
        }
    }
    Some(result)
}

/// Cut a zone along `axis` at `line`. The first part keeps the zone's ID; the second takes
/// `new_id`. `None` when either part would be smaller than the minimum.
pub fn split(zone: &Zone, axis: Axis, line: f64, new_id: impl Into<String>) -> Option<(Zone, Zone)> {
    let span = zone.rect.span(axis);
    if !line.is_finite() || line - span.start < MINIMUM_ZONE_SIZE || span.end - line < MINIMUM_ZONE_SIZE {
        return None;
    }
    let mut first = zone.clone();
    first.rect.set_edge(axis, Side::End, line);
    let mut second = Zone::new(new_id, zone.rect);
    second.rect.set_edge(axis, Side::Start, line);
    (first.rect.is_valid() && second.rect.is_valid()).then_some((first, second))
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{grid, near, rect, zone};
    use super::super::{is_separated, is_sound};
    use super::*;

    fn ids(count: usize) -> Vec<String> {
        (0..count).map(|index| format!("new-{index}")).collect()
    }

    fn order(zones: &[Zone]) -> Vec<&str> {
        zones.iter().map(|zone| zone.id.as_str()).collect()
    }

    #[test]
    fn splitting_keeps_coverage_and_the_first_id() {
        let original = zone("a", 0.0, 0.0, 1.0, 1.0);
        let (first, second) = split(&original, Axis::X, 0.3, "b").unwrap();
        assert_eq!(first.id, "a");
        assert_eq!(second.id, "b");
        near(first.rect.width, 0.3);
        near(second.rect.x, 0.3);
        near(first.rect.width + second.rect.width, 1.0);
        assert!(split(&original, Axis::Y, 0.01, "c").is_none());
    }

    #[test]
    fn cutting_one_zone_puts_its_second_part_right_after_it() {
        let result = cut(&grid(), CutScope::Zone("a"), Axis::X, 0.25, &ids(1)).unwrap();
        assert_eq!(order(&result), ["a", "new-0", "b", "c", "d"]);
        near(rect(&result, "a").right(), 0.25);
        near(rect(&result, "new-0").x, 0.25);
        near(rect(&result, "c").width, 0.5);
    }

    #[test]
    fn cutting_through_splits_every_zone_the_line_crosses() {
        let result = cut(&grid(), CutScope::Through, Axis::Y, 0.25, &ids(4)).unwrap();
        assert_eq!(order(&result), ["a", "new-0", "b", "new-1", "c", "d"]);
        near(rect(&result, "a").bottom(), 0.25);
        near(rect(&result, "new-1").y, 0.25);
        near(rect(&result, "new-1").bottom(), 0.5);
        near(rect(&result, "c").height, 0.5);
        assert!(is_sound(&result));
    }

    #[test]
    fn cutting_through_skips_zones_off_the_line_and_crosses_gaps() {
        let zones = vec![
            zone("left", 0.0, 0.0, 0.3, 1.0),
            zone("short", 0.4, 0.6, 0.2, 0.4),
            zone("right", 0.7, 0.0, 0.3, 1.0),
        ];
        let result = cut(&zones, CutScope::Through, Axis::Y, 0.5, &ids(3)).unwrap();
        assert_eq!(order(&result), ["left", "new-0", "short", "right", "new-1"]);
        assert_eq!(rect(&result, "short"), zones[1].rect);
        near(rect(&result, "new-1").y, 0.5);
        assert!(is_separated(&result));
    }

    #[test]
    fn cutting_through_leaves_whole_the_zones_it_would_leave_a_sliver_of() {
        let zones = vec![
            zone("tall", 0.0, 0.0, 0.5, 1.0),
            zone("low", 0.5, 0.48, 0.5, 0.52),
        ];
        let split_ids: Vec<&str> = cut_zones(&zones, CutScope::Through, Axis::Y, 0.5)
            .into_iter()
            .map(|zone| zone.id.as_str())
            .collect();
        assert_eq!(split_ids, ["tall"]);
        let result = cut(&zones, CutScope::Through, Axis::Y, 0.5, &ids(2)).unwrap();
        assert_eq!(order(&result), ["tall", "new-0", "low"]);
    }

    #[test]
    fn a_cut_that_splits_nothing_or_overfills_the_layout_is_refused() {
        assert!(cut(&grid(), CutScope::Through, Axis::X, 0.5, &ids(4)).is_none());
        assert!(cut(&grid(), CutScope::Zone("missing"), Axis::X, 0.25, &ids(1)).is_none());
        assert!(cut(&grid(), CutScope::Through, Axis::X, 0.25, &ids(1)).is_none());

        let columns = 15;
        let width = 1.0 / f64::from(columns);
        let crowded: Vec<Zone> = (0..columns)
            .flat_map(|column| {
                let x = f64::from(column) * width;
                [
                    zone(&format!("top{column}"), x, 0.0, width, 0.5),
                    zone(&format!("bottom{column}"), x, 0.5, width, 0.5),
                ]
            })
            .collect();
        assert!(cut(&crowded, CutScope::Zone("top0"), Axis::Y, 0.25, &ids(1)).is_some());
        assert!(cut(&crowded, CutScope::Through, Axis::Y, 0.25, &ids(MAX_ZONES)).is_none());
    }

    #[test]
    fn a_zone_cut_snaps_to_its_own_fractions_and_a_through_cut_to_every_edge() {
        let zones = vec![zone("a", 0.0, 0.0, 0.6, 1.0), zone("b", 0.6, 0.0, 0.4, 1.0)];
        let own = cut_guides(&zones, CutScope::Zone("a"), Axis::X);
        for fraction in [0.15, 0.2, 0.3, 0.4, 0.45] {
            assert!(
                own.iter().any(|target| (target - fraction).abs() < 1e-9),
                "{fraction}"
            );
        }
        let through = cut_guides(&zones, CutScope::Through, Axis::X);
        assert!(through.iter().any(|target| (target - 0.6).abs() < 1e-9));
        assert!(!through.iter().any(|target| (target - 0.15).abs() < 1e-9));
    }
}
