//! Swapping two windows between zones, or a floating window with a zone's occupant.

use crate::bounds::GRID_TOLERANCE;
use crate::geometry::{Rect, largest_overlap};
use crate::keys::{Modifier, Mods};
use crate::window::{WindowId, WindowSnapshot};

/// Largest mismatch, in points, of a window's top and left edges for it to count as
/// filling a zone. Snapping places a window at the zone's corner exactly.
const OCCUPANCY_TOLERANCE: f64 = 12.0;
/// The tolerance never exceeds this fraction of the zone's shorter side.
const OCCUPANCY_FRACTION: f64 = 0.04;
/// The right and bottom edges may be further off, by as much as a snap accepts from apps
/// that size in steps, so a terminal snapped into a zone still counts as filling it.
const GRID_FRACTION: f64 = 0.08;

/// A zone on a specific display, with its frame on screen.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneSlot {
    pub display_id: String,
    pub zone_id: String,
    pub frame: Rect,
    pub work_area: Rect,
}

impl ZoneSlot {
    pub fn is_same_zone(&self, other: &ZoneSlot) -> bool {
        self.display_id == other.display_id && self.zone_id == other.zone_id
    }

    /// True when a window with `frame` fills this zone.
    pub fn is_occupied_by(&self, frame: Rect) -> bool {
        fills_zone(frame, self.frame)
    }
}

/// True when `frame` sits in `target`: its top-left corner matches closely, and its size is
/// as close as a snap accepts, since apps on a cell grid or with a minimum size can't
/// match the zone exactly.
pub fn fills_zone(frame: Rect, target: Rect) -> bool {
    if !is_real(frame) || !is_real(target) {
        return false;
    }
    let shorter = target.width.min(target.height);
    let corner = OCCUPANCY_TOLERANCE.min(shorter * OCCUPANCY_FRACTION);
    let size = GRID_TOLERANCE.min(shorter * GRID_FRACTION);
    let close = |a: f64, b: f64, tolerance: f64| (a - b).abs() <= tolerance;
    close(frame.min_x(), target.min_x(), corner)
        && close(frame.min_y(), target.min_y(), corner)
        && close(frame.max_x(), target.max_x(), size)
        && close(frame.max_y(), target.max_y(), size)
}

/// Where the swapping window starts, and so where the occupant goes. A swap trades zones,
/// so the occupant lands on the layout whenever a zone can take it.
#[derive(Debug, Clone, PartialEq)]
pub enum SwapSource {
    /// The window fills this zone.
    Zone(ZoneSlot),
    /// The window floats at `frame` over `zone`, the zone under its center, and the
    /// occupant takes that zone. [`plan_swap`] falls back to [`SwapSource::Floating`] when
    /// the zone is the destination or another window fills it.
    NearestZone { zone: ZoneSlot, frame: Rect },
    /// The window floats at `frame`, over no zone, and the occupant takes that frame.
    Floating { frame: Rect, work_area: Rect },
}

impl SwapSource {
    /// The source for a window at `frame` on the display with `work_area`. Every slot
    /// carries its display's work area, which is how the display's own zones are told
    /// apart from the others.
    pub fn find(frame: Rect, work_area: Rect, slots: &[ZoneSlot]) -> SwapSource {
        if let Some(slot) = source_slot(frame, slots) {
            return SwapSource::Zone(slot.clone());
        }
        let own: Vec<&ZoneSlot> = slots.iter().filter(|slot| slot.work_area == work_area).collect();
        match zone_under(frame, work_area, &own) {
            Some(zone) => SwapSource::NearestZone {
                zone: zone.clone(),
                frame,
            },
            None => SwapSource::Floating { frame, work_area },
        }
    }

    /// Where the occupant goes.
    pub fn frame(&self) -> Rect {
        match self {
            SwapSource::Zone(slot) | SwapSource::NearestZone { zone: slot, .. } => slot.frame,
            SwapSource::Floating { frame, .. } => *frame,
        }
    }

    pub fn work_area(&self) -> Rect {
        match self {
            SwapSource::Zone(slot) | SwapSource::NearestZone { zone: slot, .. } => slot.work_area,
            SwapSource::Floating { work_area, .. } => *work_area,
        }
    }

    /// The zone the occupant goes to, if any.
    pub fn zone(&self) -> Option<&ZoneSlot> {
        match self {
            SwapSource::Zone(slot) | SwapSource::NearestZone { zone: slot, .. } => Some(slot),
            SwapSource::Floating { .. } => None,
        }
    }

