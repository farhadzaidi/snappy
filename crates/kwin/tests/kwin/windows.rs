//! Listing, moving, and parking windows.

use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{DesktopGuard, WindowId, WindowSystem};

use crate::support::{Fixture, Session, kwin};

#[test]
fn windows_are_listed_with_their_facts_and_move_where_asked() {
    let mut kwin = kwin!();
    let fixture = Fixture::open("facts", Size::new(600.0, 400.0));
    let window = kwin.window_of(&fixture);
    assert!(
        window.recovery_key.is_some(),
        "KWin's UUID and the process identify it"
    );

    let frame = kwin.place(&window, Point::new(200.0, 150.0));
    assert!(frame.width >= 600.0 && frame.height >= 400.0, "{frame:?}");
    kwin.platform
        .set_size(window.id, Size::new(700.0, 500.0))
        .unwrap();
    kwin.wait_until("the window resizes", |session| {
        let frame = session.platform.frame(window.id).unwrap();
        (frame.width - 700.0).abs() < 1.0 && (frame.height - 500.0).abs() < 1.0
    });

    let displays = kwin.platform.displays();
    assert_eq!(displays.len(), 2);
    assert_eq!(displays[0].frame, Rect::new(0.0, 0.0, 1920.0, 1080.0));
    assert_eq!(displays[1].frame.x, 1920.0);
    assert_eq!(
        kwin.platform.window_at(Point::new(300.0, 300.0)).unwrap().id,
        window.id
    );
    assert!(
        kwin.platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window.id)
    );
}

#[test]
fn visible_windows_come_front_to_back_and_lowering_puts_one_behind() {
    let mut kwin = kwin!();
    let below_fixture = Fixture::open("below", Size::new(500.0, 400.0));
    let below = kwin.window_of(&below_fixture);
    kwin.place(&below, Point::new(100.0, 100.0));
    let above_fixture = Fixture::open("above", Size::new(500.0, 400.0));
    let above = kwin.window_of(&above_fixture);
    kwin.place(&above, Point::new(300.0, 250.0));
    kwin.platform.focus(above.id);
    let order = |session: &mut Session| -> Vec<WindowId> {
        session
            .platform
            .visible_windows(None)
            .into_iter()
            .map(|info| info.id)
            .filter(|window| [below.id, above.id].contains(window))
            .collect()
    };
    kwin.wait_until("the window focused is in front", |session| {
        order(session) == [above.id, below.id]
    });

    let frame = kwin.platform.frame(above.id).unwrap();
    kwin.platform.lower(above.id).unwrap();
    kwin.wait_until("the window lowered is behind", |session| {
        order(session) == [below.id, above.id]
    });
    assert_eq!(kwin.platform.frame(above.id).unwrap(), frame, "lowered in place");
    assert_eq!(
        kwin.platform.lower(above.id).map_err(|error| error.to_string()),
        Err("KWin lets Snappy lower only the active window. Click it and try again.".to_owned()),
        "KWin's scripts lower only the active window"
    );

    kwin.platform.focus(above.id);
    kwin.wait_until("the window focused is in front again", |session| {
        order(session) == [above.id, below.id]
    });
}

#[test]
fn a_window_just_focused_is_lowered_once_kwin_activates_it() {
    let mut kwin = kwin!();
    let back_fixture = Fixture::open("back", Size::new(500.0, 400.0));
    let back = kwin.window_of(&back_fixture);
    kwin.place(&back, Point::new(100.0, 100.0));
    let front_fixture = Fixture::open("front", Size::new(500.0, 400.0));
    let front = kwin.window_of(&front_fixture);
    kwin.place(&front, Point::new(300.0, 250.0));
    kwin.platform.focus(front.id);
    kwin.wait_until("the front window is active", |session| {
        session
            .platform
            .focused_window()
            .is_ok_and(|window| window.id == front.id)
    });

    // As a second press does straight after the first focused the window behind.
    kwin.platform.focus(back.id);
    kwin.platform.lower(back.id).unwrap();
    kwin.wait_until("the window focused and lowered is at the back", |session| {
        let order: Vec<WindowId> = session
            .platform
            .visible_windows(None)
            .into_iter()
            .map(|info| info.id)
            .filter(|window| [back.id, front.id].contains(window))
            .collect();
        order == [front.id, back.id]
    });
}

#[test]
fn parking_hides_a_window_on_snappys_desktop_and_brings_it_back() {
    let mut kwin = kwin!();
    let fixture = Fixture::open("park", Size::new(400.0, 300.0));
    let window = kwin.window_of(&fixture);
    let frame = kwin.place(&window, Point::new(100.0, 100.0));
    let displays = kwin.platform.displays();
    kwin.platform.bind(&displays);
    assert!(kwin.platform.on_managed_desktop());

    kwin.platform
        .hide_for_workspace(window.id, Point::default())
        .unwrap();
    assert!(kwin.platform.state(window.id).unwrap().on_parking_desktop);
    assert!(
        !kwin
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window.id)
    );
    assert!(
        kwin.platform.on_managed_desktop(),
        "parking doesn't switch desktops"
    );

    kwin.platform
        .show_for_workspace(window.id, frame.origin())
        .unwrap();
    assert!(!kwin.platform.state(window.id).unwrap().on_parking_desktop);
    assert!(
        kwin.platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window.id)
    );
    kwin.platform.clear();
}
