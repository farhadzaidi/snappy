//! Choosing the window to focus in a direction, for the shortcuts that move focus between
//! windows.

use crate::geometry::{Point, Rect};
use crate::navigate::{Direction, next_zone, starting_zone};
use crate::swap::{ZoneSlot, fills_zone};
use crate::window::{WindowId, WindowSnapshot};

/// How focus chose where to go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    /// Between the zones windows fill, as the arrows move a window.
    Zones,
    /// Between all windows by their centers, when no window is in a zone.
    Windows,
}

impl FocusMode {
    /// The mode's name in the debug log.
    pub fn name(self) -> &'static str {
        match self {
            FocusMode::Zones => "zones",
            FocusMode::Windows => "windows",
        }
    }
}

/// Where focus goes, and the zone that led there when it moves between zones.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusChoice<'a> {
    pub mode: FocusMode,
    pub window: Option<WindowId>,
    pub zone: Option<&'a ZoneSlot>,
}

/// The window to focus going `direction`.
///
/// `focused` is the focused window's frame when it is on the active workspace, and `from`
/// is where to start when it isn't in a zone: its center, or the center of the display
/// under the pointer when nothing is focused. `slots` are the zones of every display's
/// current layout, and `candidates` the windows focus can move to, on the active workspace,
/// most recently focused first.
///
/// While any of those windows, the focused one included, fills a zone, focus moves
/// between filled zones only, on any display: from the zone the focused window is in, or
/// from `from`, to the next zone the arrows would choose among the filled ones, skipping
/// empty zones and windows outside zones. Of several windows in that zone, the earliest
/// candidate wins. Otherwise focus goes to the nearest window by [`window_toward`].
pub fn focus_target<'a>(
    focused: Option<Rect>,
    from: Point,
    direction: Direction,
    slots: &'a [ZoneSlot],
    candidates: &[WindowSnapshot],
) -> FocusChoice<'a> {
    let own: Vec<usize> = (0..slots.len())
        .filter(|&index| focused.is_some_and(|frame| fills_zone(frame, slots[index].frame)))
        .collect();
    let occupied: Vec<usize> = (0..slots.len())
        .filter(|&index| {
            candidates
                .iter()
                .any(|window| fills_zone(window.frame, slots[index].frame))
        })
        .collect();
    if own.is_empty() && occupied.is_empty() {
        return FocusChoice {
            mode: FocusMode::Windows,
            window: window_toward(from, direction, candidates),
            zone: None,
        };
    }

    let start = focused.and_then(|window| Some((window, start_zone(window, &own, slots)?)));
    let (origin, anchor, start) = match start {
        Some((window, index)) => {
            let origin = slots[index].frame;
            let (low, high) = direction.cross_span(origin);
            (
                origin,
                direction.across(window.center()).clamp(low, high),
                Some(index),
            )
        }
        None => (Rect::new(from.x, from.y, 0.0, 0.0), direction.across(from), None),
    };
    let targets = occupied
        .into_iter()
        .filter(|&index| Some(index) != start)
        .map(|index| (index, slots[index].frame));
    let zone = next_zone(direction, origin, anchor, targets).map(|index| &slots[index]);
    let window = zone.and_then(|slot| {
        candidates
            .iter()
            .find(|window| fills_zone(window.frame, slot.frame))
            .map(|window| window.id)
    });
    FocusChoice {
        mode: FocusMode::Zones,
        window,
        zone,
    }
}

/// Of the zones `own` the focused window fills, the one it overlaps most, as the arrows
/// start.
fn start_zone(focused: Rect, own: &[usize], slots: &[ZoneSlot]) -> Option<usize> {
    let frames: Vec<Rect> = own.iter().map(|&index| slots[index].frame).collect();
    starting_zone(focused, &frames).map(|position| own[position])
}