    /// True when a window with `frame` fills the place the occupant goes.
    pub fn is_filled_by(&self, frame: Rect) -> bool {
        fills_zone(frame, self.frame())
    }

    /// True when `original` is still where the swapping window started: the zone it
    /// fills, or its floating frame.
    pub fn starts_at(&self, original: Rect) -> bool {
        match self {
            SwapSource::Zone(slot) => fills_zone(original, slot.frame),
            SwapSource::NearestZone { frame, .. } | SwapSource::Floating { frame, .. } => {
                fills_zone(original, *frame)
            }
        }
    }

    pub fn is_zone(&self, slot: &ZoneSlot) -> bool {
        self.zone().is_some_and(|own| own.is_same_zone(slot))
    }

    /// The source's name in the debug log.
    pub fn kind(&self) -> &'static str {
        match self {
            SwapSource::Zone(_) => "zone",
            SwapSource::NearestZone { .. } => "nearestZone",
            SwapSource::Floating { .. } => "floating",
        }
    }

    /// The occupant takes the window's own frame instead of the zone under it.
    fn floating(&self) -> SwapSource {
        match self {
            SwapSource::NearestZone { zone, frame } => SwapSource::Floating {
                frame: *frame,
                work_area: zone.work_area,
            },
            other => other.clone(),
        }
    }
}

/// The one zone a window fills, or `None` when it fills none or several.
pub fn source_slot(frame: Rect, slots: &[ZoneSlot]) -> Option<&ZoneSlot> {
    let mut filled = slots.iter().filter(|slot| slot.is_occupied_by(frame));
    match (filled.next(), filled.next()) {
        (Some(slot), None) => Some(slot),
        _ => None,
    }
}

/// The zone of `slots` under the center of a window at `frame`. A center in a gap between
/// zones takes the zone the window overlaps most; a center outside the work area takes
/// none.
fn zone_under<'a>(frame: Rect, work_area: Rect, slots: &[&'a ZoneSlot]) -> Option<&'a ZoneSlot> {
    let center = frame.center();
    if !work_area.contains(center) {
        return None;
    }
    // Custom layouts may nest zones; the smallest one under the center is the one the
    // overlay would highlight there.
    let containing = slots
        .iter()
        .filter(|slot| slot.frame.contains(center))
        .min_by(|a, b| a.frame.area().total_cmp(&b.frame.area()));
    if let Some(slot) = containing {
        return Some(*slot);
    }
    let frames: Vec<Rect> = slots.iter().map(|slot| slot.frame).collect();
    largest_overlap(frame, &frames)
        .filter(|&index| frame.intersection_area(frames[index]) > 0.0)
        .map(|index| slots[index])
}

/// What a swap drop should do.
#[derive(Debug, Clone, PartialEq)]
pub enum SwapPlan {
    /// The destination is empty: snap normally.
    Snap,
    /// Exchange the swapping window with `occupant`, which moves to `source`: the source
    /// the swap was asked for, or the window's own frame when the zone under a floating
    /// window can't take the occupant.
    Exchange { occupant: WindowId, source: SwapSource },
    /// The drop can't swap; return the dragged window.
    Blocked(SwapBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapBlock {
    CrowdedDestination,
    SameZone,
    CrowdedSource,
}

impl SwapBlock {
    pub fn message(self) -> &'static str {
        match self {
            SwapBlock::CrowdedDestination => {
                "More than one window occupies that zone. Move one out before swapping."
            }
            SwapBlock::SameZone => "The window is already in that zone.",
            SwapBlock::CrowdedSource => {
                "Another window occupies the source zone. Move it out before swapping."
            }
        }
    }
}

/// Decide a swap of `window`, starting at `source`, into `destination`. `windows` are the
/// windows filling either place.
pub fn plan_swap(
    window: WindowId,
    source: &SwapSource,
    destination: &ZoneSlot,
    windows: &[WindowSnapshot],
) -> SwapPlan {
    let others: Vec<&WindowSnapshot> = windows.iter().filter(|other| other.id != window).collect();
    let occupants: Vec<&WindowSnapshot> = others
        .iter()
        .copied()
        .filter(|window| destination.is_occupied_by(window.frame))
        .collect();

    let occupant = match occupants.as_slice() {
        [] => return SwapPlan::Snap,
        [only] => *only,
        _ => return SwapPlan::Blocked(SwapBlock::CrowdedDestination),
    };
    let exchange = |source: SwapSource| SwapPlan::Exchange {
        occupant: occupant.id,
        source,
    };
    // A third window in the source zone, besides the occupant.
    let taken = |slot: &ZoneSlot| {
        others
            .iter()
            .any(|other| other.id != occupant.id && slot.is_occupied_by(other.frame))
    };
    match source {
        SwapSource::Floating { .. } => exchange(source.clone()),
        // A floating window never blocks a swap: when the zone under it can't take the
        // occupant, the occupant takes the window's own frame.
        SwapSource::NearestZone { zone, .. } if zone.is_same_zone(destination) || taken(zone) => {
            exchange(source.floating())
        }
        SwapSource::NearestZone { .. } => exchange(source.clone()),
        SwapSource::Zone(slot) if slot.is_same_zone(destination) => SwapPlan::Blocked(SwapBlock::SameZone),
        SwapSource::Zone(slot) if taken(slot) => SwapPlan::Blocked(SwapBlock::CrowdedSource),
        SwapSource::Zone(_) => exchange(source.clone()),
    }
}

/// What a keyboard move or a drop into a zone another window fills does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WhenTaken {
    /// Trade places with the window already there.
    #[default]
    Swap,
    /// Cover the window already there.
    Stack,
}

