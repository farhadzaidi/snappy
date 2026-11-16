//! Property tests: any sequence of editor operations on a sound layout leaves it sound.

use proptest::prelude::*;

use super::{
    Axis, CutScope, ResizeHandle, Side, cut, draw, is_sound, merge, move_edge, move_zone, place_copies,
    resize_rect, resize_zone,
};
use crate::geometry::Point;
use crate::layout::{MAX_ZONES, UnitRect, Zone, presets};

/// One editor gesture, aimed at the zones at `zone` and `other` modulo the zone count.
#[derive(Debug, Clone)]
enum Gesture {
    Move {
        zone: usize,
        x: f64,
        y: f64,
    },
    Edge {
        zone: usize,
        vertical: bool,
        end: bool,
        to: f64,
    },
    Resize {
        zone: usize,
        handle: usize,
        dx: f64,
        dy: f64,
    },
    Draw {
        from: (f64, f64),
        to: (f64, f64),
    },
    Cut {
        zone: usize,
        line: f64,
        vertical: bool,
        through: bool,
    },
    Merge {
        zone: usize,
        other: usize,
    },
    Paste {
        zone: usize,
        other: usize,
    },
}

fn gesture() -> impl Strategy<Value = Gesture> {
    let unit = -0.2..1.2f64;
    let delta = -0.6..0.6f64;
    prop_oneof![
        (any::<usize>(), unit.clone(), unit.clone()).prop_map(|(zone, x, y)| Gesture::Move { zone, x, y }),
        (any::<usize>(), any::<bool>(), any::<bool>(), unit.clone()).prop_map(|(zone, vertical, end, to)| {
            Gesture::Edge {
                zone,
                vertical,
                end,
                to,
            }
        }),
        (any::<usize>(), 0..ResizeHandle::ALL.len(), delta.clone(), delta)
            .prop_map(|(zone, handle, dx, dy)| Gesture::Resize { zone, handle, dx, dy }),
        ((unit.clone(), unit.clone()), (unit.clone(), unit))
            .prop_map(|(from, to)| Gesture::Draw { from, to }),
        (any::<usize>(), 0.0..1.0f64, any::<bool>(), any::<bool>()).prop_map(
            |(zone, line, vertical, through)| Gesture::Cut {
                zone,
                line,
                vertical,
                through,
            }
        ),
        (any::<usize>(), any::<usize>()).prop_map(|(zone, other)| Gesture::Merge { zone, other }),
        (any::<usize>(), any::<usize>()).prop_map(|(zone, other)| Gesture::Paste { zone, other }),
    ]
}

fn axis(vertical: bool) -> Axis {
    if vertical { Axis::Y } else { Axis::X }
}

/// IDs for the zones a gesture adds, unique across the sequence.
fn new_ids(step: usize, count: usize) -> Vec<String> {
    (0..count).map(|index| format!("{step}-{index}")).collect()
}

fn apply(zones: Vec<Zone>, gesture: &Gesture, step: usize) -> Vec<Zone> {
    let pick = |index: usize| zones.get(index % zones.len().max(1)).cloned();
    match *gesture {
        Gesture::Move { zone, x, y } => match pick(zone) {
            Some(zone) => move_zone(&zones, &zone.id, Point::new(x, y)),
            None => zones,
        },
        Gesture::Edge {
            zone,
            vertical,
            end,
            to,
        } => match pick(zone) {
            Some(zone) => {
                let side = if end { Side::End } else { Side::Start };
                move_edge(&zones, &zone.id, axis(vertical), side, to)
            }
            None => zones,
        },
        Gesture::Resize { zone, handle, dx, dy } => match pick(zone) {
            Some(zone) => {
                let handle = ResizeHandle::ALL[handle];
                let target = resize_rect(zone.rect, dx, dy, handle);
                resize_zone(&zones, &zone.id, target, handle)
            }
            None => zones,
        },
        Gesture::Draw { from, to } => {
            let drawn = draw(&zones, Point::new(from.0, from.1), Point::new(to.0, to.1));
            let mut zones = zones;
            if zones.len() < MAX_ZONES {
                zones.extend(drawn.map(|rect| Zone::new(format!("{step}-drawn"), rect)));
            }
            zones
        }
        Gesture::Cut {
            zone,
            line,
            vertical,
            through,
        } => {
            let Some(target) = pick(zone) else {
                return zones;
            };
            let scope = if through {
                CutScope::Through
            } else {
                CutScope::Zone(&target.id)
            };
            cut(&zones, scope, axis(vertical), line, &new_ids(step, MAX_ZONES)).unwrap_or(zones)
        }
        Gesture::Merge { zone, other } => match (pick(zone), pick(other)) {
            (Some(zone), Some(other)) => {
                merge(&zones, &[zone.id, other.id]).map_or(zones, |merged| merged.zones)
            }
            _ => zones,
        },
        Gesture::Paste { zone, other } => {
            let copied: Vec<UnitRect> = [pick(zone), pick(other)]
                .into_iter()
                .flatten()
                .map(|zone| zone.rect)
                .collect();
            let placed = place_copies(&copied, &zones).unwrap_or_default();
            let mut zones = zones;
            zones.extend(
                new_ids(step, placed.len())
                    .into_iter()
                    .zip(placed)
                    .map(|(id, rect)| Zone::new(id, rect)),
            );
            zones
        }
    }
}

proptest! {
    #[test]
    fn editor_gestures_never_create_overlapping_undersized_or_too_many_zones(
        preset in 0..presets().len(),
        gestures in prop::collection::vec(gesture(), 1..40),
    ) {
        let mut zones = presets()[preset].zones.clone();
        prop_assert!(is_sound(&zones));
        for (step, gesture) in gestures.iter().enumerate() {
            zones = apply(zones, gesture, step);
            prop_assert!(is_sound(&zones), "after {gesture:?}: {zones:?}");
            prop_assert!(zones.len() <= MAX_ZONES, "after {gesture:?}: {} zones", zones.len());
        }
    }

    #[test]
    fn gestures_on_an_empty_canvas_stay_sound(gestures in prop::collection::vec(gesture(), 1..40)) {
        let mut zones = Vec::new();
        for (step, gesture) in gestures.iter().enumerate() {
            zones = apply(zones, gesture, step);
            prop_assert!(is_sound(&zones), "after {gesture:?}: {zones:?}");
            prop_assert!(zones.len() <= MAX_ZONES, "after {gesture:?}: {} zones", zones.len());
        }
    }
}