/// A window's sideways offset counts this many times its distance ahead, so a window
/// straight ahead wins over one as far off to the side: one at 45 degrees must be three
/// times nearer to win.
pub const SIDEWAYS_WEIGHT: f64 = 2.0;
/// A window counts as ahead only when its center is further than this past the start.
const AHEAD_THRESHOLD: f64 = 0.5;
/// Scores closer than this tie.
const TIE_TOLERANCE: f64 = 0.001;

/// The window to focus going `direction` from `from`, the focused window's center or the
/// center of the display under the pointer.
///
/// Only windows whose centers lie past `from` in that direction count, and the nearest
/// wins: the least distance ahead plus `SIDEWAYS_WEIGHT` times the distance to the side.
/// `candidates` come in the order ties go, most recently focused first. Nothing is chosen
/// when nothing lies that way; focus never wraps around.
pub fn window_toward(from: Point, direction: Direction, candidates: &[WindowSnapshot]) -> Option<WindowId> {
    let mut best: Option<(f64, WindowId)> = None;
    for candidate in candidates {
        let center = candidate.frame.center();
        let ahead = (direction.along(center) - direction.along(from)) * direction.sign();
        if ahead <= AHEAD_THRESHOLD {
            continue;
        }
        let sideways = (direction.across(center) - direction.across(from)).abs();
        let score = ahead + SIDEWAYS_WEIGHT * sideways;
        if best.is_none_or(|(best_score, _)| score < best_score - TIE_TOLERANCE) {
            best = Some((score, candidate.id));
        }
    }
    best.map(|(_, window)| window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Layout, Spacing, UnitRect, Zone, preset, zone_frame};
    use crate::navigate::destination;

    /// A 200 by 100 window centered on `(x, y)`.
    fn at(id: u64, x: f64, y: f64) -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId(id),
            frame: Rect::new(x - 100.0, y - 50.0, 200.0, 100.0),
        }
    }

    const FROM: Point = Point::new(500.0, 500.0);

    fn toward(direction: Direction, candidates: &[WindowSnapshot]) -> Option<u64> {
        window_toward(FROM, direction, candidates).map(|window| window.0)
    }

    #[test]
    fn each_direction_picks_the_window_that_way() {
        let around = [
            at(1, 500.0, 200.0),
            at(2, 200.0, 500.0),
            at(3, 500.0, 800.0),
            at(4, 800.0, 500.0),
        ];
        assert_eq!(toward(Direction::Up, &around), Some(1));
        assert_eq!(toward(Direction::Left, &around), Some(2));
        assert_eq!(toward(Direction::Down, &around), Some(3));
        assert_eq!(toward(Direction::Right, &around), Some(4));
    }

    #[test]
    fn a_window_straight_ahead_beats_a_nearer_one_off_to_the_side() {
        let straight = at(1, 900.0, 500.0);
        let diagonal = at(2, 700.0, 700.0);
        assert_eq!(toward(Direction::Right, &[diagonal, straight]), Some(1));
        let close_and_slightly_off = at(3, 600.0, 560.0);
        assert_eq!(
            toward(Direction::Right, &[straight, close_and_slightly_off]),
            Some(3),
            "a little off to the side but much nearer still wins"
        );
    }

    #[test]
    fn only_windows_past_the_start_count_and_focus_never_wraps() {
        let level_with_it = at(1, 500.0, 900.0);
        let behind = at(2, 300.0, 500.0);
        assert_eq!(toward(Direction::Right, &[level_with_it, behind]), None);
        assert_eq!(toward(Direction::Right, &[]), None);
    }

    #[test]
    fn a_tie_goes_to_the_earlier_candidate_the_most_recently_focused() {
        let above = at(7, 800.0, 300.0);
        let below = at(8, 800.0, 700.0);
        assert_eq!(toward(Direction::Right, &[above, below]), Some(7));
        assert_eq!(toward(Direction::Right, &[below, above]), Some(8));
    }

    const MAIN: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);
    const SIDE: Rect = Rect::new(1440.0, 0.0, 1920.0, 1080.0);

    /// The zones of `layout` on the display with `work_area`, named by display.
    fn slots_of(display: &str, layout: &Layout, work_area: Rect) -> Vec<ZoneSlot> {
        layout
            .zones
            .iter()
            .map(|zone| ZoneSlot {
                display_id: display.into(),
                zone_id: zone.id.clone(),
                frame: zone_frame(zone.rect, work_area, Spacing::STANDARD),
                work_area,
            })
            .collect()
    }

    fn slots(layout: &str) -> Vec<ZoneSlot> {
        slots_of("main", preset(layout).unwrap(), MAIN)
    }

    fn in_zone(id: u64, slot: &ZoneSlot) -> WindowSnapshot {
        WindowSnapshot {
            id: WindowId(id),
            frame: slot.frame,
        }
    }

    /// Where focus goes from the window filling `slot`, and through which zone.
    fn from_zone(
        slot: &ZoneSlot,
        direction: Direction,
        slots: &[ZoneSlot],
        candidates: &[WindowSnapshot],
    ) -> (Option<u64>, Option<String>) {
        let choice = focus_target(
            Some(slot.frame),
            slot.frame.center(),
            direction,
            slots,
            candidates,
        );
        assert_eq!(choice.mode, FocusMode::Zones);
        named(&choice)
    }

    fn named(choice: &FocusChoice) -> (Option<u64>, Option<String>) {
        (
            choice.window.map(|window| window.0),
            choice.zone.map(|slot| slot.zone_id.clone()),
        )
    }

    #[test]
    fn a_floating_window_between_two_zoned_windows_is_skipped() {
        let halves = slots("halves");
        let (left, right) = (&halves[0], &halves[1]);
        let between = at(3, 720.0, 450.0);
        let candidates = [between, in_zone(2, right)];
        assert_eq!(
            window_toward(left.frame.center(), Direction::Right, &candidates),
            Some(WindowId(3)),
            "by center distance the floating window would win"
        );
        assert_eq!(
            from_zone(left, Direction::Right, &halves, &candidates),
            (Some(2), Some("halves-1".into()))
        );
        assert_eq!(
            from_zone(left, Direction::Down, &halves, &candidates),
            (None, None),
            "the floating window below isn't reachable either"
        );
    }

    #[test]
    fn an_empty_zone_is_skipped_for_the_next_filled_one() {
        let thirds = slots("thirds");
        let candidates = [in_zone(3, &thirds[2])];
        assert_eq!(
            from_zone(&thirds[0], Direction::Right, &thirds, &candidates),
            (Some(3), Some("thirds-2".into()))
        );
    }

    #[test]
    fn of_several_windows_in_one_zone_the_most_recently_focused_wins() {
        let halves = slots("halves");
        let (older, newer) = (in_zone(2, &halves[1]), in_zone(3, &halves[1]));
        assert_eq!(
            from_zone(&halves[0], Direction::Right, &halves, &[newer, older]).0,
            Some(3)
        );
        assert_eq!(
            from_zone(&halves[0], Direction::Right, &halves, &[older, newer]).0,
            Some(2)
        );
    }

    #[test]
    fn a_floating_focused_window_starts_from_its_center_toward_the_nearest_filled_zone() {
        let thirds = slots("thirds");
        let candidates = [in_zone(1, &thirds[0]), in_zone(3, &thirds[2])];
        let floating = Rect::new(500.0, 300.0, 300.0, 200.0);
        let go = |direction| {
            named(&focus_target(
                Some(floating),
                floating.center(),
                direction,
                &thirds,
                &candidates,
            ))
        };
        assert_eq!(go(Direction::Right), (Some(3), Some("thirds-2".into())));
        assert_eq!(go(Direction::Left), (Some(1), Some("thirds-0".into())));
        assert_eq!(go(Direction::Up), (None, None));
    }

    #[test]
    fn from_a_point_a_zone_beside_it_beats_a_nearer_one_off_to_the_side() {
        let layout = custom(&[
            ("diagonal", UnitRect::new(0.4, 0.6, 0.15, 0.4)),
            ("beside", UnitRect::new(0.8, 0.3, 0.2, 0.3)),
        ]);
        let zones = slots_of("main", &layout, MAIN);
        let candidates = [in_zone(1, &zones[0]), in_zone(2, &zones[1])];
        let from = Point::new(400.0, 400.0);
        let choice = focus_target(None, from, Direction::Right, &zones, &candidates);
        assert_eq!(named(&choice), (Some(2), Some("beside".into())));
    }

    #[test]
    fn focus_crosses_onto_another_display_skipping_its_empty_zones() {
        let mut zones = slots("halves");
        zones.extend(slots_of("side", preset("thirds").unwrap(), SIDE));
        let candidates = [in_zone(1, &zones[0]), in_zone(4, &zones[4])];
        let choice = focus_target(
            Some(zones[1].frame),
            zones[1].frame.center(),
            Direction::Right,
            &zones,
            &candidates,
        );
        assert_eq!(choice.window, Some(WindowId(4)));
        assert_eq!(choice.zone.map(|slot| slot.display_id.as_str()), Some("side"));
        assert_eq!(
            from_zone(&zones[4], Direction::Left, &zones, &candidates),
            (Some(1), Some("halves-0".into())),
            "back past the empty zones on both displays"
        );
    }

    #[test]
    fn with_no_window_in_a_zone_focus_goes_by_window_centers() {
        let halves = slots("halves");
        let around = [at(1, 500.0, 200.0), at(4, 800.0, 500.0)];
        let choice = focus_target(None, FROM, Direction::Right, &halves, &around);
        assert_eq!(choice.mode, FocusMode::Windows);
        assert_eq!(choice.window, Some(WindowId(4)));
        assert_eq!(choice.zone, None);
    }

    #[test]
    fn a_zoned_focused_window_alone_keeps_focus_between_zones() {
        let halves = slots("halves");
        let floating = [at(2, 1100.0, 450.0)];
        assert_eq!(
            from_zone(&halves[0], Direction::Right, &halves, &floating),
            (None, None)
        );
    }

    fn custom(zones: &[(&str, UnitRect)]) -> Layout {
        Layout {
            id: "t".into(),
            name: "t".into(),
            zones: zones.iter().map(|(id, rect)| Zone::new(*id, *rect)).collect(),
        }
    }

    #[test]
    fn with_every_zone_filled_focus_goes_where_the_arrows_move_a_window() {
        let layouts = [
            preset("quarters").unwrap().clone(),
            preset("wide-left").unwrap().clone(),
            preset("thirds").unwrap().clone(),
            custom(&[
                ("a", UnitRect::new(0.0, 0.0, 0.3, 0.5)),
                ("diagonal", UnitRect::new(0.3, 0.5, 0.2, 0.5)),
                ("aligned", UnitRect::new(0.6, 0.0, 0.4, 0.5)),
            ]),
            custom(&[
                ("a", UnitRect::new(0.0, 0.3, 0.3, 0.4)),
                ("below", UnitRect::new(0.5, 0.75, 0.3, 0.25)),
                ("above", UnitRect::new(0.5, 0.0, 0.3, 0.25)),
            ]),
        ];
        for layout in &layouts {
            let zones = slots_of("main", layout, MAIN);
            let candidates: Vec<WindowSnapshot> = (0..zones.len())
                .map(|index| in_zone(index as u64, &zones[index]))
                .collect();
            for (index, slot) in zones.iter().enumerate() {
                let others: Vec<WindowSnapshot> = candidates
                    .iter()
                    .filter(|window| window.id.0 != index as u64)
                    .copied()
                    .collect();
                for direction in Direction::ALL {
                    let arrows = destination(slot.frame, layout, MAIN, Spacing::STANDARD, direction, None)
                        .map(|zone| zone.id.clone());
                    assert_eq!(
                        from_zone(slot, direction, &zones, &others).1,
                        arrows,
                        "{} from {} going {}",
                        layout.id,
                        slot.zone_id,
                        direction.name()
                    );
                }
            }
        }
    }
}