impl WhenTaken {
    /// True when the move swaps. `inverted` is true while the swap key is held during a
    /// drag, which does the opposite of the setting for that drop.
    pub fn swaps(self, inverted: bool) -> bool {
        (self == WhenTaken::Swap) != inverted
    }
}

/// The extra key that, held during a drag, does the opposite of [`WhenTaken`] for that drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SwapModifier {
    Meta,
    Ctrl,
    Alt,
    Shift,
    Off,
}

impl SwapModifier {
    /// The modifier actually used: the drag modifier can't double as the swap key, so a
    /// clash falls back to Meta.
    pub fn effective(self, drag: Modifier) -> SwapModifier {
        match self.modifier() {
            Some(modifier) if modifier == drag => SwapModifier::Meta,
            _ => self,
        }
    }

    pub fn modifier(self) -> Option<Modifier> {
        match self {
            SwapModifier::Meta => Some(Modifier::Meta),
            SwapModifier::Ctrl => Some(Modifier::Ctrl),
            SwapModifier::Alt => Some(Modifier::Alt),
            SwapModifier::Shift => Some(Modifier::Shift),
            SwapModifier::Off => None,
        }
    }

    /// True when `held` includes both the drag modifier and the effective swap modifier.
    pub fn is_held(self, held: Mods, drag: Modifier) -> bool {
        self.effective(drag)
            .modifier()
            .is_some_and(|swap| held.contains(drag) && held.contains(swap))
    }
}

