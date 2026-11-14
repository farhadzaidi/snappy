//! Moving keyboard focus between windows and zones.

use std::time::Duration;

use snappy_core::geometry::Rect;
use snappy_engine::Engine;
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_platform::{MarkStyle, PlatformEvent, WindowId};

use crate::support::{
    LEFT_ZONE, RIGHT_ZONE, default_workspaces, event, focus, focused, frame_of, hotkey_id, live_engine,
    logged, marked, marks, press_default, scenario, settle, side_display, side_third, snappy, status_text,
    thirds_on_the_side, two_zoned_windows,
};

/// Window 1 in the middle of the main display, focused, with a window on each side of it:
/// 2 to the left, 3 to the right, 4 above, and 5 below.
fn around_a_center() -> FakePlatform {
    let mut fake = FakePlatform::default();
    let frames = [
        (2, Rect::new(60.0, 375.0, 240.0, 150.0)),
        (3, Rect::new(1140.0, 375.0, 240.0, 150.0)),
        (4, Rect::new(600.0, 60.0, 240.0, 150.0)),
        (5, Rect::new(600.0, 700.0, 240.0, 150.0)),
        (1, Rect::new(600.0, 375.0, 240.0, 150.0)),
    ];
    for (id, frame) in frames {
        fake.open(id, FakeWindow::new("com.example.editor", 100, frame));
    }
    fake
}

fn frontmost(engine: &Engine<FakePlatform>) -> WindowId {
    engine.with_platform(|p| p.stacking[0])
}

#[test]
fn focus_shortcuts_focus_and_raise_the_nearest_window_in_each_direction() {
    scenario(|| async {
        let engine = live_engine("focus-directions", around_a_center());
        for (key, expected) in [("KeyL", 3), ("KeyJ", 2), ("KeyI", 4), ("KeyK", 5)] {
            focus(&engine, 1);
            press_default(&engine, key).await;
            assert_eq!(focused(&engine), Some(WindowId(expected)), "{key}");
            assert_eq!(frontmost(&engine), WindowId(expected), "{key} raises it");
        }
        assert_eq!(
            frame_of(&engine, 1),
            Rect::new(600.0, 375.0, 240.0, 150.0),
            "nothing moves"
        );
    });
}

#[test]
fn a_focus_shortcut_with_nothing_that_way_leaves_focus_and_says_so_quietly() {
    scenario(|| async {
        let engine = live_engine("focus-nothing-there", around_a_center());
        focus(&engine, 3);
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(3)), "focus never wraps around");
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("No window to the right.", false, false)
        );
        assert!(marks(&engine).is_empty());
    });
}

#[test]
fn with_nothing_focused_focus_moves_from_the_middle_of_the_display_under_the_pointer() {
    scenario(|| async {
        let engine = live_engine("focus-from-pointer", around_a_center());
        engine.with_platform(|p| p.focused = None);
        press_default(&engine, "KeyL").await;
        assert_eq!(
            focused(&engine),
            Some(WindowId(3)),
            "window 1 sits on the middle, not past it"
        );
    });
}

#[test]
fn equally_near_windows_go_to_the_one_focused_most_recently() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        let window =
            |x: f64, y: f64| FakeWindow::new("com.example.editor", 100, Rect::new(x, y, 240.0, 150.0));
        fake.open(2, window(700.0, 225.0));
        fake.open(3, window(700.0, 525.0));
        fake.open(1, window(100.0, 375.0));
        let engine = default_workspaces("focus-ties", fake);
        for (order, expected) in [([2, 3], 3), ([3, 2], 2)] {
            for window in order {
                focus(&engine, window);
            }
            focus(&engine, 1);
            press_default(&engine, "KeyL").await;
            assert_eq!(focused(&engine), Some(WindowId(expected)));
        }
    });
}

/// Window 1 focused in the left zone, window 2 in the right zone, and window 3 floating
/// between them, a little lower, where the nearest window by center would be it.
fn zoned_with_a_floating_window_between() -> FakePlatform {
    let mut fake = two_zoned_windows();
    let between = Rect::new(570.0, 400.0, 300.0, 200.0);
    fake.open(3, FakeWindow::new("com.example.notes", 300, between));
    fake.focused = Some(WindowId(1));
    fake
}

