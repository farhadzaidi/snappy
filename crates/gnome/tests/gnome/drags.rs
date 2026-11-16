//! Drags: what they report, when, and the drag key keeping GNOME's edge snapping out.

use std::time::Duration;

use serde_json::json;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{Modifier, Mods};
use snappy_linux::key_codes;
use snappy_platform::{Input, PlatformEvent, WindowSystem};

use crate::support::{Shell, TITLE_BAR_GRIP, floating, key_request, shell};

#[test]
fn a_drag_reports_its_window_and_the_modifiers_held_at_the_release() {
    let mut shell = shell!();
    let id = shell.open("drag", floating(500.0));
    shell.platform.set_press_monitoring(true);
    shell.platform.set_drag_monitoring(true);
    shell.drain();

    let frame = floating(500.0);
    let grip = Point::new(frame.mid_x(), frame.y + TITLE_BAR_GRIP);
    shell.pointer(grip);
    std::thread::sleep(Duration::from_millis(100));
    shell.modifier(Modifier::Shift, true);
    shell.button(true);
    shell.pointer(Point::new(grip.x - 10.0, grip.y + 7.0));
    shell.wait_until_moving();
    shell.pointer_steps(grip, (-30.0, 20.0), 8);
    // Let go of Shift right behind the button, as fast as input allows.
    shell.helper(json!({"op": "button", "pressed": false}));
    shell.helper(key_request(key_codes::modifier_keycode(Modifier::Shift), false));

    let events = shell.wait_for("the release", |event| {
        matches!(event, PlatformEvent::PointerUp { .. })
    });
    let shift = Mods::from_iter([Modifier::Shift]);
    let down = events.iter().find_map(|event| match event {
        PlatformEvent::PointerDown { mods, window, .. } => Some((*mods, *window)),
        _ => None,
    });
    assert_eq!(
        down,
        Some((shift, Some(snappy_platform::WindowId(id)))),
        "{events:?}"
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, PlatformEvent::PointerDragged { mods, .. } if *mods == shift)),
        "{events:?}"
    );
    let Some(PlatformEvent::PointerUp { mods, .. }) = events.last() else {
        panic!("{events:?}");
    };
    assert_eq!(*mods, shift, "Shift was down when the button came up");
}

#[test]
fn without_press_monitoring_drags_are_not_reported() {
    let mut shell = shell!();
    shell.open("unwatched", floating(600.0));
    shell.platform.set_press_monitoring(false);
    shell.platform.set_drag_monitoring(false);
    shell.drain();
    shell.pointer(Point::new(900.0, 314.0));
    shell.button(true);
    shell.pointer(Point::new(950.0, 360.0));
    shell.button(false);
    let pointer: Vec<PlatformEvent> = shell
        .drain()
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                PlatformEvent::PointerDown { .. } | PlatformEvent::PointerUp { .. }
            )
        })
        .collect();
    assert!(pointer.is_empty(), "{pointer:?}");
}

/// Drags `id`'s title bar from `(500, 164)` by 390 × 330 with Shift held, past another window's
/// edges, and returns where it ends.
fn shift_drag_past_an_edge(shell: &mut Shell, id: u64) -> Rect {
    shell.helper(json!({"op": "placeWindow", "title": "Fixture follower", "x": 200, "y": 150}));
    std::thread::sleep(Duration::from_millis(300));
    shell.pointer(Point::new(500.0, 164.0));
    std::thread::sleep(Duration::from_millis(100));
    shell.modifier(Modifier::Shift, true);
    shell.button(true);
    shell.pointer(Point::new(510.0, 170.0));
    shell.wait_until_moving();
    shell.pointer_steps(Point::new(510.0, 170.0), (13.0, 11.0), 30);
    std::thread::sleep(Duration::from_millis(100));
    shell.button(false);
    shell.modifier(Modifier::Shift, false);
    std::thread::sleep(Duration::from_millis(300));
    shell.platform.frame(snappy_platform::WindowId(id)).unwrap()
}

#[test]
fn with_the_drag_key_held_a_moved_window_follows_the_pointer_not_gnomes_edges() {
    let mut shell = shell!();
    shell.open("edges", Rect::new(1000.0, 500.0, 600.0, 400.0));
    let id = shell.open("follower", Rect::new(200.0, 150.0, 600.0, 400.0));
    shell.platform.set_press_monitoring(true);

    // Where the pointer takes the window: it started moving with the pointer at (510, 170)
    // and the pointer ends at (900, 500).
    let followed = Point::new(200.0 + 390.0, 150.0 + 330.0);
    let near = |frame: Rect| (frame.x - followed.x).abs() <= 12.0 && (frame.y - followed.y).abs() <= 12.0;

    shell.platform.set_drag_modifier(Modifier::Shift);
    let frame = shift_drag_past_an_edge(&mut shell, id);
    assert!(
        near(frame),
        "the window followed the pointer to about {followed:?}, not {frame:?}"
    );

    // With another drag key, GNOME's own Shift-snapping is left alone and pulls the window onto
    // the other window's edges, so the drag above really did undo it.
    shell.platform.set_drag_modifier(Modifier::Ctrl);
    let frame = shift_drag_past_an_edge(&mut shell, id);
    assert!(!near(frame), "GNOME snapped the window to an edge: {frame:?}");
}
