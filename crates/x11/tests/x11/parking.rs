//! Parking windows on Snappy's own desktop, and following the desktop in use.

use snappy_core::geometry::{Point, Rect};
use snappy_platform::{DesktopGuard, PlatformEvent, WindowSystem};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ClientMessageEvent, ConnectionExt as _, EventMask};

use crate::support::{Desktop, desktop, id, wait_until};

#[test]
fn parked_windows_wait_on_snappys_desktop_and_return_exactly() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(300.0, 200.0, 400.0, 300.0));
    let home = desktop.platform.frame(id(window)).unwrap();
    let displays = desktop.platform.displays();
    let desktops = desktop.desktop_count();
    let names: Vec<String> = (1..=desktops).map(|number| format!("Desk {number}")).collect();
    desktop.set_desktop_names(&names);
    desktop.platform.bind(&displays);
    assert_eq!(desktop.desktop_count(), desktops + 1);
    assert_eq!(desktop.desktop_names().last().map(String::as_str), Some("Snappy"));
    assert!(desktop.platform.on_managed_desktop());

    desktop
        .platform
        .hide_for_workspace(id(window), home.origin())
        .unwrap();
    assert!(desktop.platform.state(id(window)).unwrap().on_parking_desktop);
    assert_eq!(desktop.window_desktop(window), Some(desktops));
    assert!(wait_until(|| !desktop.is_viewable(window)), "hidden at once");
    assert_eq!(
        desktop.platform.frame(id(window)).unwrap(),
        home,
        "the frame is untouched"
    );
    assert!(
        !desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(window))
    );

    desktop
        .platform
        .show_for_workspace(id(window), home.origin())
        .unwrap();
    assert!(!desktop.platform.state(id(window)).unwrap().on_parking_desktop);
    assert_eq!(desktop.window_desktop(window), Some(0));
    assert!(wait_until(|| desktop.is_viewable(window)));
    assert_eq!(desktop.frame_becomes(window, home), home);

    desktop.platform.clear();
    assert!(wait_until(|| desktop.desktop_count() == desktops));
    assert_eq!(desktop.desktop_names(), names, "the user's names are kept");
    desktop.set_desktop_names(&[]);
}

#[test]
fn showing_a_parked_window_reapplies_its_saved_position() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(300.0, 200.0, 400.0, 300.0));
    let home = desktop.platform.frame(id(window)).unwrap();
    let displays = desktop.platform.displays();
    desktop.platform.bind(&displays);
    desktop
        .platform
        .hide_for_workspace(id(window), home.origin())
        .unwrap();

    let saved = home.with_origin(Point::new(500.0, 260.0));
    desktop
        .platform
        .show_for_workspace(id(window), saved.origin())
        .unwrap();
    assert_eq!(desktop.platform.frame(id(window)).unwrap(), saved);
    assert!(wait_until(|| desktop.is_viewable(window)));
    desktop.platform.clear();
}

#[test]
fn a_restarted_daemon_finds_windows_left_on_the_parking_desktop() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(300.0, 200.0, 400.0, 300.0));
    let home = desktop.platform.frame(id(window)).unwrap();
    let displays = desktop.platform.displays();
    let desktops = desktop.desktop_count();
    desktop.platform.bind(&displays);
    desktop
        .platform
        .hide_for_workspace(id(window), home.origin())
        .unwrap();

    let mut restarted = desktop!();
    let platform = &mut restarted.platform;
    assert!(platform.state(id(window)).unwrap().on_parking_desktop);
    // Xfwm4 takes moving and resizing out of a parked window's allowed actions.
    let parked = platform
        .all_windows()
        .into_iter()
        .find(|info| info.id == id(window))
        .expect("the parked window is listed");
    assert!(parked.recovery_key.is_some());
    platform.show_for_workspace(id(window), home.origin()).unwrap();
    assert!(wait_until(|| desktop.is_viewable(window)));
    platform.bind(&displays);
    assert_eq!(
        desktop.desktop_count(),
        desktops + 1,
        "the same desktop is reused"
    );
    platform.clear();
    assert!(wait_until(|| desktop.desktop_count() == desktops));
}

#[test]
fn desktop_guard_follows_the_native_desktop() {
    let mut desktop = desktop!();
    let displays = desktop.platform.displays();
    desktop.platform.bind(&displays);
    assert!(desktop.platform.on_managed_desktop());

    let switch_to = |desktop: &Desktop, number: u32| {
        let event = ClientMessageEvent::new(
            32,
            desktop.root,
            desktop.atom("_NET_CURRENT_DESKTOP"),
            [number, 0, 0, 0, 0],
        );
        let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
        desktop
            .client
            .send_event(false, desktop.root, mask, event)
            .unwrap();
        desktop.client.flush().unwrap();
    };
    switch_to(&desktop, 1);
    assert!(wait_until(|| !desktop.platform.on_managed_desktop()));
    assert_eq!(
        desktop.expect(|event| *event == PlatformEvent::NativeDesktopChanged),
        Some(PlatformEvent::NativeDesktopChanged)
    );
    switch_to(&desktop, 0);
    assert!(wait_until(|| desktop.platform.on_managed_desktop()));
    desktop.platform.clear();
    assert!(!desktop.platform.on_managed_desktop());
}