fn is_real(frame: Rect) -> bool {
    frame.is_usable(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(zone: &str, x: f64, width: f64) -> ZoneSlot {
        ZoneSlot {
            display_id: "d".into(),
            zone_id: zone.into(),
            frame: Rect::new(x, 37.0, width, 851.0),
            work_area: Rect::new(0.0, 25.0, 1440.0, 875.0),
        }
    }

    fn window(id: u64, frame: Rect) -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId(id),
            frame,
        }
    }

    #[test]
    fn a_window_snapped_to_a_cell_grid_still_fills_its_zone() {
        // A terminal snapped into a 1264 × 1049 zone that landed 18 px narrower and 8 px
        // shorter, as Terminal does.
        let zone = ZoneSlot {
            display_id: "d".into(),
            zone_id: "bottom-left".into(),
            frame: Rect::new(12.0, 1099.0, 1264.0, 1049.0),
            work_area: Rect::new(0.0, 25.0, 3840.0, 2135.0),
        };
        assert!(zone.is_occupied_by(Rect::new(12.0, 1099.0, 1246.0, 1041.0)));
        assert!(zone.is_occupied_by(Rect::new(12.0, 1099.0, 1268.0, 1041.0)));
        assert!(
            !zone.is_occupied_by(Rect::new(30.0, 1099.0, 1246.0, 1041.0)),
            "the corner must still match closely"
        );
        assert!(
            !zone.is_occupied_by(Rect::new(12.0, 1099.0, 1200.0, 1049.0)),
            "a window well short of the zone isn't in it"
        );
    }

    #[test]
    fn floating_windows_do_not_occupy_and_cell_rounding_does() {
        let left = slot("l", 12.0, 702.0);
        assert!(left.is_occupied_by(Rect::new(14.0, 39.0, 698.0, 846.0)));
        assert!(!left.is_occupied_by(Rect::new(100.0, 100.0, 400.0, 400.0)));
    }

    const WORK: Rect = Rect::new(0.0, 25.0, 1440.0, 875.0);

    fn from(frame: Rect, slots: &[ZoneSlot]) -> SwapSource {
        SwapSource::find(frame, WORK, slots)
    }

    #[test]
    fn an_empty_destination_snaps() {
        let (left, right) = (slot("l", 12.0, 702.0), slot("r", 726.0, 702.0));
        let source = from(left.frame, &[left.clone(), right.clone()]);
        assert_eq!(plan_swap(WindowId(1), &source, &right, &[]), SwapPlan::Snap);
    }

    #[test]
    fn one_occupant_exchanges() {
        let (left, right) = (slot("l", 12.0, 702.0), slot("r", 726.0, 702.0));
        let source = from(left.frame, &[left.clone(), right.clone()]);
        assert_eq!(source, SwapSource::Zone(left.clone()));
        let windows = [window(1, left.frame), window(2, right.frame)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &right, &windows),
            SwapPlan::Exchange {
                occupant: WindowId(2),
                source,
            }
        );
    }

    #[test]
    fn crowded_zones_and_the_same_zone_block_the_swap() {
        let (left, right) = (slot("l", 12.0, 702.0), slot("r", 726.0, 702.0));
        let slots = [left.clone(), right.clone()];
        let source = from(left.frame, &slots);
        let crowded = [window(2, right.frame), window(3, right.frame)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &right, &crowded),
            SwapPlan::Blocked(SwapBlock::CrowdedDestination)
        );

        let stacked = [window(2, right.frame), window(3, left.frame)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &right, &stacked),
            SwapPlan::Blocked(SwapBlock::CrowdedSource)
        );

        let occupied = [window(2, left.frame)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &left, &occupied),
            SwapPlan::Blocked(SwapBlock::SameZone)
        );
    }

    /// Three side-by-side zones, as a thirds layout lays them out.
    fn thirds() -> [ZoneSlot; 3] {
        [
            slot("a", 12.0, 464.0),
            slot("b", 488.0, 464.0),
            slot("c", 964.0, 464.0),
        ]
    }

    #[test]
    fn a_floating_window_sends_the_occupant_to_the_zone_under_its_center() {
        let [a, b, c] = thirds();
        let floating = Rect::new(500.0, 300.0, 300.0, 200.0);
        let source = from(floating, &[a, b.clone(), c.clone()]);
        assert_eq!(
            source,
            SwapSource::NearestZone {
                zone: b.clone(),
                frame: floating
            }
        );
        assert_eq!(
            source.frame(),
            b.frame,
            "the occupant goes to the zone, sized to it"
        );
        assert!(source.starts_at(floating), "the window need not fill the zone");
        assert!(source.is_zone(&b));

        let windows = [window(2, c.frame)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &c, &windows),
            SwapPlan::Exchange {
                occupant: WindowId(2),
                source,
            }
        );
    }

    #[test]
    fn a_center_in_a_gap_takes_the_zone_the_window_overlaps_most() {
        let [a, b, c] = thirds();
        let slots = [a.clone(), b.clone(), c];
        // Both centers sit in the gap between a and b, from x 476 to 488.
        let mostly_b = Rect::new(334.0, 300.0, 300.0, 200.0);
        assert!(!a.frame.contains(mostly_b.center()) && !b.frame.contains(mostly_b.center()));
        assert_eq!(from(mostly_b, &slots).zone(), Some(&b));

        let mostly_a = Rect::new(330.0, 300.0, 300.0, 200.0);
        assert!(!a.frame.contains(mostly_a.center()) && !b.frame.contains(mostly_a.center()));
        assert_eq!(from(mostly_a, &slots).zone(), Some(&a));
    }

    #[test]
    fn a_window_centered_over_no_zone_of_its_display_keeps_its_own_frame() {
        let [a, b, c] = thirds();
        let off_the_work_area = Rect::new(500.0, 0.0, 300.0, 40.0);
        assert_eq!(
            from(off_the_work_area, &[a.clone(), b.clone(), c.clone()]),
            SwapSource::Floating {
                frame: off_the_work_area,
                work_area: WORK
            }
        );

        let other_display = ZoneSlot {
            display_id: "side".into(),
            work_area: Rect::new(1440.0, 25.0, 1920.0, 875.0),
            ..b
        };
        let floating = Rect::new(500.0, 300.0, 300.0, 200.0);
        assert_eq!(
            from(floating, &[other_display]),
            SwapSource::Floating {
                frame: floating,
                work_area: WORK
            },
            "zones on other displays don't count"
        );
        assert_eq!(
            from(floating, &[]),
            SwapSource::Floating {
                frame: floating,
                work_area: WORK
            },
            "a display with its layout off has no zones"
        );
    }

    #[test]
    fn the_occupant_takes_the_window_frame_when_the_zone_under_it_is_the_destination() {
        let [a, b, c] = thirds();
        let floating = Rect::new(500.0, 300.0, 300.0, 200.0);
        let source = from(floating, &[a, b.clone(), c]);
        let windows = [window(2, b.frame)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &b, &windows),
            SwapPlan::Exchange {
                occupant: WindowId(2),
                source: SwapSource::Floating {
                    frame: floating,
                    work_area: WORK
                },
            }
        );
    }

    #[test]
    fn the_occupant_takes_the_window_frame_when_a_third_window_fills_the_zone_under_it() {
        let [a, b, c] = thirds();
        let floating = Rect::new(500.0, 300.0, 300.0, 200.0);
        let source = from(floating, &[a, b.clone(), c.clone()]);
        let windows = [window(2, c.frame), window(3, b.frame)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &c, &windows),
            SwapPlan::Exchange {
                occupant: WindowId(2),
                source: SwapSource::Floating {
                    frame: floating,
                    work_area: WORK
                },
            },
            "never blocked"
        );
    }

    #[test]
    fn a_window_floating_over_no_zone_trades_places_with_the_occupant() {
        let (left, right) = (slot("l", 12.0, 702.0), slot("r", 726.0, 702.0));
        let floating = Rect::new(300.0, 0.0, 200.0, 30.0);
        let source = from(floating, &[left.clone(), right.clone()]);
        assert_eq!(source.kind(), "floating");
        assert!(source.is_filled_by(Rect::new(301.0, 1.0, 200.0, 29.0)));
        assert!(!source.is_zone(&left));
        let windows = [window(2, right.frame), window(3, floating)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &right, &windows),
            SwapPlan::Exchange {
                occupant: WindowId(2),
                source,
            }
        );
    }

    #[test]
    fn the_swap_key_is_extra_and_never_the_drag_key() {
        let held = Mods::from_iter([Modifier::Shift, Modifier::Meta]);
        assert!(SwapModifier::Meta.is_held(held, Modifier::Shift));
        assert!(!SwapModifier::Meta.is_held(Mods::from_iter([Modifier::Meta]), Modifier::Shift));
        assert_eq!(SwapModifier::Shift.effective(Modifier::Shift), SwapModifier::Meta);
        assert!(!SwapModifier::Off.is_held(held, Modifier::Shift));
    }

    #[test]
    fn the_swap_key_does_the_opposite_of_when_taken() {
        assert!(WhenTaken::Swap.swaps(false));
        assert!(!WhenTaken::Swap.swaps(true));
        assert!(!WhenTaken::Stack.swaps(false));
        assert!(WhenTaken::Stack.swaps(true));
        assert_eq!(WhenTaken::default(), WhenTaken::Swap);
    }

    #[test]
    fn the_source_is_the_only_zone_the_original_frame_fills_on_any_display() {
        let left = slot("l", 12.0, 702.0);
        let other_display = ZoneSlot {
            display_id: "side".into(),
            zone_id: "s".into(),
            frame: Rect::new(1452.0, 37.0, 900.0, 851.0),
            work_area: Rect::new(1440.0, 25.0, 1920.0, 875.0),
        };
        let slots = [left.clone(), other_display.clone()];
        assert_eq!(source_slot(other_display.frame, &slots), Some(&other_display));
        assert_eq!(source_slot(left.frame, &slots), Some(&left));
        assert_eq!(source_slot(Rect::new(300.0, 300.0, 200.0, 200.0), &slots), None);

        let same_frame_elsewhere = ZoneSlot {
            zone_id: "twin".into(),
            ..left.clone()
        };
        assert_eq!(
            source_slot(left.frame, &[left, same_frame_elsewhere]),
            None,
            "several matches mean no zone"
        );
    }
}
