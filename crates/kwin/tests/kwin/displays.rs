//! Displays' work areas as panels come, change, and go.

use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{Input, PlatformEvent, WindowSystem};

use crate::input::input;
use crate::panel::{Edge, Panel};
use crate::support::{Fixture, Session, TITLE_BAR_GRIP, kwin};

#[test]
fn work_areas_follow_panels_as_they_come_change_and_go() {
    let mut kwin = kwin!();
    let output = kwin.platform.displays()[0].id.clone();
    let area = |kwin: &mut Session| kwin.platform.displays()[0].work_area;
    assert_eq!(area(&mut kwin), Rect::new(0.0, 0.0, 1920.0, 1080.0));
    kwin.drain();

    let mut top = Panel::open(&output, Edge::Top, 40);
    kwin.next(|event| matches!(event, PlatformEvent::DisplaysChanged));
    assert_eq!(area(&mut kwin), Rect::new(0.0, 40.0, 1920.0, 1040.0));

    let bottom = Panel::open(&output, Edge::Bottom, 51);
    kwin.next(|event| matches!(event, PlatformEvent::DisplaysChanged));
    assert_eq!(area(&mut kwin), Rect::new(0.0, 40.0, 1920.0, 989.0));

    // Reserving less space moves no panel, so KWin says nothing; the next drag finds out
    // before its press is reported.
    top.reserve(0);
    let fixture = Fixture::open("panels", Size::new(500.0, 300.0));
    let window = kwin.window_of(&fixture);
    let frame = kwin.place(&window, Point::new(300.0, 300.0));
    kwin.platform.set_press_monitoring(true);
    kwin.drain();
    let mut input = input();
    let grip = Point::new(frame.x + frame.width / 2.0, frame.y + TITLE_BAR_GRIP);
    input.move_to(grip);
    input.button(true);
    for step in 1..=5 {
        input.move_to(Point::new(grip.x + 20.0 * f64::from(step), grip.y));
    }
    let first = kwin.next(|event| {
        matches!(
            event,
            PlatformEvent::DisplaysChanged | PlatformEvent::PointerDown { .. }
        )
    });
    input.button(false);
    assert_eq!(
        first,
        PlatformEvent::DisplaysChanged,
        "the change is heard before the press"
    );
    assert_eq!(area(&mut kwin), Rect::new(0.0, 0.0, 1920.0, 1029.0));
    drop(input);

    drop(bottom);
    drop(top);
    kwin.next(|event| matches!(event, PlatformEvent::DisplaysChanged));
    kwin.wait_until("the panels' space is free again", |session| {
        session.platform.displays()[0].work_area == Rect::new(0.0, 0.0, 1920.0, 1080.0)
    });
}
