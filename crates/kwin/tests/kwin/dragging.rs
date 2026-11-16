//! Drags and the modifier keys held during them.

use snappy_core::geometry::{Point, Size};
use snappy_core::keys::{Modifier, Mods};
use snappy_platform::{Input, PlatformEvent};

use crate::input::input;
use crate::support::{Fixture, TITLE_BAR_GRIP, kwin};

#[test]
fn a_drag_reports_the_modifiers_held_when_the_button_came_up() {
    let mut kwin = kwin!();
    let fixture = Fixture::open("drag", Size::new(500.0, 300.0));
    let window = kwin.window_of(&fixture);
    let frame = kwin.place(&window, Point::new(300.0, 200.0));
    kwin.platform.set_press_monitoring(true);
    kwin.drain();

    let mut input = input();
    let grip = Point::new(frame.x + frame.width / 2.0, frame.y + TITLE_BAR_GRIP);
    input.move_to(grip);
    input.modifier(Modifier::Shift, true);
    input.button(true);
    for step in 1..=5 {
        input.move_to(Point::new(
            grip.x + 20.0 * f64::from(step),
            grip.y + 10.0 * f64::from(step),
        ));
    }
    let PlatformEvent::PointerDown { window: pressed, .. } =
        kwin.next(|event| matches!(event, PlatformEvent::PointerDown { .. }))
    else {
        unreachable!()
    };
    assert_eq!(pressed, Some(window.id));
    // The engine turns drag monitoring on when a press starts tracking a window.
    kwin.platform.set_drag_monitoring(true);
    let PlatformEvent::ModifiersChanged { mods, .. } =
        kwin.next(|event| matches!(event, PlatformEvent::ModifiersChanged { .. }))
    else {
        unreachable!()
    };
    assert_eq!(mods, Mods::from_iter([Modifier::Shift]));
    input.move_to(Point::new(grip.x + 200.0, grip.y + 120.0));
    let PlatformEvent::PointerDragged { at, mods } =
        kwin.next(|event| matches!(event, PlatformEvent::PointerDragged { .. }))
    else {
        unreachable!()
    };
    assert!(mods.contains(Modifier::Shift));
    assert!(at.x > grip.x, "{at:?}");

    // Let go of Shift right behind the button, as quickly as input allows.
    input.button(false);
    input.modifier(Modifier::Shift, false);
    let PlatformEvent::PointerUp { mods, .. } =
        kwin.next(|event| matches!(event, PlatformEvent::PointerUp { .. }))
    else {
        unreachable!()
    };
    assert_eq!(
        mods,
        Mods::from_iter([Modifier::Shift]),
        "Shift was down at the release"
    );
    kwin.platform.set_drag_monitoring(false);
}
