//! Listing, moving, and parking windows.

use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{DesktopGuard, WindowSystem};

use crate::support::{Fixture, kwin};

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
