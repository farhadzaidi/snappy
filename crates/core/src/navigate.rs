//! Moving a window between zones with the arrow shortcuts.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::geometry::{Point, Rect};
use crate::layout::{Layout, Spacing, Zone, zone_frame};

/// A zone counts as ahead only when its center is further than this in the direction.
const AHEAD_THRESHOLD: f64 = 0.5;
/// Zones whose spans across the direction share more than this are aligned.
const ALIGNED_THRESHOLD: f64 = 0.5;
/// Areas and distances closer than this tie.
const TIE_TOLERANCE: f64 = 0.001;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub const ALL: [Direction; 4] = [Direction::Left, Direction::Right, Direction::Up, Direction::Down];

    pub fn name(self) -> &'static str {
        match self {
            Direction::Left => "left",
            Direction::Right => "right",
            Direction::Up => "up",
            Direction::Down => "down",
        }
    }

    fn is_horizontal(self) -> bool {
        matches!(self, Direction::Left | Direction::Right)
    }

    /// +1 when travel increases the coordinate, -1 when it decreases it.
    fn sign(self) -> f64 {
        match self {
            Direction::Right | Direction::Down => 1.0,
            Direction::Left | Direction::Up => -1.0,
        }
    }

    /// Position along the direction of travel.
    fn along(self, point: Point) -> f64 {
        if self.is_horizontal() { point.x } else { point.y }
    }

    /// Position across the direction of travel.
    fn across(self, point: Point) -> f64 {
        if self.is_horizontal() { point.y } else { point.x }
    }

    /// A rectangle's extent across the direction of travel.
    fn cross_span(self, rect: Rect) -> (f64, f64) {
        if self.is_horizontal() {
            (rect.min_y(), rect.max_y())
        } else {
            (rect.min_x(), rect.max_x())
        }
    }

    /// Empty space between `from` and `to` along the direction of travel.
    fn gap(self, from: Rect, to: Rect) -> f64 {
        match self {
            Direction::Right => to.min_x() - from.max_x(),
            Direction::Left => from.min_x() - to.max_x(),
            Direction::Down => to.min_y() - from.max_y(),
            Direction::Up => from.min_y() - to.max_y(),
        }
    }
}

/// The zone the window should move to, or `None` at the screen's edge.
///
/// `from_zone` continues from a destination still being settled; otherwise the
/// window starts from the zone it overlaps most.
pub fn destination<'a>(
    window: Rect,
    layout: &'a Layout,
    work_area: Rect,
    spacing: Spacing,
    direction: Direction,
    from_zone: Option<&str>,
) -> Option<&'a Zone> {
    if !window.is_usable(f64::MIN_POSITIVE) || !work_area.is_usable(f64::MIN_POSITIVE) || !layout.is_valid() {
        return None;
    }
    if window.intersection_area(work_area) <= 0.0 {
        return None;
    }
    let frames: Vec<Rect> = layout
        .zones
        .iter()
        .map(|zone| zone_frame(zone.rect, work_area, spacing))
        .collect();
    let start = match from_zone {
        Some(id) => layout.zones.iter().position(|zone| zone.id == id),
        None => starting_zone(window, &frames),
    }?;

    let origin = frames[start];
    let (low, high) = direction.cross_span(origin);
    let anchor = direction.across(window.center()).clamp(low, high);

    (0..frames.len())
        .filter(|&index| index != start)
        .filter(|&index| {
            (direction.along(frames[index].center()) - direction.along(origin.center())) * direction.sign()
                > AHEAD_THRESHOLD
        })
        .map(|index| (index, rank(direction, origin, frames[index], anchor, index)))
        .min_by(|a, b| a.1.cmp(&b.1))
        .map(|(index, _)| &layout.zones[index])
}

/// The zone the window overlaps most; nearer centers, then earlier zones, break ties.
fn starting_zone(window: Rect, frames: &[Rect]) -> Option<usize> {
    let squared_distance = |frame: Rect| {
        let (dx, dy) = (window.mid_x() - frame.mid_x(), window.mid_y() - frame.mid_y());
        dx * dx + dy * dy
    };
    (0..frames.len()).min_by(|&a, &b| {
        let (overlap_a, overlap_b) = (
            window.intersection_area(frames[a]),
            window.intersection_area(frames[b]),
        );
        if (overlap_a - overlap_b).abs() > TIE_TOLERANCE {
            return overlap_b.total_cmp(&overlap_a);
        }
        let (near_a, near_b) = (squared_distance(frames[a]), squared_distance(frames[b]));
        if (near_a - near_b).abs() > TIE_TOLERANCE {
            return near_a.total_cmp(&near_b);
        }
        a.cmp(&b)
    })
}

