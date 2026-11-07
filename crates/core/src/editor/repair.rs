use super::guides::{GUIDE_FRACTIONS, snap};
use super::{Axis, Side, is_sound};
use crate::layout::{UnitRect, Zone};

/// Edges closer than this are pulled onto one line by "Align edges".
pub const ALIGN_TOLERANCE: f64 = 0.012;

/// Repair a draft with slivers of overlap by pulling nearby edges onto shared lines.
///
/// Every zone and ID survives. Returns `None` when nothing would change or the result
/// still overlaps, so large overlaps are never guessed at.
pub fn align_edges(zones: &[Zone], tolerance: f64) -> Option<Vec<Zone>> {
    if !(tolerance.is_finite() && tolerance > 0.0) || !zones.iter().all(|zone| zone.rect.is_valid()) {
        return None;
    }
    let columns = EdgeLines::new(zones, Axis::X, tolerance);
    let rows = EdgeLines::new(zones, Axis::Y, tolerance);

    let aligned: Vec<Zone> = zones
        .iter()
        .map(|zone| {
            let r = zone.rect;
            let (left, right) = (columns.line_for(r.x), columns.line_for(r.right()));
            let (top, bottom) = (rows.line_for(r.y), rows.line_for(r.bottom()));
            Zone::new(
                zone.id.clone(),
                UnitRect::new(left, top, right - left, bottom - top),
            )
        })
        .collect();

    (aligned != zones && is_sound(&aligned)).then_some(aligned)
}

/// Edge positions along one axis, grouped into lines. Each group starts at its smallest
/// value and takes every later value within the tolerance; the line sits at the group's
/// mean, snapped to a screen fraction when one is close.
struct EdgeLines {
    lines: Vec<(f64, f64, f64)>,
}

impl EdgeLines {
    fn new(zones: &[Zone], axis: Axis, tolerance: f64) -> Self {
        let mut edges: Vec<f64> = zones
            .iter()
            .flat_map(|zone| [zone.rect.edge(axis, Side::Start), zone.rect.edge(axis, Side::End)])
            .collect();
        edges.sort_by(f64::total_cmp);

        let mut groups: Vec<Vec<f64>> = Vec::new();
        for edge in edges {
            match groups.last_mut() {
                Some(group) if edge - group[0] <= tolerance => group.push(edge),
                _ => groups.push(vec![edge]),
            }
        }

        let lines = groups
            .into_iter()
            .map(|group| {
                let mean = group.iter().sum::<f64>() / group.len() as f64;
                let first = group[0];
                let last = group[group.len() - 1];
                (first, last, snap(mean, &GUIDE_FRACTIONS, tolerance))
            })
            .collect();
        Self { lines }
    }

    fn line_for(&self, edge: f64) -> f64 {
        self.lines
            .iter()
            .find(|(first, last, _)| (*first..=*last).contains(&edge))
            .map_or(edge, |(_, _, line)| *line)
    }
}

#[cfg(test)]
mod tests {
    use super::super::is_separated;
    use super::super::test_support::zone;
    use super::*;

    #[test]
    fn slivers_align_and_ids_survive() {
        let zones = vec![zone("a", 0.0, 0.0, 0.505, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)];
        let aligned = align_edges(&zones, ALIGN_TOLERANCE).unwrap();
        assert!(is_separated(&aligned));
        assert_eq!(
            aligned.iter().map(|z| z.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
    }

    #[test]
    fn large_overlaps_are_refused() {
        let zones = vec![zone("a", 0.0, 0.0, 0.7, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)];
        assert!(align_edges(&zones, ALIGN_TOLERANCE).is_none());
    }
}
