//! Empty zones for the shortcut that fills them: the one nearest the focused window, or
//! one each for the selected windows.

use crate::geometry::{Rect, distance};
use crate::placement::assignment::{FORBIDDEN, cheapest_assignment};
use crate::placement::swap::{ZoneSlot, fills_zone};
use crate::window::{WindowId, WindowSnapshot};

/// Distances are compared in thousandths of a point, so equal layouts tie exactly.
const DISTANCE_UNITS: f64 = 1000.0;
/// Puts the distance ahead of the tie-break in a combined cost: no sum of tie-breaks
/// reaches it.
const TIE_BREAK_SCALE: i128 = 1 << 40;

/// The empty zone nearest `window`, which sits at `frame` on the display `display_id`.
///
/// `slots` are the zones of every display's current layout, in display and zone order,
/// and `windows` the windows filling any of them that count, which leaves out windows of
/// workspaces that aren't showing. A zone is empty when no window but the moving one
/// fills it. A zone the window already fills is never chosen.
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
) -> Option<&'a ZoneSlot> {
    let blocking: Vec<Rect> = windows
        .iter()
        .filter(|other| other.id != window)
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

/// Where the selected windows go: each into an empty zone of its own.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionPlan<'a> {
    /// Each window that moves and its zone, in selection order.
    pub moves: Vec<(WindowId, &'a ZoneSlot)>,
    /// Selected windows already filling a zone, which stay where they are.
    pub in_zones: Vec<WindowId>,
    /// Windows no empty zone was left for, in selection order.
    pub left_over: Vec<WindowId>,
}

/// Plan putting the `selected` windows, in selection order, into empty zones.
///
/// Selected windows already filling a zone stay. The others each get an empty zone of
/// their own. Empty means what it means for `nearest_empty_zone`: no window of `windows`,
/// those filling any of `slots` that count, fills it.
///
/// Zones on `first_display`, the display under the pointer, are used first; other
/// displays' zones, in slot order, only when it hasn't enough. Among the zones that may be
/// used, windows and zones are paired so that the total distance from each window's center
/// to its zone's center is the least. With more windows than empty zones every zone is
/// filled, and the windows that would add most to the total are left over. On equal totals
/// earlier selected windows take earlier zones, and later ones are left over.
pub fn place_selection<'a>(
    selected: &[WindowSnapshot],
    first_display: &str,
    slots: &'a [ZoneSlot],
    windows: &[WindowSnapshot],
) -> SelectionPlan<'a> {
    let (in_zones, moving): (Vec<&WindowSnapshot>, Vec<&WindowSnapshot>) = selected
        .iter()
        .partition(|window| slots.iter().any(|slot| fills_zone(window.frame, slot.frame)));
    let blocking: Vec<Rect> = windows
        .iter()
        .chain(in_zones.iter().copied())
        .map(|other| other.frame)
        .collect();
    let empty: Vec<&ZoneSlot> = slots
        .iter()
        .filter(|slot| !blocking.iter().any(|other| fills_zone(*other, slot.frame)))
        .collect();
    let (zones, required) = candidate_zones(&empty, first_display, moving.len());
    let assigned = assign(&moving, &zones, required);

    let mut plan = SelectionPlan {
        moves: Vec::new(),
        in_zones: in_zones.iter().map(|window| window.id).collect(),
        left_over: Vec::new(),
    };
    for (window, zone) in moving.iter().zip(assigned) {
        match zone {
            Some(index) => plan.moves.push((window.id, zones[index])),
            None => plan.left_over.push(window.id),
        }
    }
    plan
}

/// The empty zones `count` windows may use, display by display with `first_display`
/// first, until there are enough. Returns them in that order, with how many of the first
/// must all be filled: every zone but those of the last display taken, which may have more
/// than are needed.
fn candidate_zones<'a>(
    empty: &[&'a ZoneSlot],
    first_display: &str,
    count: usize,
) -> (Vec<&'a ZoneSlot>, usize) {
    let mut displays: Vec<&str> = vec![first_display];
    for slot in empty {
        if !displays.contains(&slot.display_id.as_str()) {
            displays.push(&slot.display_id);
        }
    }
    let mut zones = Vec::new();
    let mut required = 0;
    for display in displays {
        if zones.len() >= count {
            break;
        }
        required = zones.len();
        zones.extend(empty.iter().copied().filter(|slot| slot.display_id == display));
    }
    (zones, required)
}

/// The index in `zones` of each moving window's zone, or `None` for a window left over.
/// Rows past the windows stand for zones left empty, which only the zones after the first
/// `required` may be; columns past the zones stand for windows left over, after every
/// zone in the tie-break.
fn assign(moving: &[&WindowSnapshot], zones: &[&ZoneSlot], required: usize) -> Vec<Option<usize>> {
    let size = moving.len().max(zones.len());
    let tie_break = |row: usize, column: usize| ((moving.len() - row) * column) as i128;
    let costs: Vec<Vec<i128>> = (0..size)
        .map(|row| {
            (0..size)
                .map(|column| match (moving.get(row), zones.get(column)) {
                    (Some(window), Some(zone)) => {
                        between_centers(window.frame, zone.frame) * TIE_BREAK_SCALE + tie_break(row, column)
                    }
                    (Some(_), None) => tie_break(row, column),
                    (None, Some(_)) if column < required => FORBIDDEN,
                    (None, _) => 0,
                })
                .collect()
        })
        .collect();
    let columns = cheapest_assignment(&costs);
    (0..moving.len())
        .map(|row| columns.get(row).copied().filter(|column| *column < zones.len()))
        .collect()
}

