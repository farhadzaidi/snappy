//! Merging selected zones into one.

use std::collections::BTreeMap;

use thiserror::Error;

use super::creating::bounding_rect;
use super::{Axis, EPSILON, Side, is_free, is_separated};
use crate::layout::{UnitRect, Zone};

/// Why the selected zones can't be merged, as the editor explains it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MergeRefusal {
    #[error("Shift-click another zone to merge them.")]
    TooFew,
    #[error("Overlapping zones can't be merged.")]
    Overlapping,
    #[error("Zones merge only when together they fill a rectangle with no gaps.")]
    NotARectangle,
}

/// The layout after a merge.
#[derive(Debug, Clone, PartialEq)]
pub struct Merged {
    pub zones: Vec<Zone>,
    /// The merged zone's ID: that of the first merged zone in zone order.
    pub kept: String,
    /// The other merged zones, gone now, in zone order.
    pub absorbed: Vec<String>,
}

/// Merge the zones with `ids` into the rectangle they fill together.
///
/// The merged zone is the first of them in zone order, grown, so it keeps that zone's ID,
/// number, and place in the order; the others are removed and later zones move up.
pub fn merge(zones: &[Zone], ids: &[String]) -> Result<Merged, MergeRefusal> {
    let (selected, others): (Vec<Zone>, Vec<Zone>) =
        zones.iter().cloned().partition(|zone| ids.contains(&zone.id));
    if selected.len() < 2 {
        return Err(MergeRefusal::TooFew);
    }
    let rects: Vec<UnitRect> = selected.iter().map(|zone| zone.rect).collect();
    let bounds = bounding_rect(&rects).ok_or(MergeRefusal::TooFew)?;

    if !is_separated(&selected) {
        return Err(MergeRefusal::Overlapping);
    }
    let covered: f64 = rects.iter().map(|rect| rect.area()).sum();
    if (bounds.area() - covered).abs() > EPSILON {
        return Err(MergeRefusal::NotARectangle);
    }
    // Only a draft that already overlaps can have another zone inside a filled rectangle.
    if !is_free(bounds, &others, None) {
        return Err(MergeRefusal::Overlapping);
    }

    let kept = selected[0].id.clone();
    let grown = grown_to(selected[0].rect, bounds);
    let absorbed: Vec<String> = selected[1..].iter().map(|zone| zone.id.clone()).collect();
    let merged = zones
        .iter()
        .filter(|zone| !absorbed.contains(&zone.id))
        .map(|zone| {
            if zone.id == kept {
                Zone::new(kept.clone(), grown)
            } else {
                zone.clone()
            }
        })
        .collect();
    Ok(Merged {
        zones: merged,
        kept,
        absorbed,
    })
}

/// `rect` with each edge moved out to `bounds`. Edges already there keep their exact values:
/// a minimum-size side worked out again from the bounds can round to just under the minimum.
fn grown_to(rect: UnitRect, bounds: UnitRect) -> UnitRect {
    let mut grown = rect;
    for axis in [Axis::X, Axis::Y] {
        for side in [Side::Start, Side::End] {
            let edge = bounds.edge(axis, side);
            if (grown.edge(axis, side) - edge).abs() > EPSILON {
                grown.set_edge(axis, side, edge);
            }
        }
    }
    grown
}

