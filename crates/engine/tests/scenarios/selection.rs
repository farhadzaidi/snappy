//! Selecting windows, taking them along, sending them, and placing them in empty zones.

use snappy_core::geometry::Rect;
use snappy_core::keys::SnappyKey;
use snappy_core::settings::{NavigationShortcut, Settings};
use snappy_engine::Engine;
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_ipc::Command;
use snappy_platform::{MarkStyle, PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, active, command, default_workspaces, event, focus, focused,
    four_windows, frame_of, inactive_window_in_the_right_zone, is_parked, live_engine, logged, marked, marks,
    paths, place, press_default, press_nearest_empty, scenario, settle, snappy, status_text,
    two_zoned_windows, undo, window_counts, writes_to,
};

#[test]
fn a_selected_window_goes_along_to_the_workspace_switched_to_by_number() {
    scenario(|| async {
        let engine = default_workspaces("take-by-number", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(
            marked(&engine),
            [(LEFT_ZONE, MarkStyle::Selected)],
            "the selected window is outlined"
        );

        press_default(&engine, "Digit3").await;
        assert_eq!(active(&engine), "workspace-3");
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "it keeps its frame");
        assert!(is_parked(&engine, 2));
        assert_eq!(focused(&engine), Some(WindowId(1)));
        assert_eq!(window_counts(&engine)["workspace-3"], 1);
        assert!(marks(&engine).is_empty(), "arrived windows lose their outline");

        press_default(&engine, "Digit1").await;
        assert!(is_parked(&engine, 1), "the switch emptied the selection");
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
    });
}

#[test]
fn two_selected_windows_go_along_to_the_next_workspace_and_the_last_selected_gets_focus() {
    scenario(|| async {
        let engine = default_workspaces("take-two-by-step", four_windows());
        let frames: Vec<Rect> = (1..=4).map(|window| frame_of(&engine, window)).collect();
        focus(&engine, 3);
        press_default(&engine, "Enter").await;
        focus(&engine, 1);
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [frames[2], frames[0]]);

        press_default(&engine, "Quote").await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 3)),
            (frames[0], frames[2])
        );
        assert!(is_parked(&engine, 2) && is_parked(&engine, 4));
        assert_eq!(focused(&engine), Some(WindowId(1)));
        assert_eq!(window_counts(&engine)["workspace-2"], 2);
        assert!(marks(&engine).is_empty());
    });
}

#[test]
fn selecting_a_selected_window_again_unselects_it() {
    scenario(|| async {
        let engine = default_workspaces("put-down", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        press_default(&engine, "Enter").await;
        assert!(marks(&engine).is_empty());

        press_default(&engine, "Digit2").await;
        assert!(is_parked(&engine, 1) && is_parked(&engine, 2));
        assert_eq!(window_counts(&engine)["workspace-1"], 2);
    });
}

#[test]
fn a_selected_window_that_closes_leaves_the_selection() {
    scenario(|| async {
        let engine = default_workspaces("taken-along-closed", two_zoned_windows());
        press_default(&engine, "Enter").await;
        focus(&engine, 2);
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE, RIGHT_ZONE]);

        engine.with_platform(|p| {
            p.windows.remove(&WindowId(1));
            p.focused = Some(WindowId(2));
        });
        event(&engine, PlatformEvent::WindowDestroyed(WindowId(1)));
        assert_eq!(marks(&engine), [RIGHT_ZONE]);

        press_default(&engine, "Digit2").await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
    });
}

#[test]
fn a_selected_windows_outline_follows_it_and_survives_a_switch_to_the_same_workspace() {
    scenario(|| async {
        let engine = default_workspaces("taken-along-moves", two_zoned_windows());
        press_default(&engine, "Enter").await;
        let moved = Rect::new(100.0, 120.0, 500.0, 400.0);
        place(&engine, 1, moved);
        event(&engine, PlatformEvent::WindowMoved(WindowId(1)));
        assert_eq!(marks(&engine), [moved]);

        press_default(&engine, "Digit1").await;
        assert_eq!(marks(&engine), [moved], "already there: still selected");

        event(
            &engine,
            PlatformEvent::Tray(snappy_platform::TrayCommand::SwitchWorkspace(
                "workspace-2".into(),
            )),
        );
        settle(&engine).await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(
            frame_of(&engine, 1),
            moved,
            "the menu's switch takes it along too"
        );
        assert!(marks(&engine).is_empty());
    });
}