/// How good a candidate is. Lower is better; aligned candidates always beat others.
#[derive(Debug, PartialEq)]
struct Rank {
    misaligned: bool,
    distance: f64,
    anchor_to_span: f64,
    anchor_to_center: f64,
    travel: f64,
    index: usize,
}

impl Eq for Rank {}

impl PartialOrd for Rank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rank {
    fn cmp(&self, other: &Self) -> Ordering {
        self.misaligned
            .cmp(&other.misaligned)
            .then(self.distance.total_cmp(&other.distance))
            .then(self.anchor_to_span.total_cmp(&other.anchor_to_span))
            .then(self.anchor_to_center.total_cmp(&other.anchor_to_center))
            .then(self.travel.total_cmp(&other.travel))
            .then(self.index.cmp(&other.index))
    }
}

fn rank(direction: Direction, origin: Rect, candidate: Rect, anchor: f64, index: usize) -> Rank {
    let (origin_low, origin_high) = direction.cross_span(origin);
    let (low, high) = direction.cross_span(candidate);
    let misaligned = origin_high.min(high) - origin_low.max(low) <= ALIGNED_THRESHOLD;
    let (dx, dy) = (
        candidate.mid_x() - origin.mid_x(),
        candidate.mid_y() - origin.mid_y(),
    );

    Rank {
        misaligned,
        distance: if misaligned {
            dx * dx + dy * dy
        } else {
            direction.gap(origin, candidate).max(0.0)
        },
        anchor_to_span: (low - anchor).max(anchor - high).max(0.0),
        anchor_to_center: ((low + high) / 2.0 - anchor).abs(),
        travel: if misaligned {
            0.0
        } else {
            (direction.along(candidate.center()) - direction.along(origin.center())).abs()
        },
        index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{UnitRect, preset};

    const WORK: Rect = Rect::new(0.0, 25.0, 1440.0, 875.0);

    fn frame_of(layout: &Layout, id: &str) -> Rect {
        zone_frame(layout.zone(id).unwrap().rect, WORK, Spacing::STANDARD)
    }

    fn go(layout: &Layout, window: Rect, direction: Direction) -> Option<String> {
        destination(window, layout, WORK, Spacing::STANDARD, direction, None).map(|zone| zone.id.clone())
    }

    #[test]
    fn grids_move_spatially_and_stop_at_edges() {
        let quarters = preset("quarters").unwrap();
        let top_left = frame_of(quarters, "quarters-0");
        assert_eq!(
            go(quarters, top_left, Direction::Right).as_deref(),
            Some("quarters-1")
        );
        assert_eq!(
            go(quarters, top_left, Direction::Down).as_deref(),
            Some("quarters-2")
        );
        assert_eq!(go(quarters, top_left, Direction::Left), None);
        assert_eq!(go(quarters, top_left, Direction::Up), None);
    }

    #[test]
    fn the_window_position_picks_among_a_stack() {
        let stack = preset("wide-left").unwrap();
        let main = frame_of(stack, "wide-left-0");
        let low_window = Rect::new(main.x, main.max_y() - 200.0, main.width, 200.0);
        assert_eq!(
            go(stack, low_window, Direction::Right).as_deref(),
            Some("wide-left-2")
        );
        let high_window = Rect::new(main.x, main.y, main.width, 200.0);
        assert_eq!(
            go(stack, high_window, Direction::Right).as_deref(),
            Some("wide-left-1")
        );
    }

    #[test]
    fn aligned_neighbors_beat_closer_diagonals() {
        let layout = Layout {
            id: "t".into(),
            name: "t".into(),
            zones: vec![
                Zone::new("a", UnitRect::new(0.0, 0.0, 0.3, 0.5)),
                Zone::new("diagonal", UnitRect::new(0.3, 0.5, 0.2, 0.5)),
                Zone::new("aligned", UnitRect::new(0.6, 0.0, 0.4, 0.5)),
            ],
        };
        let window = frame_of(&layout, "a");
        assert_eq!(go(&layout, window, Direction::Right).as_deref(), Some("aligned"));
    }

    fn custom(zones: &[(&str, UnitRect)]) -> Layout {
        Layout {
            id: "t".into(),
            name: "t".into(),
            zones: zones.iter().map(|(id, rect)| Zone::new(*id, *rect)).collect(),
        }
    }

    fn squared_center_distance(a: Rect, b: Rect) -> f64 {
        let (dx, dy) = (a.mid_x() - b.mid_x(), a.mid_y() - b.mid_y());
        dx * dx + dy * dy
    }

    #[test]
    fn the_nearer_non_aligned_center_beats_zone_order() {
        let layout = custom(&[
            ("a", UnitRect::new(0.0, 0.0, 0.3, 0.5)),
            ("far", UnitRect::new(0.7, 0.5, 0.3, 0.5)),
            ("near", UnitRect::new(0.35, 0.5, 0.2, 0.5)),
        ]);
        let window = frame_of(&layout, "a");
        assert_eq!(go(&layout, window, Direction::Right).as_deref(), Some("near"));
    }

    #[test]
    fn equal_distances_fall_back_to_the_anchor_then_zone_order() {
        let layout = custom(&[
            ("a", UnitRect::new(0.0, 0.3, 0.3, 0.4)),
            ("below", UnitRect::new(0.5, 0.75, 0.3, 0.25)),
            ("above", UnitRect::new(0.5, 0.0, 0.3, 0.25)),
        ]);
        let origin = frame_of(&layout, "a");
        assert_eq!(
            squared_center_distance(origin, frame_of(&layout, "above")),
            squared_center_distance(origin, frame_of(&layout, "below")),
            "the layout must tie on distance"
        );

        let high = Rect::new(origin.x, origin.y, origin.width, 100.0);
        assert_eq!(go(&layout, high, Direction::Right).as_deref(), Some("above"));
        let low = Rect::new(origin.x, origin.max_y() - 100.0, origin.width, 100.0);
        assert_eq!(go(&layout, low, Direction::Right).as_deref(), Some("below"));
        assert_eq!(
            go(&layout, origin, Direction::Right).as_deref(),
            Some("below"),
            "a full tie goes to the earlier zone"
        );
    }

    #[test]
    fn repeated_presses_advance_from_the_pending_destination() {
        let thirds = preset("thirds").unwrap();
        let left = frame_of(thirds, "thirds-0");
        let next = destination(
            left,
            thirds,
            WORK,
            Spacing::STANDARD,
            Direction::Right,
            Some("thirds-1"),
        );
        assert_eq!(next.map(|zone| zone.id.as_str()), Some("thirds-2"));
    }

    #[test]
    fn invalid_or_offscreen_windows_go_nowhere() {
        let halves = preset("halves").unwrap();
        assert_eq!(
            go(halves, Rect::new(5000.0, 0.0, 100.0, 100.0), Direction::Right),
            None
        );
        assert_eq!(
            go(halves, Rect::new(0.0, 0.0, f64::NAN, 1.0), Direction::Right),
            None
        );
    }

    #[test]
    fn the_start_is_the_most_overlapped_zone_then_the_nearest_then_the_earliest() {
        let small = Rect::new(0.0, 0.0, 100.0, 100.0);
        let tall = Rect::new(200.0, 0.0, 100.0, 300.0);
        let mostly_right = Rect::new(80.0, 0.0, 200.0, 100.0);
        assert_eq!(starting_zone(mostly_right, &[small, tall]), Some(1));

        let straddling = Rect::new(50.0, 0.0, 200.0, 100.0);
        assert_eq!(
            starting_zone(straddling, &[tall, small]),
            Some(1),
            "equal overlaps go to the nearer center"
        );

        let twin = Rect::new(200.0, 0.0, 100.0, 100.0);
        assert_eq!(starting_zone(straddling, &[small, twin]), Some(0));
        assert_eq!(
            starting_zone(straddling, &[twin, small]),
            Some(0),
            "then the earlier zone"
        );
    }
}
