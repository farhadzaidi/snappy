//! Moving the focused window into the nearest empty zone.

use snappy_core::keys::{KeyChord, SnappyKey};
use snappy_core::settings::NavigationShortcut;
use snappy_engine::{Engine, LogEvent};
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, command, event, floating_over_the_left_zone, frame_of, hotkey_id,
    inactive_window_in_the_right_zone, live_engine, logged, press_nearest_empty, scenario, settle,
    side_display, side_third, snappy, thirds_on_the_side, two_zoned_windows, undo, with_window, writes_to,
};

#[test]
fn snappy_space_snaps_a_floating_window_into_the_nearest_empty_zone_and_undoes() {
    scenario(|| async {
        let name = "nearest-empty-floating";
        let engine = live_engine(name, floating_over_the_left_zone(LEFT_ZONE));
        engine.force_debug_log();
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 1),
            RIGHT_ZONE,
            "the left zone is nearer but taken"
        );
        assert_eq!(frame_of(&engine, 2), LEFT_ZONE, "the occupant stays");
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("Snapped.", false, false)
        );
        let snaps = logged(name, LogEvent::Snap);
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0]["outcome"], "snapped");
        assert_eq!(snaps[0]["window"], 1);

        undo(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), FLOATING);
    });
}

#[test]
fn snappy_space_moves_a_window_already_in_a_zone_to_another_empty_zone() {
    scenario(|| async {
        let engine = live_engine("nearest-empty-zoned", with_window(LEFT_ZONE));
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);
    });
}

#[test]
fn snappy_space_with_every_zone_taken_moves_nothing_and_says_so_quietly() {
    scenario(|| async {
        let engine = live_engine("nearest-empty-full", two_zoned_windows());
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(writes_to(&engine, 2), 0);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error, status.notify),
            ("Every zone is taken.", false, false)
        );
    });
}

#[test]
fn snappy_space_on_a_full_display_goes_to_the_nearest_empty_zone_on_another() {
    scenario(|| async {
        let mut fake = two_zoned_windows();
        fake.displays.push(side_display());
        fake.open(3, FakeWindow::new("com.example.notes", 300, side_third(0)));
        fake.focused = Some(WindowId(1));
        let engine = live_engine("nearest-empty-other-display", fake);
        thirds_on_the_side(&engine);
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(
            frame_of(&engine, 1),
            side_third(1),
            "the side display's first third is taken, so the next one is nearest"
        );
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(frame_of(&engine, 3), side_third(0));
    });
}

#[test]
fn snappy_space_ignores_windows_of_inactive_workspaces() {
    scenario(|| async {
        let engine = inactive_window_in_the_right_zone("nearest-empty-inactive").await;
        press_nearest_empty(&engine);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);
    });
}

#[test]
fn the_nearest_empty_zone_shortcut_can_be_changed_or_cleared() {
    scenario(|| async {
        let engine = live_engine("nearest-empty-shortcut", with_window(LEFT_ZONE));
        let registered = |engine: &Engine<FakePlatform>, chord: &KeyChord| {
            engine.with_platform(|p| {
                p.registered
                    .iter()
                    .any(|(_, combo)| *combo == chord.combo(SnappyKey::DEFAULT))
            })
        };
        assert!(registered(&engine, &snappy("Space")));

        command(
            &engine,
            Command::SetNavigationShortcut {
                shortcut: NavigationShortcut::NearestEmptyZone,
                chord: Some(snappy("KeyE")),
            },
        );
        assert_eq!(engine.settings().navigation.nearest_empty, Some(snappy("KeyE")));
        assert!(!registered(&engine, &snappy("Space")));
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("KeyE"))),
        );
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), RIGHT_ZONE);

        command(
            &engine,
            Command::SetNavigationShortcut {
                shortcut: NavigationShortcut::NearestEmptyZone,
                chord: None,
            },
        );
        assert_eq!(engine.settings().navigation.nearest_empty, None);
        assert!(!registered(&engine, &snappy("KeyE")));
    });
}
