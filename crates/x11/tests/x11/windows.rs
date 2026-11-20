//! Finding, reading, moving, focusing, and following other clients' windows.

use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{PlatformEvent, WindowSystem};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, PropMode};
use x11rb::wrapper::ConnectionExt as _;

use crate::support::{Desktop, desktop, id, wait_until};

#[test]
fn window_at_finds_the_topmost_managed_window() {
    let mut desktop = desktop!();
    let below = desktop.open_plain(Rect::new(100.0, 100.0, 400.0, 300.0));
    let above = desktop.open_plain(Rect::new(300.0, 250.0, 400.0, 300.0));
    let frame_below = desktop.platform.frame(id(below)).unwrap();
    let frame_above = desktop.platform.frame(id(above)).unwrap();

    let overlap = Point::new(frame_above.x + 10.0, frame_above.y + 10.0);
    assert!(frame_below.contains(overlap));
    let found = desktop.platform.window_at(overlap).unwrap();
    assert_eq!(found.id, id(above));
    assert_eq!(found.app, "Fixture");
    assert_eq!(found.pid, std::process::id());
    assert!(found.recovery_key.is_some());

    let only_below = Point::new(frame_below.x + 5.0, frame_below.y + 5.0);
    assert_eq!(desktop.platform.window_at(only_below).unwrap().id, id(below));
    assert!(desktop.platform.window_at(Point::new(3000.0, 900.0)).is_err());
}

#[test]
fn visible_windows_come_front_to_back_and_lowering_or_restacking_puts_one_behind() {
    let mut desktop = desktop!();
    let below = desktop.open_plain(Rect::new(100.0, 100.0, 400.0, 300.0));
    let above = desktop.open_plain(Rect::new(300.0, 250.0, 400.0, 300.0));
    let order = |desktop: &mut Desktop| -> Vec<snappy_platform::WindowId> {
        desktop
            .platform
            .visible_windows(None)
            .into_iter()
            .map(|info| info.id)
            .filter(|window| [id(below), id(above)].contains(window))
            .collect()
    };
    assert_eq!(order(&mut desktop), [id(above), id(below)]);

    let frame = desktop.platform.frame(id(above)).unwrap();
    desktop.platform.lower(id(above)).unwrap();
    assert!(
        wait_until(|| order(&mut desktop) == [id(below), id(above)]),
        "{:?}",
        order(&mut desktop)
    );
    assert_eq!(
        desktop.platform.frame(id(above)).unwrap(),
        frame,
        "lowered in place"
    );

    desktop.platform.focus(id(above));
    assert!(wait_until(|| order(&mut desktop) == [id(above), id(below)]));

    desktop.platform.stack_below(id(above), id(below)).unwrap();
    assert!(
        wait_until(|| order(&mut desktop) == [id(below), id(above)]),
        "restacked under the other: {:?}",
        order(&mut desktop)
    );
}

#[test]
fn dialogs_are_refused_with_a_message() {
    let mut desktop = desktop!();
    let dialog = desktop.open(Rect::new(200.0, 200.0, 300.0, 200.0), |desktop, window| {
        let kind = desktop.atom("_NET_WM_WINDOW_TYPE");
        let dialog = desktop.atom("_NET_WM_WINDOW_TYPE_DIALOG");
        desktop
            .client
            .change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, &[dialog])
            .unwrap();
    });
    let frame = desktop.platform.frame(id(dialog)).unwrap();
    let error = desktop
        .platform
        .window_at(Point::new(frame.x + 20.0, frame.y + 20.0))
        .unwrap_err();
    assert_eq!(error.to_string(), "Choose a standard application window.");
    assert!(
        !desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(dialog))
    );
}

#[test]
fn frames_include_decorations_and_follow_moves_and_resizes() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(150.0, 150.0, 500.0, 400.0));
    // Windows the engine has been told about are tracked for moves.
    assert!(
        desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(window))
    );
    let extents = desktop.cardinals(window, "_NET_FRAME_EXTENTS", AtomEnum::CARDINAL);
    let frame = desktop.platform.frame(id(window)).unwrap();
    assert_eq!(frame.width, 500.0 + f64::from(extents[0] + extents[1]));
    assert_eq!(frame.height, 400.0 + f64::from(extents[2] + extents[3]));

    desktop
        .platform
        .set_position(id(window), Point::new(240.0, 180.0))
        .unwrap();
    let moved = Rect::new(240.0, 180.0, frame.width, frame.height);
    assert_eq!(desktop.frame_becomes(window, moved), moved);

    desktop
        .platform
        .set_size(id(window), Size::new(800.0, 600.0))
        .unwrap();
    let resized = Rect::new(240.0, 180.0, 800.0, 600.0);
    assert_eq!(desktop.frame_becomes(window, resized), resized);

    desktop
        .platform
        .set_position(id(window), Point::new(2000.0, 100.0))
        .unwrap();
    let other_display = Rect::new(2000.0, 100.0, 800.0, 600.0);
    assert_eq!(desktop.frame_becomes(window, other_display), other_display);

    let events = desktop.drain();
    assert!(
        events.contains(&PlatformEvent::WindowMoved(id(window))),
        "{events:?}"
    );
}