#[test]
fn with_windows_in_zones_focus_moves_between_zones_past_a_floating_window() {
    scenario(|| async {
        let name = "focus-zones";
        let engine = live_engine(name, zoned_with_a_floating_window_between());
        engine.force_debug_log();
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(2)));
        press_default(&engine, "KeyJ").await;
        assert_eq!(focused(&engine), Some(WindowId(1)));
        press_default(&engine, "KeyK").await;
        assert_eq!(
            focused(&engine),
            Some(WindowId(1)),
            "the floating window below is skipped"
        );
        assert_eq!(status_text(&engine), "No window below.");

        let moves = logged(name, "focusMove");
        assert_eq!(moves.len(), 3);
        assert!(moves.iter().all(|record| record["mode"] == "zones"), "{moves:?}");
        assert_eq!(
            moves[0]["zone"],
            serde_json::json!({ "display": "main", "zone": "halves-1" })
        );
        assert_eq!(moves[2]["zone"], serde_json::Value::Null);
    });
}

#[test]
fn from_a_floating_focused_window_focus_goes_to_the_zone_ahead() {
    scenario(|| async {
        let engine = live_engine("focus-from-floating", zoned_with_a_floating_window_between());
        for (key, expected) in [("KeyL", 2), ("KeyJ", 1)] {
            focus(&engine, 3);
            press_default(&engine, key).await;
            assert_eq!(focused(&engine), Some(WindowId(expected)), "{key}");
        }
    });
}

#[test]
fn of_windows_stacked_in_a_zone_focus_goes_to_the_most_recently_focused() {
    scenario(|| async {
        let mut fake = two_zoned_windows();
        fake.open(4, FakeWindow::new("com.example.notes", 300, RIGHT_ZONE));
        let engine = default_workspaces("focus-stacked", fake);
        for (order, expected) in [([2, 4], 4), ([4, 2], 2)] {
            for window in order {
                focus(&engine, window);
            }
            focus(&engine, 1);
            press_default(&engine, "KeyL").await;
            assert_eq!(focused(&engine), Some(WindowId(expected)));
        }
    });
}

#[test]
fn focus_moves_between_zones_across_displays_skipping_empty_ones() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.displays.push(side_display());
        fake.open(2, FakeWindow::new("com.example.browser", 200, side_third(1)));
        fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
        let engine = live_engine("focus-across-displays", fake);
        thirds_on_the_side(&engine);
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(2)));
        press_default(&engine, "KeyJ").await;
        assert_eq!(focused(&engine), Some(WindowId(1)));
    });
}

#[test]
fn with_no_window_in_a_zone_focus_moves_by_window_centers() {
    scenario(|| async {
        let name = "focus-windows-mode";
        let engine = live_engine(name, around_a_center());
        engine.force_debug_log();
        focus(&engine, 1);
        press_default(&engine, "KeyL").await;
        assert_eq!(focused(&engine), Some(WindowId(3)));
        let moves = logged(name, "focusMove");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0]["mode"], "windows");
        assert_eq!(moves[0]["zone"], serde_json::Value::Null);
    });
}

#[test]
fn the_newly_focused_window_flashes_an_outline_of_its_own_that_then_goes() {
    scenario(|| async {
        let engine = live_engine("focus-flash", around_a_center());
        let right = frame_of(&engine, 3);
        let middle = frame_of(&engine, 1);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyL"))),
        );
        assert_eq!(marked(&engine), [(right, MarkStyle::Focused)]);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(marked(&engine), [(right, MarkStyle::Focused)], "still showing");
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(marks(&engine).is_empty(), "gone after about 400 ms");

        press_default(&engine, "Enter").await;
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyJ"))),
        );
        assert_eq!(
            marked(&engine),
            [(right, MarkStyle::Selected), (middle, MarkStyle::Focused)]
        );
        settle(&engine).await;
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyL"))),
        );
        assert_eq!(
            marked(&engine),
            [(right, MarkStyle::Focused)],
            "the flash shows over the selection"
        );
        settle(&engine).await;
        assert_eq!(marked(&engine), [(right, MarkStyle::Selected)]);
    });
}
