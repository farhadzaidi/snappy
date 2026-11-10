//! Choosing the empty zone nearest a window, for the shortcut that moves the focused window
//! into one.

use crate::geometry::{Rect, distance};
use crate::swap::{ZoneSlot, fills_zone};
use crate::window::{WindowId, WindowSnapshot};

/// The empty zone nearest `window`, which sits at `frame` on the display `display_id`.
///
/// `slots` are the zones of every display's current layout, in display and zone order,
/// and `windows` the windows filling any of them. A zone is empty when no window fills
/// it, leaving out the moving window itself and windows `is_inactive` says belong to a
/// workspace that isn't showing. A zone the window already fills is never chosen.
///
/// Zones on the window's own display come first; other displays are considered only
/// when it has no empty zone. Nearest means the smallest distance between the window's
/// center and the zone's center, and the earlier zone wins a tie.
pub fn nearest_empty_zone<'a>(
    window: WindowId,
    frame: Rect,
    display_id: &str,
    slots: &'a [ZoneSlot],
    windows: &[WindowSnapshot],
    is_inactive: impl Fn(WindowId) -> bool,
) -> Option<&'a ZoneSlot> {
    let blocking: Vec<Rect> = windows
        .iter()
        .filter(|other| other.id != window && !is_inactive(other.id))
        .map(|other| other.frame)
        .collect();
    let is_empty = |slot: &&ZoneSlot| {
        !fills_zone(frame, slot.frame) && !blocking.iter().any(|other| fills_zone(*other, slot.frame))
    };
    let (own, elsewhere): (Vec<&ZoneSlot>, Vec<&ZoneSlot>) = slots
        .iter()
        .filter(is_empty)
        .partition(|slot| slot.display_id == display_id);
    nearest(frame, own).or_else(|| nearest(frame, elsewhere))
}

/// The slot whose center is closest to `frame`'s; `min_by` keeps the first of equals.
fn nearest(frame: Rect, slots: Vec<&ZoneSlot>) -> Option<&ZoneSlot> {
    let center = frame.center();
    let away = |slot: &ZoneSlot| {
        let zone = slot.frame.center();
        distance(zone.x - center.x, zone.y - center.y)
    };
    slots.into_iter().min_by(|a, b| away(a).total_cmp(&away(b)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: Rect = Rect::new(0.0, 0.0, 1200.0, 800.0);
    const SIDE: Rect = Rect::new(1200.0, 0.0, 1200.0, 800.0);
    const MOVING: WindowId = WindowId(1);

    /// Three side-by-side columns on a display.
    fn columns(display_id: &str, area: Rect) -> Vec<ZoneSlot> {
        let width = area.width / 3.0;
        (0..3)
            .map(|index| ZoneSlot {
                display_id: display_id.to_owned(),
                zone_id: format!("{display_id}-{index}"),
                frame: Rect::new(area.x + width * index as f64, area.y, width, area.height),
                work_area: area,
            })
            .collect()
    }

    fn two_displays() -> Vec<ZoneSlot> {
        let mut slots = columns("main", MAIN);
        slots.extend(columns("side", SIDE));
        slots
    }

    fn window(id: u64, frame: Rect) -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId(id),
            frame,
        }
    }

    fn chosen(frame: Rect, slots: &[ZoneSlot], windows: &[WindowSnapshot]) -> Option<String> {
        nearest_empty_zone(MOVING, frame, "main", slots, windows, |_| false).map(|slot| slot.zone_id.clone())
    }

    #[test]
    fn a_floating_window_goes_to_the_nearest_empty_zone_on_its_display() {
        let slots = two_displays();
        let near_right = Rect::new(900.0, 300.0, 200.0, 200.0);
        let in_middle = window(2, slots[1].frame);
        assert_eq!(
            chosen(near_right, &slots, &[in_middle]).as_deref(),
            Some("main-2")
        );

        let in_right = window(3, slots[2].frame);
        assert_eq!(
            chosen(near_right, &slots, &[window(2, slots[1].frame), in_right]).as_deref(),
            Some("main-0"),
            "the right and middle zones are taken, so the left one is nearest"
        );
    }

    #[test]
    fn a_full_display_falls_back_to_the_nearest_zone_elsewhere() {
        let slots = two_displays();
        let full: Vec<WindowSnapshot> = (0..3)
            .map(|index| window(10 + index, slots[index as usize].frame))
            .collect();
        let near_left = Rect::new(50.0, 300.0, 200.0, 200.0);
        assert_eq!(chosen(near_left, &slots, &full).as_deref(), Some("side-0"));

        let mut side_left_taken = full.clone();
        side_left_taken.push(window(20, slots[3].frame));
        assert_eq!(
            chosen(near_left, &slots, &side_left_taken).as_deref(),
            Some("side-1")
        );
    }

    #[test]
    fn a_window_in_a_zone_moves_to_another_empty_zone() {
        let slots = columns("main", MAIN);
        let in_middle = slots[1].frame;
        let moving_itself = window(MOVING.0, in_middle);
        let chosen = chosen(in_middle, &slots, &[moving_itself]);
        assert_eq!(
            chosen.as_deref(),
            Some("main-0"),
            "its own zone isn't a candidate, and the left one wins the tie"
        );
    }

    #[test]
    fn nothing_is_chosen_when_every_zone_is_full() {
        let slots = two_displays();
        let everywhere: Vec<WindowSnapshot> = slots
            .iter()
            .enumerate()
            .map(|(index, slot)| window(10 + index as u64, slot.frame))
            .collect();
        let floating = Rect::new(500.0, 300.0, 200.0, 200.0);
        assert_eq!(chosen(floating, &slots, &everywhere), None);

        let alone = columns("main", MAIN);
        let full: Vec<WindowSnapshot> = alone
            .iter()
            .enumerate()
            .map(|(index, slot)| window(10 + index as u64, slot.frame))
            .collect();
        assert_eq!(chosen(floating, &alone, &full), None);
    }

    #[test]
    fn windows_of_inactive_workspaces_never_block_a_zone() {
        let slots = columns("main", MAIN);
        let near_right = Rect::new(900.0, 300.0, 200.0, 200.0);
        let parked_in_right = window(7, slots[2].frame);
        let inactive = |id: WindowId| id == WindowId(7);
        let chosen = nearest_empty_zone(MOVING, near_right, "main", &slots, &[parked_in_right], inactive);
        assert_eq!(chosen.map(|slot| slot.zone_id.as_str()), Some("main-2"));
    }

    #[test]
    fn a_tie_goes_to_the_earlier_zone() {
        let slots = columns("main", MAIN);
        let between_left_and_right = Rect::new(500.0, 300.0, 200.0, 200.0);
        let in_middle = window(2, slots[1].frame);
        assert_eq!(
            chosen(between_left_and_right, &slots, &[in_middle]).as_deref(),
            Some("main-0")
        );
    }

    #[test]
    fn a_window_stacked_over_another_still_counts_the_zone_as_taken() {
        let slots = columns("main", MAIN);
        let in_left = slots[0].frame;
        let both_in_left = [window(MOVING.0, in_left), window(2, in_left)];
        assert_eq!(chosen(in_left, &slots, &both_in_left).as_deref(), Some("main-1"));
    }
}