/// The distance between the frames' centers, in whole distance units.
fn between_centers(window: Rect, zone: Rect) -> i128 {
    let (from, to) = (window.center(), zone.center());
    (distance(to.x - from.x, to.y - from.y) * DISTANCE_UNITS).round() as i128
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
        nearest_empty_zone(MOVING, frame, "main", slots, windows).map(|slot| slot.zone_id.clone())
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

    /// Each moved window's zone, by ID, in selection order.
    fn placed(plan: &SelectionPlan) -> Vec<(u64, String)> {
        plan.moves
            .iter()
            .map(|(window, slot)| (window.0, slot.zone_id.clone()))
            .collect()
    }

    fn plan<'a>(
        selected: &[WindowSnapshot],
        slots: &'a [ZoneSlot],
        windows: &[WindowSnapshot],
    ) -> SelectionPlan<'a> {
        place_selection(selected, "main", slots, windows)
    }

    /// A small floating window centered on `x`, halfway down the main display.
    fn floating_at(id: u64, x: f64) -> WindowSnapshot {
        window(id, Rect::new(x - 50.0, 350.0, 100.0, 100.0))
    }

    #[test]
    fn two_floating_windows_take_the_empty_zones_that_keep_the_total_distance_least() {
        let slots = columns("main", MAIN);
        let in_middle = window(9, slots[1].frame);
        // Greedy would give window 1 the left zone, its nearest, and send window 2 across the
        // display; trading costs window 1 a little and saves window 2 a lot.
        let selected = [floating_at(1, 300.0), floating_at(2, 50.0)];
        let plan = plan(&selected, &slots, &[in_middle]);
        assert_eq!(
            placed(&plan),
            [(1, "main-2".to_owned()), (2, "main-0".to_owned())]
        );
        assert!(plan.in_zones.is_empty() && plan.left_over.is_empty());
    }

    #[test]
    fn selected_windows_already_in_zones_stay_and_keep_their_zones() {
        let slots = columns("main", MAIN);
        let zoned = window(1, slots[0].frame);
        let selected = [zoned, floating_at(2, 100.0)];
        let plan = plan(&selected, &slots, &[zoned]);
        assert_eq!(plan.in_zones, [WindowId(1)]);
        assert_eq!(placed(&plan), [(2, "main-1".to_owned())]);
    }

    #[test]
    fn more_windows_than_empty_zones_fill_them_and_leave_the_farthest_over() {
        let slots = columns("main", MAIN);
        let in_left = window(9, slots[0].frame);
        let selected = [
            floating_at(1, 600.0),
            floating_at(2, 50.0),
            floating_at(3, 1000.0),
        ];
        let plan = plan(&selected, &slots, &[in_left]);
        assert_eq!(
            placed(&plan),
            [(1, "main-1".to_owned()), (3, "main-2".to_owned())]
        );
        assert_eq!(plan.left_over, [WindowId(2)]);
    }

    #[test]
    fn the_display_under_the_pointer_is_filled_before_others() {
        let slots = two_displays();
        // Both windows sit on the side display, but the pointer's display comes first.
        let selected = [
            window(1, Rect::new(1300.0, 300.0, 100.0, 100.0)),
            window(2, Rect::new(2200.0, 300.0, 100.0, 100.0)),
        ];
        let plan = plan(&selected, &slots, &[window(9, slots[1].frame)]);
        let mut zones: Vec<String> = placed(&plan).into_iter().map(|(_, zone)| zone).collect();
        zones.sort();
        assert_eq!(zones, ["main-0", "main-2"]);

        let crowded = [window(9, slots[0].frame), window(10, slots[1].frame)];
        let plan = place_selection(&selected, "main", &slots, &crowded);
        let zones: Vec<String> = placed(&plan).into_iter().map(|(_, zone)| zone).collect();
        assert!(
            zones.contains(&"main-2".to_owned()),
            "the pointer's last empty zone is used"
        );
        assert_eq!(
            placed(&plan)[1],
            (2, "side-2".to_owned()),
            "the rest go to the nearest elsewhere"
        );

        let from_side = place_selection(&selected, "side", &slots, &[]);
        assert_eq!(
            placed(&from_side),
            [(1, "side-0".to_owned()), (2, "side-2".to_owned())]
        );
    }

    #[test]
    fn equal_distances_give_earlier_windows_earlier_zones_every_time() {
        let slots = columns("main", MAIN);
        let center = Rect::new(550.0, 350.0, 100.0, 100.0);
        let selected = [window(1, center), window(2, center), window(3, center)];
        let middle_taken = [window(9, slots[1].frame)];
        for _ in 0..3 {
            let plan = plan(&selected, &slots, &middle_taken);
            assert_eq!(
                placed(&plan),
                [(1, "main-0".to_owned()), (2, "main-2".to_owned())]
            );
            assert_eq!(plan.left_over, [WindowId(3)]);
        }
    }

    #[test]
    fn an_empty_selection_or_no_empty_zone_moves_nothing() {
        let slots = columns("main", MAIN);
        assert!(plan(&[], &slots, &[]).moves.is_empty());
        let full: Vec<WindowSnapshot> = slots
            .iter()
            .enumerate()
            .map(|(index, slot)| window(10 + index as u64, slot.frame))
            .collect();
        let plan = plan(&[floating_at(1, 300.0)], &slots, &full);
        assert!(plan.moves.is_empty());
        assert_eq!(plan.left_over, [WindowId(1)]);
    }
}
