//! The overlay showing above windows without taking focus.

use std::time::Duration;

use serde_json::json;
use snappy_core::geometry::Rect;
use snappy_platform::{Caption, Overlay, OverlayScene, OverlayZone, WindowSystem};

use crate::support::{floating, shell};

fn scene(work_area: Rect) -> OverlayScene {
    OverlayScene {
        work_area,
        zones: vec![OverlayZone {
            frame: Rect::new(work_area.x + 12.0, work_area.y + 12.0, 600.0, 400.0),
            number: 1,
            highlighted: true,
            caption: Some(Caption::Swap),
            swap_source: false,
        }],
    }
}

#[test]
fn the_overlay_shows_above_windows_without_taking_focus_and_hides() {
    let mut shell = shell!();
    shell.open("under", floating(300.0));
    let display = shell.platform.displays().remove(0);
    shell.platform.show(&display.id, &scene(display.work_area));
    std::thread::sleep(Duration::from_millis(200));
    let shown = shell.helper(json!({"op": "screenWindows"}));
    let front = &shown.as_array().unwrap()[0];
    assert_eq!(front["overlay"], true, "{shown}");
    assert_eq!(front["frame"]["y"].as_f64(), Some(display.work_area.y));
    let focused = shown
        .as_array()
        .unwrap()
        .iter()
        .find(|window| window["focused"] == true)
        .map(|window| window["frame"]["x"].as_f64());
    assert_eq!(focused, Some(Some(300.0)), "the window keeps focus");

    shell.platform.hide();
    std::thread::sleep(Duration::from_millis(200));
    let shown = shell.helper(json!({"op": "screenWindows"}));
    assert!(
        shown
            .as_array()
            .unwrap()
            .iter()
            .all(|window| window["overlay"] == false)
    );
}
