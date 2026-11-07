//! Property tests: any sequence of editor operations on a sound layout leaves it sound.

use proptest::prelude::*;

use super::{
    Axis, ResizeHandle, Side, draw, is_sound, move_edge, move_zone, resize_rect, resize_zone, split,
};
use crate::geometry::Point;
use crate::layout::{Zone, presets};

/// One editor gesture, aimed at the zone at `zone` modulo the zone count.
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
    Split {
        zone: usize,
        fraction: f64,
        vertical: bool,
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
        (any::<usize>(), 0.0..1.0f64, any::<bool>()).prop_map(|(zone, fraction, vertical)| Gesture::Split {
            zone,
            fraction,
            vertical
        }),
    ]
}

fn axis(vertical: bool) -> Axis {
    if vertical { Axis::Y } else { Axis::X }
}

fn apply(zones: Vec<Zone>, gesture: &Gesture, next_id: usize) -> Vec<Zone> {
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
            zones.extend(drawn.map(|rect| Zone::new(format!("drawn-{next_id}"), rect)));
            zones
        }
        Gesture::Split {
            zone,
            fraction,
            vertical,
        } => {
            let Some(target) = pick(zone) else {
                return zones;
            };
            match split(&target, fraction, axis(vertical), format!("cut-{next_id}")) {
                Some((first, second)) => {
                    let mut zones: Vec<Zone> =
                        zones.into_iter().filter(|zone| zone.id != target.id).collect();
                    zones.extend([first, second]);
                    zones
                }
                None => zones,
            }
        }
    }
}

proptest! {
    #[test]
    fn editor_gestures_never_create_overlapping_or_undersized_zones(
        preset in 0..presets().len(),
        gestures in prop::collection::vec(gesture(), 1..40),
    ) {
        let mut zones = presets()[preset].zones.clone();
        prop_assert!(is_sound(&zones));
        for (step, gesture) in gestures.iter().enumerate() {
            zones = apply(zones, gesture, step);
            prop_assert!(is_sound(&zones), "after {gesture:?}: {zones:?}");
        }
    }

    #[test]
    fn gestures_on_an_empty_canvas_stay_sound(gestures in prop::collection::vec(gesture(), 1..40)) {
        let mut zones = Vec::new();
        for (step, gesture) in gestures.iter().enumerate() {
            zones = apply(zones, gesture, step);
            prop_assert!(is_sound(&zones), "after {gesture:?}: {zones:?}");
        }
    }
}