impl Merged {
    /// The zone shortcuts after the merge. The merged zone keeps the first shortcut, in zone
    /// order, among the zones merged into it; the others' shortcuts go with them.
    pub fn shortcuts<Chord: Clone>(&self, shortcuts: &BTreeMap<String, Chord>) -> BTreeMap<String, Chord> {
        let carried = std::iter::once(&self.kept)
            .chain(&self.absorbed)
            .find_map(|id| shortcuts.get(id))
            .cloned();
        let mut result: BTreeMap<String, Chord> = shortcuts
            .iter()
            .filter(|(id, _)| **id != self.kept && !self.absorbed.contains(id))
            .map(|(id, chord)| (id.clone(), chord.clone()))
            .collect();
        if let Some(chord) = carried {
            result.insert(self.kept.clone(), chord);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{grid, rect, zone};
    use super::*;
    use crate::layout::MINIMUM_ZONE_SIZE;

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    fn order(zones: &[Zone]) -> Vec<&str> {
        zones.iter().map(|zone| zone.id.as_str()).collect()
    }

    #[test]
    fn merging_a_row_keeps_the_first_zone_grown_in_its_place() {
        let merged = merge(&grid(), &ids(&["b", "a"])).unwrap();
        assert_eq!(order(&merged.zones), ["a", "c", "d"]);
        assert_eq!(merged.kept, "a");
        assert_eq!(merged.absorbed, ["b"]);
        assert_eq!(rect(&merged.zones, "a"), UnitRect::new(0.0, 0.0, 1.0, 0.5));
    }

    #[test]
    fn later_zones_move_up_in_the_order() {
        let merged = merge(&grid(), &ids(&["b", "d"])).unwrap();
        assert_eq!(order(&merged.zones), ["a", "b", "c"]);
        assert_eq!(rect(&merged.zones, "b"), UnitRect::new(0.5, 0.0, 0.5, 1.0));
    }

    #[test]
    fn every_zone_of_a_grid_merges_into_one() {
        let merged = merge(&grid(), &ids(&["a", "b", "c", "d"])).unwrap();
        assert_eq!(merged.zones, [zone("a", 0.0, 0.0, 1.0, 1.0)]);
    }

    #[test]
    fn merging_minimum_height_zones_keeps_them_at_the_minimum() {
        let top = 0.4178598197170308;
        let thin = vec![
            zone("a", 0.0, top, 0.8907161892112186, MINIMUM_ZONE_SIZE),
            zone(
                "b",
                0.8907161892112186,
                top,
                0.0975810346519895,
                MINIMUM_ZONE_SIZE,
            ),
        ];
        let merged = merge(&thin, &ids(&["a", "b"])).unwrap();
        let rect = rect(&merged.zones, "a");
        assert_eq!(rect.height, MINIMUM_ZONE_SIZE);
        assert!(rect.is_valid());
    }

    #[test]
    fn zones_that_leave_a_gap_or_an_l_shape_do_not_merge() {
        assert_eq!(
            merge(&grid(), &ids(&["a", "b", "c"])),
            Err(MergeRefusal::NotARectangle)
        );
        assert_eq!(
            merge(&grid(), &ids(&["a", "d"])),
            Err(MergeRefusal::NotARectangle)
        );
        let apart = vec![zone("a", 0.0, 0.0, 0.4, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)];
        assert_eq!(merge(&apart, &ids(&["a", "b"])), Err(MergeRefusal::NotARectangle));
    }

    #[test]
    fn one_zone_or_unknown_ids_are_too_few() {
        assert_eq!(merge(&grid(), &ids(&["a"])), Err(MergeRefusal::TooFew));
        assert_eq!(merge(&grid(), &ids(&["a", "gone"])), Err(MergeRefusal::TooFew));
        assert_eq!(merge(&grid(), &[]), Err(MergeRefusal::TooFew));
    }

    #[test]
    fn overlapping_zones_do_not_merge() {
        let overlapping = vec![zone("a", 0.0, 0.0, 0.6, 1.0), zone("b", 0.5, 0.0, 0.5, 1.0)];
        assert_eq!(
            merge(&overlapping, &ids(&["a", "b"])),
            Err(MergeRefusal::Overlapping)
        );
        let covered = vec![
            zone("a", 0.0, 0.0, 0.5, 1.0),
            zone("b", 0.5, 0.0, 0.5, 1.0),
            zone("over", 0.4, 0.4, 0.2, 0.2),
        ];
        assert_eq!(merge(&covered, &ids(&["a", "b"])), Err(MergeRefusal::Overlapping));
    }

    #[test]
    fn the_merged_zone_keeps_the_first_shortcut_in_zone_order() {
        let merged = merge(&grid(), &ids(&["a", "b", "c", "d"])).unwrap();
        let shortcuts = BTreeMap::from([("c".to_owned(), "C"), ("b".to_owned(), "B")]);
        assert_eq!(
            merged.shortcuts(&shortcuts),
            BTreeMap::from([("a".to_owned(), "B")])
        );

        let merged = merge(&grid(), &ids(&["a", "b"])).unwrap();
        let shortcuts = BTreeMap::from([
            ("a".to_owned(), "A"),
            ("b".to_owned(), "B"),
            ("d".to_owned(), "D"),
        ]);
        assert_eq!(
            merged.shortcuts(&shortcuts),
            BTreeMap::from([("a".to_owned(), "A"), ("d".to_owned(), "D")])
        );
        assert!(merged.shortcuts::<&str>(&BTreeMap::new()).is_empty());
    }
}
