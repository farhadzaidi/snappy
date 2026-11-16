//! Displays, primary first, and their work areas without panel struts.

use snappy_core::geometry::Rect;
use snappy_platform::{PlatformEvent, WindowSystem};
use x11rb::protocol::xproto::{AtomEnum, PropMode};
use x11rb::wrapper::ConnectionExt as _;

use crate::support::{desktop, wait_until};

#[test]
fn displays_list_both_monitors_primary_first() {
    let mut desktop = desktop!();
    let displays = desktop.platform.displays();
    let frames: Vec<Rect> = displays.iter().map(|display| display.frame).collect();
    assert_eq!(
        frames,
        [
            Rect::new(0.0, 0.0, 1920.0, 1080.0),
            Rect::new(1920.0, 0.0, 1920.0, 1080.0)
        ]
    );
    assert_eq!(displays[0].id, "DUMMY0");
    assert_eq!(displays[1].id, "DUMMY1");
    assert!(displays.iter().all(|display| display.scale == 1.0));
}

#[test]
fn work_areas_exclude_panel_struts() {
    let mut desktop = desktop!();
    let before = desktop.platform.displays();
    desktop.open(Rect::new(0.0, 0.0, 1920.0, 30.0), |desktop, window| {
        let dock = desktop.atom("_NET_WM_WINDOW_TYPE_DOCK");
        let kind = desktop.atom("_NET_WM_WINDOW_TYPE");
        let strut = desktop.atom("_NET_WM_STRUT_PARTIAL");
        let conn = &desktop.client;
        conn.change_property32(PropMode::REPLACE, window, kind, AtomEnum::ATOM, &[dock])
            .unwrap();
        let top_panel = [0, 0, 30, 0, 0, 0, 0, 0, 0, 1919, 0, 0];
        conn.change_property32(PropMode::REPLACE, window, strut, AtomEnum::CARDINAL, &top_panel)
            .unwrap();
    });
    let mut areas = Vec::new();
    wait_until(|| {
        areas = desktop
            .platform
            .displays()
            .iter()
            .map(|display| display.work_area)
            .collect();
        areas[0] != before[0].work_area
    });
    assert_eq!(areas[0], Rect::new(0.0, 30.0, 1920.0, 1050.0));
    assert_eq!(areas[1], before[1].work_area);
    let events = desktop.drain();
    assert!(events.contains(&PlatformEvent::DisplaysChanged), "{events:?}");
}