#[test]
fn maximized_windows_are_restored_before_moving() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(100.0, 100.0, 400.0, 300.0));
    desktop.set_state(
        window,
        true,
        ["_NET_WM_STATE_MAXIMIZED_VERT", "_NET_WM_STATE_MAXIMIZED_HORZ"],
    );
    assert!(wait_until(|| desktop
        .platform
        .frame(id(window))
        .is_ok_and(|frame| frame.width > 1000.0)));

    desktop
        .platform
        .set_position(id(window), Point::new(300.0, 200.0))
        .unwrap();
    desktop
        .platform
        .set_size(id(window), Size::new(600.0, 500.0))
        .unwrap();
    let expected = Rect::new(300.0, 200.0, 600.0, 500.0);
    assert_eq!(desktop.frame_becomes(window, expected), expected);
    let states = desktop.cardinals(window, "_NET_WM_STATE", AtomEnum::ATOM);
    assert!(!states.contains(&desktop.atom("_NET_WM_STATE_MAXIMIZED_HORZ")));
}

#[test]
fn focused_window_follows_activation() {
    let mut desktop = desktop!();
    let first = desktop.open_plain(Rect::new(100.0, 100.0, 300.0, 200.0));
    let second = desktop.open_plain(Rect::new(500.0, 100.0, 300.0, 200.0));
    for window in [first, second, first] {
        desktop.platform.focus(id(window));
        assert!(wait_until(|| desktop
            .platform
            .focused_window()
            .is_ok_and(|info| info.id == id(window))));
    }
    let focus = desktop.expect(|event| matches!(event, PlatformEvent::FocusChanged { .. }));
    assert_eq!(
        focus,
        Some(PlatformEvent::FocusChanged {
            pid: std::process::id(),
            by_activation: true
        })
    );
}

#[test]
fn window_lists_follow_creation_and_closing() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(120.0, 120.0, 300.0, 200.0));
    let created = desktop.expect(|event| matches!(event, PlatformEvent::WindowCreated { .. }));
    assert_eq!(
        created,
        Some(PlatformEvent::WindowCreated {
            pid: std::process::id()
        })
    );
    assert!(
        desktop
            .platform
            .visible_windows(Some("Fixture"))
            .iter()
            .any(|info| info.id == id(window))
    );
    assert!(desktop.platform.visible_windows(Some("Other")).is_empty());
    assert!(
        desktop
            .platform
            .all_windows()
            .iter()
            .any(|info| info.id == id(window))
    );

    let frame = desktop.platform.frame(id(window)).unwrap();
    let occupying = desktop.platform.windows_occupying(&[frame]).unwrap();
    assert_eq!(
        occupying.iter().map(|info| info.id).collect::<Vec<_>>(),
        [id(window)]
    );

    assert!(!desktop.platform.is_closed(id(window)));
    desktop.close(window);
    assert!(wait_until(|| desktop.platform.is_closed(id(window))));
    let destroyed = desktop.expect(|event| matches!(event, PlatformEvent::WindowDestroyed(_)));
    assert_eq!(destroyed, Some(PlatformEvent::WindowDestroyed(id(window))));
}

#[test]
fn minimized_windows_are_reported_and_not_visible() {
    let mut desktop = desktop!();
    let window = desktop.open_plain(Rect::new(140.0, 140.0, 300.0, 200.0));
    assert!(!desktop.platform.state(id(window)).unwrap().minimized);
    desktop.set_state(window, true, ["_NET_WM_STATE_HIDDEN", "_NET_WM_STATE_HIDDEN"]);
    let event = ClientMessageEvent::new(32, window, desktop.atom("WM_CHANGE_STATE"), [3, 0, 0, 0, 0]);
    let mask = EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY;
    desktop
        .client
        .send_event(false, desktop.root, mask, event)
        .unwrap();
    desktop.client.flush().unwrap();
    assert!(wait_until(|| desktop
        .platform
        .state(id(window))
        .is_ok_and(|state| state.minimized)));
    assert!(
        !desktop
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == id(window))
    );
    assert!(
        desktop
            .platform
            .all_windows()
            .iter()
            .any(|info| info.id == id(window))
    );
}
