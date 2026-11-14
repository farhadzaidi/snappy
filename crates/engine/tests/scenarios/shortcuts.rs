//! Zone and arrow shortcuts, and registering shortcuts.

use snappy_core::geometry::Rect;
use snappy_core::keys::SnappyKey;
use snappy_core::settings::DragModifier;
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, command, default_workspaces, event, frame_of, hotkey_id,
    inactive_window_in_the_right_zone, live_engine, logged, place, press_default, scenario, settle,
    side_display, side_third, snappy, thirds_on_the_side, two_zoned_windows, with_window, writes_to,
};

#[test]
fn the_debug_log_names_each_shortcut_by_its_action() {
    scenario(|| async {
        let name = "shortcut-names";
        let engine = default_workspaces(name, two_zoned_windows());
        engine.force_debug_log();
        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        command(&engine, Command::SetWorkspacesEnabled { enabled: true });
        press_default(&engine, "KeyJ").await;
        press_default(&engine, "Digit2").await;

        let pressed = logged(name, "shortcut");
        let pressed: Vec<(&str, Option<&str>, bool)> = pressed
            .iter()
            .map(|record| {
                (
                    record["action"].as_str().unwrap(),
                    record["workspace"].as_str(),
                    record["live"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            pressed,
            [
                ("focusLeft", None, true),
                ("switchWorkspace", Some("workspace-2"), true)
            ]
        );
        let registered = logged(name, "shortcutsRegistered");
        let shortcuts = registered.last().unwrap()["shortcuts"].as_array().unwrap();
        assert!(shortcuts.contains(&serde_json::json!({ "chord": "Snappy + J", "action": "focusLeft" })));
        assert!(shortcuts.contains(&serde_json::json!({
            "chord": "Snappy + 3",
            "action": "switchWorkspace",
            "workspace": "workspace-3",
        })));
    });
}

#[test]
fn zone_shortcuts_move_the_focused_window() {
    scenario(|| async {
        let engine = live_engine(
            "zone-shortcut",
            with_window(Rect::new(300.0, 200.0, 600.0, 400.0)),
        );
        let id = hotkey_id(&engine, &snappy("Digit2"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
    });
}

#[test]
fn arrow_shortcuts_move_between_zones() {
    scenario(|| async {
        let engine = live_engine("arrow-shortcut", with_window(LEFT_ZONE));
        let id = hotkey_id(&engine, &snappy("ArrowRight"));
        event(&engine, PlatformEvent::HotKey(id));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
    });
}

#[test]
fn recording_unregisters_every_shortcut() {
    scenario(|| async {
        let engine = live_engine("recording", FakePlatform::default());
        assert!(!engine.with_platform(|p| p.registered.is_empty()));
        command(&engine, Command::BeginRecording);
        assert!(engine.with_platform(|p| p.registered.is_empty()));
        command(&engine, Command::EndRecording);
        assert!(!engine.with_platform(|p| p.registered.is_empty()));
    });
}

#[test]
fn disabled_layouts_register_no_zone_shortcuts() {
    scenario(|| async {
        let engine = live_engine("disabled-layout-shortcuts", FakePlatform::default());
        command(
            &engine,
            Command::SetLayoutEnabled {
                layout: "halves".into(),
                enabled: false,
            },
        );
        let digit_one = snappy("Digit1").combo(SnappyKey::DEFAULT);
        let focus_zones =
            engine.with_platform(|p| p.registered.iter().filter(|(_, c)| *c == digit_one).count());
        assert_eq!(
            focus_zones, 1,
            "the display falls back to another enabled layout's zones"
        );
        assert!(
            engine.settings().layouts.options["halves"]
                .zone_shortcuts
                .contains_key("halves-0"),
            "bindings are kept"
        );
    });
}

#[test]
fn arrow_shortcuts_ignore_inactive_workspace_windows() {
    scenario(|| async {
        let engine = inactive_window_in_the_right_zone("arrow-inactive").await;
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        let before = writes_to(&engine, 2);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowLeft"))),
        );
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), RIGHT_ZONE);
        assert_eq!(writes_to(&engine, 2), before);
    });
}

#[test]
fn shortcuts_do_nothing_on_a_display_whose_layout_is_disabled() {
    scenario(|| async {
        let engine = live_engine("arrow-disabled-layout", with_window(LEFT_ZONE));
        for layout in snappy_core::layout::presets() {
            command(
                &engine,
                Command::SetLayoutEnabled {
                    layout: layout.id.clone(),
                    enabled: false,
                },
            );
        }
        let status = engine.status();
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("ArrowRight"))),
        );
        settle(&engine).await;
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(engine.status(), status, "no message either");
    });
}

