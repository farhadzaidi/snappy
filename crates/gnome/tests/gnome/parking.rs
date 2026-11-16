//! Parking windows on Snappy's workspace, and following GNOME's own workspaces.

use std::time::Duration;

use serde_json::json;
use snappy_core::geometry::Point;
use snappy_core::keys::{KeyChord, Modifier};
use snappy_platform::{DesktopGuard, PlatformEvent, WindowSystem};

use crate::support::{floating, shell};

#[test]
fn parking_hides_a_window_from_the_screen_and_the_switcher_and_showing_returns_it() {
    let mut shell = shell!();
    let id = shell.open("parked", floating(250.0));
    let window = snappy_platform::WindowId(id);
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    assert!(shell.platform.on_managed_desktop());

    shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap();
    assert!(shell.platform.state(window).unwrap().on_parking_desktop);
    assert!(
        shell
            .platform
            .visible_windows(None)
            .iter()
            .all(|info| info.id != window)
    );
    assert!(shell.platform.all_windows().iter().any(|info| info.id == window));
    let switcher = shell.helper(json!({"op": "tabList"}));
    assert!(
        !switcher
            .as_array()
            .unwrap()
            .iter()
            .any(|title| title == "Fixture parked"),
        "Alt+Tab offers a parked window: {switcher}"
    );
    assert!(
        shell.platform.on_managed_desktop(),
        "parking doesn't leave the workspace"
    );

    shell
        .platform
        .show_for_workspace(window, Point::new(120.0, 140.0))
        .unwrap();
    assert!(!shell.platform.state(window).unwrap().on_parking_desktop);
    shell.wait_frame(id, "the saved position", |frame| {
        frame.x == 120.0 && frame.y == 140.0
    });
    assert!(
        shell
            .platform
            .visible_windows(None)
            .iter()
            .any(|info| info.id == window)
    );
    let switcher = shell.helper(json!({"op": "tabList"}));
    assert!(
        switcher
            .as_array()
            .unwrap()
            .iter()
            .any(|title| title == "Fixture parked")
    );
}

#[test]
fn with_dynamic_workspaces_the_parking_workspace_sits_before_gnomes_empty_last_one() {
    let mut shell = shell!();
    let before = shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap();
    let id = shell.open("dynamic", floating(260.0));
    let window = snappy_platform::WindowId(id);
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap(),
        before + 1,
        "GNOME keeps one empty workspace after the parking one"
    );
    shell
        .platform
        .show_for_workspace(window, floating(260.0).origin())
        .unwrap();
    shell.platform.clear();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap(),
        before
    );
}

#[test]
fn with_a_fixed_number_of_workspaces_parking_adds_one_and_clearing_removes_it() {
    let mut shell = shell!();
    shell.helper(json!({"op": "setDynamicWorkspaces", "on": false}));
    std::thread::sleep(Duration::from_millis(300));
    let before = shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap();
    let id = shell.open("fixed", floating(270.0));
    let window = snappy_platform::WindowId(id);
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap();
    assert_eq!(
        shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap(),
        before + 1
    );
    shell
        .platform
        .show_for_workspace(window, floating(270.0).origin())
        .unwrap();
    shell.platform.clear();
    std::thread::sleep(Duration::from_millis(300));
    let after = shell.helper(json!({"op": "workspaceCount"})).as_u64().unwrap();
    shell.helper(json!({"op": "setDynamicWorkspaces", "on": true}));
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(after, before);
}

#[test]
fn a_window_on_every_workspace_is_refused_with_the_reason() {
    let mut shell = shell!();
    let id = shell.open("sticky", floating(280.0));
    let window = snappy_platform::WindowId(id);
    shell.helper(json!({"op": "stick", "title": "Fixture sticky", "on": true}));
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    let error = shell
        .platform
        .hide_for_workspace(window, Point::default())
        .unwrap_err();
    shell.helper(json!({"op": "stick", "title": "Fixture sticky", "on": false}));
    assert!(error.to_string().contains("every workspace"), "{error}");
}

#[test]
fn switching_gnome_workspaces_pauses_and_coming_back_resumes() {
    let mut shell = shell!();
    let displays = shell.platform.displays();
    shell.platform.bind(&displays);
    shell.drain();
    shell.press_chord(&KeyChord::new("PageDown", [Modifier::Meta]));
    shell.wait_for("the workspace change", |event| {
        *event == PlatformEvent::NativeDesktopChanged
    });
    assert!(!shell.platform.on_managed_desktop());
    shell.press_chord(&KeyChord::new("PageUp", [Modifier::Meta]));
    shell.wait_for("the way back", |event| {
        *event == PlatformEvent::NativeDesktopChanged
    });
    assert!(shell.platform.on_managed_desktop());
}
