//! Listing, moving, resizing, and finding windows, and following them come and go.

use std::time::{Duration, Instant};

use serde_json::json;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_platform::{PlatformEvent, WindowSystem};

use crate::support::{WAIT, floating, shell};

#[test]
fn windows_are_listed_moved_resized_and_found_under_the_pointer() {
    let mut shell = shell!();
    let id = shell.open("move", floating(200.0));
    let window = snappy_platform::WindowId(id);
    assert_eq!(shell.platform.focused_window().unwrap().id, window);
    assert_eq!(
        shell.platform.window_at(Point::new(300.0, 400.0)).unwrap().id,
        window
    );
    let info = shell
        .platform
        .all_windows()
        .into_iter()
        .find(|info| info.id == window)
        .unwrap();
    assert!(info.pid > 0);
    assert!(info.recovery_key.is_some());

    shell
        .platform
        .set_position(window, Point::new(100.0, 150.0))
        .unwrap();
    shell.wait_frame(id, "its new position", |frame| {
        frame.x == 100.0 && frame.y == 150.0
    });
    shell.platform.set_size(window, Size::new(700.0, 500.0)).unwrap();
    let frame = shell.wait_frame(id, "its new size", |frame| {
        frame.width == 700.0 && frame.height == 500.0
    });
    assert_eq!((frame.x, frame.y), (100.0, 150.0), "resizing keeps the corner");
    assert!(!shell.platform.is_closed(window));

    shell.fixtures.clear();
    let deadline = Instant::now() + WAIT;
    while !shell.platform.is_closed(window) {
        assert!(Instant::now() < deadline, "the window never closed");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(shell.platform.frame(window).is_err());
}

#[test]
fn a_maximized_window_is_restored_before_it_moves() {
    let mut shell = shell!();
    let id = shell.open("maximized", floating(300.0));
    shell.helper(json!({"op": "maximize", "title": "Fixture maximized"}));
    shell.wait_frame(id, "maximized", |frame| frame.width > 1800.0);
    let window = snappy_platform::WindowId(id);
    shell.platform.set_size(window, Size::new(640.0, 480.0)).unwrap();
    shell
        .platform
        .set_position(window, Point::new(50.0, 100.0))
        .unwrap();
    shell.wait_frame(id, "a restored frame", |frame| {
        frame == Rect::new(50.0, 100.0, 640.0, 480.0)
    });
}

#[test]
fn windows_appearing_and_closing_report_their_process() {
    let mut shell = shell!();
    shell.platform.all_windows();
    shell.drain();
    shell.open("lifecycle", floating(400.0));
    let events = shell.wait_for("the window appearing", |event| {
        matches!(event, PlatformEvent::WindowCreated { .. })
    });
    let pid = events
        .iter()
        .find_map(|event| match event {
            PlatformEvent::WindowCreated { pid } => Some(*pid),
            _ => None,
        })
        .unwrap();
    assert!(events.contains(&PlatformEvent::AppLaunched { pid }));
    shell.fixtures.clear();
    shell.wait_for("the app quitting", |event| {
        *event == PlatformEvent::AppTerminated { pid }
    });
}