#[test]
fn zone_shortcuts_move_the_focused_window_not_the_frontmost() {
    scenario(|| async {
        let mut fake = with_window(Rect::new(300.0, 200.0, 600.0, 400.0));
        let other = Rect::new(400.0, 300.0, 600.0, 400.0);
        fake.open(2, FakeWindow::new("com.example.browser", 200, other));
        fake.focused = Some(WindowId(1));
        let engine = live_engine("focused-not-frontmost", fake);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(2))), other);
    });
}

#[test]
fn a_zone_shortcut_without_a_focused_window_says_so() {
    scenario(|| async {
        let mut fake = with_window(Rect::new(300.0, 200.0, 600.0, 400.0));
        fake.focused = None;
        let engine = live_engine("no-focused-window", fake);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        settle(&engine).await;
        let status = engine.status().unwrap();
        assert!(status.is_error);
        assert_eq!(status.message, "No movable window is focused.");
        assert_eq!(writes_to(&engine, 1), 0);
    });
}

#[test]
fn zone_shortcuts_follow_each_displays_layout_and_can_share_chords() {
    scenario(|| async {
        let mut fake = with_window(FLOATING);
        fake.displays.push(side_display());
        let engine = live_engine("per-display-shortcuts", fake);
        thirds_on_the_side(&engine);
        let registered = |code: &str| {
            let combo = snappy(code).combo(SnappyKey::DEFAULT);
            engine.with_platform(|p| p.registered.iter().filter(|(_, c)| *c == combo).count())
        };
        assert_eq!((registered("Digit1"), registered("Digit3")), (1, 1));

        let third = hotkey_id(&engine, &snappy("Digit3"));
        let status = engine.status();
        event(&engine, PlatformEvent::HotKey(third));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), FLOATING, "halves has no third zone");
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(engine.status(), status);

        place(&engine, 1, Rect::new(1600.0, 200.0, 600.0, 400.0));
        event(&engine, PlatformEvent::HotKey(third));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), side_third(2));
    });
}

#[test]
fn shortcuts_are_registered_again_only_when_the_set_changes_or_failed() {
    scenario(|| async {
        let engine = live_engine("reregister", FakePlatform::default());
        let registrations = || engine.with_platform(|p| p.registrations);
        let start = registrations();
        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Alt,
            },
        );
        assert_eq!(registrations(), start, "the same set stays registered");

        engine.with_platform(|p| {
            p.taken_elsewhere
                .insert(snappy("Digit3").combo(SnappyKey::DEFAULT))
        });
        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "thirds".into(),
            },
        );
        assert_eq!(registrations(), start + 1);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            ("Another app is using Snappy + 3.", true)
        );

        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Ctrl,
            },
        );
        assert_eq!(
            registrations(),
            start + 2,
            "a failure is retried on the next change"
        );
        engine.with_platform(|p| p.taken_elsewhere.clear());
        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Shift,
            },
        );
        assert_eq!(registrations(), start + 3);
        command(
            &engine,
            Command::SetDragModifier {
                modifier: DragModifier::Alt,
            },
        );
        assert_eq!(registrations(), start + 3);
    });
}
