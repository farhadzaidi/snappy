//! Pointer presses, drags, and modifier changes, reported only while monitoring asks.

use snappy_core::geometry::Point;
use snappy_core::keys::{Modifier, Mods};
use snappy_platform::{Input, PlatformEvent};

use crate::support::{desktop, key};

#[test]
fn pointer_presses_drags_and_releases_arrive_in_order() {
    let mut desktop = desktop!();
    desktop.move_pointer(Point::new(1000.0, 600.0));
    desktop.platform.set_press_monitoring(true);
    desktop.platform.set_drag_monitoring(true);
    desktop.drain();

    desktop.button(true);
    desktop.move_pointer(Point::new(1050.0, 620.0));
    desktop.key(key::SHIFT_L, true);
    desktop.move_pointer(Point::new(1100.0, 650.0));
    desktop.key(key::SHIFT_L, false);
    desktop.button(false);

    let pointer: Vec<PlatformEvent> = desktop
        .drain()
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                PlatformEvent::PointerDown { .. }
                    | PlatformEvent::PointerDragged { .. }
                    | PlatformEvent::PointerUp { .. }
                    | PlatformEvent::ModifiersChanged { .. }
            )
        })
        .collect();
    let shift = [Modifier::Shift].into_iter().collect();
    assert!(
        matches!(
            pointer.first(),
            Some(PlatformEvent::PointerDown { at, mods, window: None, .. })
                if *at == Point::new(1000.0, 600.0) && mods.is_empty()
        ),
        "{pointer:?}"
    );
    assert!(
        pointer.iter().any(|event| matches!(
            event,
            PlatformEvent::PointerDragged { at, .. } if *at == Point::new(1050.0, 620.0)
        )),
        "{pointer:?}"
    );
    assert!(
        pointer.iter().any(|event| matches!(
            event,
            PlatformEvent::ModifiersChanged { mods, .. } if *mods == shift
        )),
        "{pointer:?}"
    );
    assert!(
        pointer.iter().any(|event| matches!(
            event,
            PlatformEvent::PointerDragged { at, mods } if *at == Point::new(1100.0, 650.0) && *mods == shift
        )),
        "{pointer:?}"
    );
    assert!(
        matches!(
            pointer.last(),
            Some(PlatformEvent::PointerUp { at, .. }) if *at == Point::new(1100.0, 650.0)
        ),
        "{pointer:?}"
    );
}

#[test]
fn a_release_reports_the_drag_modifier_even_when_it_is_let_go_right_after() {
    let mut desktop = desktop!();
    desktop.move_pointer(Point::new(1000.0, 600.0));
    desktop.platform.set_press_monitoring(true);
    desktop.platform.set_drag_monitoring(true);
    desktop.drain();

    desktop.key(key::SHIFT_L, true);
    desktop.button(true);
    desktop.move_pointer(Point::new(1100.0, 650.0));
    desktop.release_button_then_key(key::SHIFT_L);

    let shift: Mods = [Modifier::Shift].into_iter().collect();
    let release = desktop.expect(|event| matches!(event, PlatformEvent::PointerUp { .. }));
    assert!(
        matches!(&release, Some(PlatformEvent::PointerUp { mods, .. }) if *mods == shift),
        "{release:?}"
    );
    let after = desktop.drain();
    assert!(
        after.iter().any(|event| matches!(
            event,
            PlatformEvent::ModifiersChanged { mods, .. } if mods.is_empty()
        )),
        "letting go of Shift is still reported: {after:?}"
    );
}

#[test]
fn keys_motion_and_presses_go_unreported_once_monitoring_is_off() {
    let mut desktop = desktop!();
    desktop.platform.set_press_monitoring(true);
    desktop.platform.set_drag_monitoring(true);
    desktop.drain();

    desktop.platform.set_drag_monitoring(false);
    desktop.drain();
    desktop.tap(&[key::SHIFT_L]);
    desktop.move_pointer(Point::new(900.0, 500.0));
    let events = desktop.drain();
    assert!(
        !events.iter().any(|event| matches!(
            event,
            PlatformEvent::ModifiersChanged { .. } | PlatformEvent::PointerDragged { .. }
        )),
        "{events:?}"
    );

    desktop.platform.set_press_monitoring(false);
    desktop.button(true);
    desktop.button(false);
    assert!(
        !desktop
            .drain()
            .iter()
            .any(|event| matches!(event, PlatformEvent::PointerDown { .. }))
    );
}
