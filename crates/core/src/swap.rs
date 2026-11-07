//! Swapping two windows between zones, or a floating window with a zone's occupant.

use crate::geometry::Rect;
use crate::keys::{Modifier, Mods};
use crate::window::{WindowId, WindowSnapshot};

/// Largest edge mismatch, in points, for a window to count as filling a zone.
const OCCUPANCY_TOLERANCE: f64 = 12.0;
/// The tolerance never exceeds this fraction of the zone's shorter side.
const OCCUPANCY_FRACTION: f64 = 0.04;

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
        fills(frame, self.frame)
    }
}

/// True when `frame` matches `target` on all four edges. Small mismatches from terminal
/// cell grids and app borders still count.
fn fills(frame: Rect, target: Rect) -> bool {
    if !is_real(frame) || !is_real(target) {
        return false;
    }
    let tolerance = OCCUPANCY_TOLERANCE.min(target.width.min(target.height) * OCCUPANCY_FRACTION);
    [
        (frame.min_x(), target.min_x()),
        (frame.min_y(), target.min_y()),
        (frame.max_x(), target.max_x()),
        (frame.max_y(), target.max_y()),
    ]
    .iter()
    .all(|(a, b)| (a - b).abs() <= tolerance)
}

/// Where the swapping window starts, and so where the occupant goes.
#[derive(Debug, Clone, PartialEq)]
pub enum SwapSource {
    /// The window fills this zone.
    Zone(ZoneSlot),
    /// The window floats at `frame`, in no zone.
    Floating { frame: Rect, work_area: Rect },
}

impl SwapSource {
    /// The source for a window at `frame` on a display with `work_area`.
    pub fn find(frame: Rect, work_area: Rect, slots: &[ZoneSlot]) -> SwapSource {
        match source_slot(frame, slots) {
            Some(slot) => SwapSource::Zone(slot.clone()),
            None => SwapSource::Floating { frame, work_area },
        }
    }

    pub fn frame(&self) -> Rect {
        match self {
            SwapSource::Zone(slot) => slot.frame,
            SwapSource::Floating { frame, .. } => *frame,
        }
    }

    pub fn work_area(&self) -> Rect {
        match self {
            SwapSource::Zone(slot) => slot.work_area,
            SwapSource::Floating { work_area, .. } => *work_area,
        }
    }

    /// True when a window with `frame` fills the source.
    pub fn is_filled_by(&self, frame: Rect) -> bool {
        fills(frame, self.frame())
    }

    pub fn is_zone(&self, slot: &ZoneSlot) -> bool {
        matches!(self, SwapSource::Zone(own) if own.is_same_zone(slot))
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

/// What a swap drop should do.
#[derive(Debug, Clone, PartialEq)]
pub enum SwapPlan {
    /// The destination is empty: snap normally.
    Snap,
    /// Exchange the swapping window with `occupant`, which moves to the source.
    Exchange { occupant: WindowId },
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
    let SwapSource::Zone(slot) = source else {
        return SwapPlan::Exchange {
            occupant: occupant.id,
        };
    };
    if slot.is_same_zone(destination) {
        return SwapPlan::Blocked(SwapBlock::SameZone);
    }
    let source_crowded = others
        .iter()
        .any(|other| other.id != occupant.id && slot.is_occupied_by(other.frame));
    if source_crowded {
        return SwapPlan::Blocked(SwapBlock::CrowdedSource);
    }
    SwapPlan::Exchange {
        occupant: occupant.id,
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
                occupant: WindowId(2)
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

    #[test]
    fn a_floating_window_trades_places_with_the_occupant() {
        let (left, right) = (slot("l", 12.0, 702.0), slot("r", 726.0, 702.0));
        let floating = Rect::new(300.0, 300.0, 200.0, 200.0);
        let source = from(floating, &[left.clone(), right.clone()]);
        assert_eq!(
            source,
            SwapSource::Floating {
                frame: floating,
                work_area: WORK
            }
        );
        assert!(source.is_filled_by(Rect::new(301.0, 299.0, 200.0, 201.0)));
        assert!(!source.is_zone(&left));
        let windows = [window(2, right.frame), window(3, floating)];
        assert_eq!(
            plan_swap(WindowId(1), &source, &right, &windows),
            SwapPlan::Exchange {
                occupant: WindowId(2)
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