#[test]
fn quitting_forgets_the_selection_and_moves_none_of_it() {
    scenario(|| async {
        let engine = default_workspaces("taken-along-quit", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        engine.shutdown();
        assert!(marks(&engine).is_empty());
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn selecting_works_with_workspaces_off_and_backslash_clears_the_selection() {
    scenario(|| async {
        let engine = live_engine("select-without-workspaces", two_zoned_windows());
        assert!(!engine.settings().workspaces.enabled);
        press_default(&engine, "Enter").await;
        focus(&engine, 2);
        press_default(&engine, "Enter").await;
        assert_eq!(
            marked(&engine),
            [
                (LEFT_ZONE, MarkStyle::Selected),
                (RIGHT_ZONE, MarkStyle::Selected)
            ]
        );

        press_default(&engine, "Backslash").await;
        assert!(marks(&engine).is_empty());
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, RIGHT_ZONE),
            "clearing moves nothing"
        );
    });
}

#[test]
fn a_macos_file_clearing_with_delete_loads_clearing_with_backslash_and_delete_is_refused() {
    scenario(|| async {
        let name = "clear-with-delete";
        let paths = paths(name);
        let mut written =
            serde_json::to_value(Settings::defaults(snappy_core::keys::Platform::MacOs)).unwrap();
        written["navigation"]["clearSelection"] = serde_json::to_value(snappy("Backspace")).unwrap();
        std::fs::create_dir_all(paths.settings.parent().unwrap()).unwrap();
        std::fs::write(&paths.settings, serde_json::to_vec(&written).unwrap()).unwrap();

        let engine = Engine::new(two_zoned_windows(), paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        assert!(engine.status().is_none_or(|status| !status.is_error));
        assert_eq!(
            engine
                .settings()
                .navigation
                .clear_selection
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            Some(snappy("Backslash").combo(SnappyKey::DEFAULT))
        );
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        press_default(&engine, "Backslash").await;
        assert!(marks(&engine).is_empty());

        command(
            &engine,
            Command::SetNavigationShortcut {
                shortcut: NavigationShortcut::ClearSelection,
                chord: Some(snappy("Backspace")),
            },
        );
        let status = engine.status().unwrap();
        assert!(status.is_error);
        assert!(
            status.message.ends_with(
                "macOS reserves Control ⌃ + Option ⌥ + Command ⌘ + Delete; it never reaches apps."
            ),
            "{}",
            status.message
        );
        assert_eq!(
            engine
                .settings()
                .navigation
                .clear_selection
                .map(|chord| chord.combo(SnappyKey::DEFAULT)),
            Some(snappy("Backslash").combo(SnappyKey::DEFAULT))
        );
    });
}

#[test]
fn losing_live_control_clears_the_selection_and_its_shortcuts() {
    scenario(|| async {
        let engine = live_engine("select-live-off", two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        command(&engine, Command::SetSnapping { enabled: false });
        assert!(marks(&engine).is_empty());
        assert!(
            !engine.with_platform(|p| p
                .registered
                .iter()
                .any(|(_, combo)| *combo == snappy("Enter").combo(SnappyKey::DEFAULT))),
            "with live control off, Snappy + Return stays the app's"
        );
    });
}

#[test]
fn changing_desktops_clears_the_selection() {
    scenario(|| async {
        let engine = default_workspaces("taken-along-paused", two_zoned_windows());
        press_default(&engine, "Enter").await;
        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        assert!(marks(&engine).is_empty());
        engine.with_platform(|p| p.on_managed_desktop = true);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        press_default(&engine, "Digit2").await;
        assert!(is_parked(&engine, 1));
    });
}

#[test]
fn sending_one_selected_window_away_empties_the_selection() {
    scenario(|| async {
        let engine = default_workspaces("taken-along-sent", two_zoned_windows());
        press_default(&engine, "Enter").await;
        press_default(&engine, "Slash").await;
        assert!(is_parked(&engine, 1));
        assert!(marks(&engine).is_empty());
        assert_eq!(focused(&engine), Some(WindowId(2)));
        press_default(&engine, "Digit3").await;
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        assert_eq!(window_counts(&engine)["workspace-3"], 0);
    });
}

#[test]
fn send_and_stay_sends_the_whole_selection_and_focuses_what_is_left() {
    scenario(|| async {
        let engine = default_workspaces("send-selection", four_windows());
        for window in [2, 4] {
            focus(&engine, window);
            press_default(&engine, "Enter").await;
        }
        focus(&engine, 3);
        focus(&engine, 1);

        press_default(&engine, "Slash").await;
        assert_eq!(active(&engine), "workspace-1", "the send stays");
        assert!(is_parked(&engine, 2) && is_parked(&engine, 4));
        assert!(!is_parked(&engine, 1) && !is_parked(&engine, 3));
        assert_eq!(window_counts(&engine)["workspace-2"], 2);
        assert!(marks(&engine).is_empty(), "sending empties the selection");
        assert_eq!(focused(&engine), Some(WindowId(1)));

        press_default(&engine, "Slash").await;
        assert!(
            is_parked(&engine, 1),
            "without a selection the focused window goes"
        );
        assert!(!is_parked(&engine, 3));
    });
}

#[test]
fn the_debug_log_records_the_selection_and_sending() {
    scenario(|| async {
        let name = "selection-log";
        let engine = default_workspaces(name, two_zoned_windows());
        engine.force_debug_log();
        press_default(&engine, "Enter").await;
        press_default(&engine, "Enter").await;
        press_default(&engine, "Enter").await;
        press_default(&engine, "Digit2").await;
        press_default(&engine, "Period").await;
        press_default(&engine, "Enter").await;
        press_default(&engine, "Backslash").await;
        press_default(&engine, "KeyL").await;
        for event in [
            "selectionAdded",
            "selectionRemoved",
            "selectionTakenAlong",
            "selectionCleared",
            "workspaceSend",
            "focusMove",
        ] {
            assert!(!logged(name, event).is_empty(), "{event}");
        }
    });
}

#[test]
fn placing_the_selection_ignores_windows_of_inactive_workspaces() {
    scenario(|| async {
        let engine = inactive_window_in_the_right_zone("place-selection-inactive").await;
        engine.with_platform(|p| {
            p.open(3, FakeWindow::new("com.example.notes", 300, FLOATING));
            p.focused = Some(WindowId(3));
        });
        press_default(&engine, "Enter").await;
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 3), RIGHT_ZONE);
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

/// Two floating windows over the halves layout: 1 nearer the right zone, 2 nearer the left.
fn two_floating_windows() -> FakePlatform {
    let mut fake = FakePlatform::default();
    let window = |app: &str, pid: u32, x: f64| FakeWindow::new(app, pid, Rect::new(x, 300.0, 300.0, 200.0));
    fake.open(2, window("com.example.browser", 200, 100.0));
    fake.open(1, window("com.example.editor", 100, 800.0));
    fake
}

#[test]
fn snappy_space_places_the_selection_into_empty_zones_as_one_undo_step() {
    scenario(|| async {
        let name = "place-selection";
        let engine = live_engine(name, two_floating_windows());
        engine.force_debug_log();
        let before = (frame_of(&engine, 1), frame_of(&engine, 2));
        press_default(&engine, "Enter").await;
        focus(&engine, 2);
        press_default(&engine, "Enter").await;

        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (RIGHT_ZONE, LEFT_ZONE)
        );
        assert!(marks(&engine).is_empty(), "placed windows leave the selection");
        assert_eq!(status_text(&engine), "Snapped 2 windows.");
        let snaps = logged(name, "snap");
        assert_eq!(snaps.len(), 2, "each move is logged as a snap");
        assert!(snaps.iter().all(|snap| snap["outcome"] == "snapped"));

        undo(&engine);
        settle(&engine).await;
        assert_eq!((frame_of(&engine, 1), frame_of(&engine, 2)), before);

        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 2),
            LEFT_ZONE,
            "without a selection, Space moves only the focused window"
        );
        assert_eq!(frame_of(&engine, 1), before.0);
    });
}

#[test]
fn selected_windows_that_dont_fit_stay_selected_and_the_status_says_how_many() {
    scenario(|| async {
        let mut fake = two_floating_windows();
        let middle = Rect::new(620.0, 350.0, 200.0, 200.0);
        fake.open(3, FakeWindow::new("com.example.notes", 300, middle));
        let engine = live_engine("place-overflow", fake);
        for window in [1, 2, 3] {
            focus(&engine, window);
            press_default(&engine, "Enter").await;
        }

        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2), frame_of(&engine, 3)),
            (RIGHT_ZONE, LEFT_ZONE, middle)
        );
        assert_eq!(marked(&engine), [(middle, MarkStyle::Selected)]);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("1 window didn't fit.", false, false)
        );
    });
}
