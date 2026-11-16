//! Workspaces: starting, switching, membership, and other desktops.

use std::collections::BTreeMap;
use std::time::Duration;

use snappy_core::geometry::Rect;
use snappy_core::keys::{KeyChord, Modifier};
use snappy_engine::{Engine, Input};
use snappy_fake::{FakePlatform, FakeWindow, OverlayCall};
use snappy_ipc::{Command, ToSettings};
use snappy_platform::{LiveControl, PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, active, app_state, click_away, command, default_workspaces,
    enable_workspaces, event, focus, frame_of, frozen, hotkey_id, is_parked, live_engine, on_parking_desktop,
    paths_kept, place, press_default, remembered_workspaces, saved_windows, scenario, send_to, settle,
    side_display, snappy, status_text, switch_to, two_zoned_windows, window_counts,
    window_one_on_the_parking_desktop, with_settings_window, with_window, writes_to,
};

#[test]
fn the_settings_window_parks_like_any_window() {
    scenario(|| async {
        let (fake, settings_frame) = with_settings_window();
        let engine = live_engine("settings-parks", fake);
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        enable_workspaces(&engine);

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(9)).x) >= 1439.0,
            "parked"
        );

        let switch_one = hotkey_id(&engine, &KeyChord::new("Digit1", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_one));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(9))), settings_frame);
    });
}

#[test]
fn opening_settings_switches_to_its_workspace_then_focuses_it() {
    scenario(|| async {
        let (fake, settings_frame) = with_settings_window();
        let engine = live_engine("settings-reveal", fake);
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        enable_workspaces(&engine);
        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        engine.take_outbox();

        event(&engine, PlatformEvent::OpenSettingsRequested);
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(9))), settings_frame);
        assert!(engine.take_outbox().contains(&ToSettings::Focus));
    });
}

#[test]
fn layouts_applied_while_workspaces_run_belong_to_the_active_workspace() {
    scenario(|| async {
        let engine = live_engine("workspace-layouts", FakePlatform::default());
        enable_workspaces(&engine);
        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "rows".into(),
            },
        );
        let settings = engine.settings();
        assert_eq!(settings.workspaces.items[0].displays["main"], "rows");
        assert_eq!(settings.workspaces.items[1].displays["main"], "halves");
        assert_eq!(settings.layouts.displays["main"], "halves");
    });
}

#[test]
fn a_switch_asked_for_while_displays_change_runs_once_they_settle() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("switch-during-display-change", with_window(start));
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::DisplaysChanged);
        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        assert_eq!(
            engine.settings().workspaces.active_id,
            "workspace-1",
            "held while settling"
        );
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "parked"
        );
    });
}

#[test]
fn switching_hides_and_restores_windows() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("switch", with_window(start));
        enable_workspaces(&engine);
        assert!(engine.settings().workspaces.enabled);

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "parked at the corner"
        );
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");

        let switch_one = hotkey_id(&engine, &KeyChord::new("Digit1", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_one));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), start);
    });
}

#[test]
fn quitting_returns_parked_windows() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("quit-returns", with_window(start));
        enable_workspaces(&engine);
        let send_two = hotkey_id(
            &engine,
            &KeyChord::new("Digit2", [Modifier::Ctrl, Modifier::Shift]),
        );
        event(&engine, PlatformEvent::HotKey(send_two));
        settle(&engine).await;
        assert!(engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0);
        engine.shutdown();
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(1))), start);
    });
}

#[test]
fn workspaces_start_off_with_three() {
    scenario(|| async {
        let engine = live_engine("workspace-defaults", FakePlatform::default());
        let workspaces = engine.settings().workspaces;
        assert!(!workspaces.enabled);
        assert_eq!(workspaces.items.len(), 3);
    });
}

#[test]
fn a_switch_refreshes_shortcuts_only_when_they_change_and_always_overlays() {
    scenario(|| async {
        let engine = live_engine("switch-refresh", FakePlatform::default());
        enable_workspaces(&engine);
        let registrations = || engine.with_platform(|p| p.registrations);
        let prepares = || {
            engine.with_platform(|p| {
                p.overlay
                    .iter()
                    .filter(|call| **call == OverlayCall::Prepare("main".into()))
                    .count()
            })
        };

        let (registered, prepared) = (registrations(), prepares());
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert_eq!(registrations(), registered, "same layouts, same shortcuts");
        assert!(prepares() > prepared);

        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "thirds".into(),
            },
        );
        let (registered, prepared) = (registrations(), prepares());
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(
            registrations(),
            registered + 1,
            "halves needs other zone shortcuts"
        );
        assert!(prepares() > prepared);
    });
}

#[test]
fn a_relaunched_apps_windows_join_the_active_workspace() {
    scenario(|| async {
        let start = Rect::new(300.0, 200.0, 600.0, 400.0);
        let engine = live_engine("relaunched-app", with_window(start));
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1));

        engine.with_platform(|p| {
            p.windows.remove(&WindowId(1));
            p.stacking.retain(|id| *id != WindowId(1));
        });
        event(&engine, PlatformEvent::AppTerminated { pid: 100 });
        engine.with_platform(|p| p.open(5, FakeWindow::new("com.example.editor", 101, start)));
        event(&engine, PlatformEvent::AppLaunched { pid: 101 });
        event(&engine, PlatformEvent::WindowCreated { pid: 101 });

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(
            is_parked(&engine, 5),
            "it belongs to workspace 1, not to the old window's workspace"
        );
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.frame_of(WindowId(5))), start);
    });
}

#[test]
fn workspaces_skip_full_screen_and_sheet_windows_and_never_move_minimized_ones() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.open(1, FakeWindow::new("com.example.editor", 100, FLOATING));
        let mut player = FakeWindow::new("com.example.player", 200, FakePlatform::main_display().frame);
        player.state.full_screen = true;
        fake.open(2, player);
        let mut writer = FakeWindow::new("com.example.writer", 300, Rect::new(200.0, 150.0, 500.0, 400.0));
        writer.state.has_sheet = true;
        fake.open(3, writer);
        fake.open(
            4,
            FakeWindow::new("com.example.mail", 400, Rect::new(250.0, 250.0, 500.0, 400.0)),
        );
        let engine = live_engine("workspace-eligibility", fake);
        enable_workspaces(&engine);
        assert_eq!(
            window_counts(&engine)["workspace-1"],
            2,
            "the editor and the mail window"
        );

        engine.with_platform(|p| p.windows.get_mut(&WindowId(4)).unwrap().state.minimized = true);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1));
        for window in [2, 3, 4] {
            assert_eq!(writes_to(&engine, window), 0, "window {window} is never moved");
        }
        assert_eq!(
            window_counts(&engine)["workspace-1"],
            2,
            "the minimized window stays"
        );

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), FLOATING);
        assert_eq!(writes_to(&engine, 4), 0);
    });
}

#[test]
fn switch_timings_are_written_at_quit_and_only_when_there_are_some() {
    scenario(|| async {
        let engine = live_engine("timings-none", with_window(FLOATING));
        enable_workspaces(&engine);
        engine.shutdown();
        let unused = paths_kept("timings-none").log("workspace-performance.json");
        assert!(!unused.exists());

        let engine = live_engine("timings", with_window(FLOATING));
        enable_workspaces(&engine);
        for number in [2, 3, 1] {
            event(&engine, PlatformEvent::HotKey(switch_to(&engine, number)));
            settle(&engine).await;
        }
        let file = paths_kept("timings").log("workspace-performance.json");
        assert!(!file.exists(), "timings stay in memory until quit");
        assert_eq!(engine.switch_timings().len(), 3);

        engine.shutdown();
        let timings: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(timings.len(), 3);
        let mut fields: Vec<&str> = timings[0]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "discoveryMs",
                "membershipMs",
                "queueMs",
                "refreshMs",
                "settingsMs",
                "succeeded",
                "totalMs",
                "windowMs",
                "windows",
            ]
        );
        assert!(
            timings
                .iter()
                .all(|timing| timing["succeeded"] == true && timing["windows"] == 1)
        );
    });
}

#[test]
fn the_first_start_copies_each_displays_layout_into_every_workspace() {
    scenario(|| async {
        let mut fake = FakePlatform::default();
        fake.displays.push(side_display());
        let engine = live_engine("workspace-layout-seed", fake);
        for (display, layout) in [("main", "rows"), ("projector", "thirds")] {
            command(
                &engine,
                Command::ApplyLayout {
                    display: display.into(),
                    layout: layout.into(),
                },
            );
        }
        assert_eq!(engine.settings().layouts.default_layout_id, "thirds");

        enable_workspaces(&engine);
        let seeded: BTreeMap<String, String> =
            [("main", "rows"), ("projector", "thirds"), ("side", "thirds")]
                .into_iter()
                .map(|(display, layout)| (display.to_owned(), layout.to_owned()))
                .collect();
        for item in &engine.settings().workspaces.items {
            assert_eq!(item.displays, seeded, "{}", item.id);
        }

        command(
            &engine,
            Command::ApplyLayout {
                display: "main".into(),
                layout: "six".into(),
            },
        );
        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        enable_workspaces(&engine);
        let items = engine.settings().workspaces.items;
        assert_eq!(
            items[0].displays["main"], "six",
            "later starts keep each workspace's layouts"
        );
        assert_eq!(items[1].displays["main"], "rows");
    });
}

#[test]
fn turning_workspaces_on_enrolls_visible_windows_and_remembered_minimized_ones() {
    scenario(|| async {
        let engine = live_engine("enroll", two_zoned_windows());
        enable_workspaces(&engine);
        assert_eq!(window_counts(&engine)["workspace-1"], 2);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);

        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(2)).unwrap().state.minimized = true;
            let mut forgotten = FakeWindow::new("com.example.notes", 300, FLOATING);
            forgotten.state.minimized = true;
            p.open(3, forgotten);
            p.focused = Some(WindowId(1));
        });
        let written = writes_to(&engine, 2);
        enable_workspaces(&engine);
        let counts = window_counts(&engine);
        assert_eq!(
            (
                counts["workspace-1"],
                counts["workspace-2"],
                counts["workspace-3"]
            ),
            (1, 1, 0),
            "the forgotten minimized window stays out"
        );
        assert_eq!(writes_to(&engine, 2), written, "minimized windows never move");
    });
}

#[test]
fn windows_join_the_active_workspace_from_events_never_by_polling() {
    scenario(|| async {
        let engine = live_engine("join-by-events", with_window(FLOATING));
        enable_workspaces(&engine);
        engine.with_platform(|p| {
            p.open(
                5,
                FakeWindow::new("com.example.notes", 500, Rect::new(100.0, 100.0, 400.0, 300.0)),
            )
        });
        tokio::time::sleep(Duration::from_secs(30)).await;
        assert_eq!(window_counts(&engine)["workspace-1"], 1, "nothing polls");

        event(&engine, PlatformEvent::WindowCreated { pid: 500 });
        assert_eq!(window_counts(&engine)["workspace-1"], 2);

        engine.with_platform(|p| {
            p.open(
                6,
                FakeWindow::new("com.example.music", 600, Rect::new(150.0, 150.0, 400.0, 300.0)),
            )
        });
        event(
            &engine,
            PlatformEvent::FocusChanged {
                pid: 600,
                by_activation: true,
            },
        );
        assert_eq!(window_counts(&engine)["workspace-1"], 3);

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!([1, 5, 6].iter().all(|window| is_parked(&engine, *window)));
    });
}

#[test]
fn a_switch_saves_only_the_active_workspace_and_reverts_when_that_fails() {
    scenario(|| async {
        let name = "switch-save";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;

        let file = paths_kept(name).settings;
        let saved = || serde_json::from_slice::<serde_json::Value>(&std::fs::read(&file).unwrap()).unwrap();
        let mut expected = saved();
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        expected["workspaces"]["activeId"] = "workspace-2".into();
        assert_eq!(saved(), expected);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);

        // Selected, window 2 would go along to workspace 3.
        focus(&engine, 2);
        press_default(&engine, "Enter").await;
        // A folder where the settings file goes makes any save of it fail.
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 3)));
        settle(&engine).await;
        std::fs::remove_dir(&file).unwrap();
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE, "switched back");
        assert!(is_parked(&engine, 1));
        assert_eq!(
            remembered_workspaces(name)[&2],
            "workspace-2",
            "saved as it is after switching back"
        );
        let status = engine.status().unwrap();
        assert!(
            status.is_error
                && status
                    .message
                    .starts_with("Couldn't save the workspace selection")
        );
    });
}

#[test]
fn while_a_switch_is_pending_next_counts_from_the_requested_workspace() {
    scenario(|| async {
        let engine = live_engine("pending-switch", with_window(FLOATING));
        enable_workspaces(&engine);
        let next = hotkey_id(&engine, &snappy("Quote"));
        event(&engine, PlatformEvent::HotKey(next));
        event(&engine, PlatformEvent::HotKey(next));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-3");
        assert!(is_parked(&engine, 1));
    });
}

#[test]
fn a_switch_a_click_cancels_before_it_runs_can_be_asked_for_again() {
    scenario(|| async {
        let engine = live_engine("cancelled-switch", with_window(FLOATING));
        enable_workspaces(&engine);
        // A snap the app is slow to follow, a switch queued behind it, and a click.
        let switch_behind_a_slow_snap_then_click = async |number: u8| {
            place(&engine, 1, FLOATING);
            engine.with_platform(|p| p.focused = Some(WindowId(1)));
            frozen(&engine, 1, true);
            event(
                &engine,
                PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit1"))),
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
            event(&engine, PlatformEvent::HotKey(switch_to(&engine, number)));
            click_away(&engine);
            frozen(&engine, 1, false);
            settle(&engine).await;
        };

        switch_behind_a_slow_snap_then_click(2).await;
        assert_eq!(active(&engine), "workspace-1", "the click cancelled the switch");
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(
            active(&engine),
            "workspace-2",
            "the same switch runs when asked again"
        );

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        switch_behind_a_slow_snap_then_click(3).await;
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Quote"))),
        );
        settle(&engine).await;
        assert_eq!(
            active(&engine),
            "workspace-2",
            "next counts from the active workspace"
        );
    });
}

#[test]
fn switching_and_sending_never_list_every_window() {
    scenario(|| async {
        let engine = live_engine("switch-without-listing", two_zoned_windows());
        enable_workspaces(&engine);
        engine.with_platform(|p| {
            p.window_listings = 0;
            p.focused = Some(WindowId(2));
        });
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(engine.with_platform(|p| p.window_listings), 0);
    });
}

#[test]
fn switches_to_the_active_workspace_and_while_paused_do_nothing() {
    scenario(|| async {
        let engine = live_engine("switch-noop", with_window(FLOATING));
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(writes_to(&engine, 1), 0);
        assert!(engine.switch_timings().is_empty());

        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(writes_to(&engine, 1), 0);
        assert_eq!(window_counts(&engine)["workspace-1"], 1);
    });
}

#[test]
fn the_snappy_key_and_a_digit_switch_to_that_workspace() {
    scenario(|| async {
        let engine = default_workspaces("switch-by-number", two_zoned_windows());
        press_default(&engine, "Digit3").await;
        assert_eq!(active(&engine), "workspace-3");
        assert!(is_parked(&engine, 1) && is_parked(&engine, 2));

        press_default(&engine, "Digit1").await;
        assert_eq!(active(&engine), "workspace-1");
        assert_eq!(
            (frame_of(&engine, 1), frame_of(&engine, 2)),
            (LEFT_ZONE, RIGHT_ZONE)
        );
    });
}

#[test]
fn another_desktop_pauses_workspaces_without_changing_them() {
    scenario(|| async {
        let engine = live_engine("other-desktop", two_zoned_windows());
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        let written = writes_to(&engine, 2);

        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        let state = app_state(&engine);
        assert!(!state.workspaces.running);
        assert_eq!(
            state.workspaces.pause_message.as_deref(),
            Some("Paused on another macOS desktop. Return to the original desktop to resume.")
        );
        assert!(state.settings.workspaces.enabled);
        assert_eq!(state.workspaces.window_counts["workspace-2"], 1);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-1");
        assert_eq!(writes_to(&engine, 2), written);
        assert!(is_parked(&engine, 2));

        engine.with_platform(|p| p.on_managed_desktop = true);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        let state = app_state(&engine);
        assert!(state.workspaces.running);
        assert_eq!(state.workspaces.pause_message, None);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
    });
}

#[test]
fn membership_is_keyed_by_process_and_window_and_dropped_when_the_app_quits() {
    scenario(|| async {
        let name = "membership";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;

        let file = paths_kept(name).membership();
        let entries = || saved_windows(&file);
        let pids = || -> Vec<u64> {
            entries()
                .iter()
                .map(|entry| entry["key"]["pid"].as_u64().unwrap())
                .collect()
        };
        let keys = |value: &serde_json::Value| -> Vec<String> {
            let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
            keys.sort();
            keys
        };
        assert_eq!(entries().len(), 2);
        for entry in entries() {
            assert_eq!(keys(&entry), ["frame", "key", "parkedOrigin", "workspaceId"]);
            assert_eq!(keys(&entry["key"]), ["launchTime", "pid", "window"]);
        }

        engine.with_platform(|p| p.windows.get_mut(&WindowId(1)).unwrap().state.minimized = true);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(pids().contains(&100), "a minimized window keeps its entry");

        engine.with_platform(|p| {
            p.windows.remove(&WindowId(2));
            p.stacking.retain(|id| *id != WindowId(2));
        });
        event(&engine, PlatformEvent::AppTerminated { pid: 200 });
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(pids(), [100]);
    });
}

#[test]
fn the_menu_bar_tooltip_names_the_active_workspace_while_workspaces_run() {
    scenario(|| async {
        let engine = live_engine("tooltip", with_window(FLOATING));
        let tooltip = || engine.with_platform(|p| p.tray.tooltip.clone());
        assert_eq!(tooltip(), "Snappy — window zones");

        enable_workspaces(&engine);
        assert_eq!(tooltip(), "Snappy · Workspace 1");
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(tooltip(), "Snappy · Workspace 2");

        engine.with_platform(|p| p.on_managed_desktop = false);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        assert_eq!(tooltip(), "Snappy — window zones", "paused");
        engine.with_platform(|p| p.on_managed_desktop = true);
        event(&engine, PlatformEvent::NativeDesktopChanged);
        assert_eq!(tooltip(), "Snappy · Workspace 2");

        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        assert_eq!(tooltip(), "Snappy — window zones");
    });
}

/// The platform loses control, and while it is gone shows window 1 again, as GNOME's
/// extension does when it is turned off.
async fn control_lost_with_window_one_shown(name: &str) -> Engine<FakePlatform> {
    let engine = window_one_on_the_parking_desktop(name).await;
    engine.with_platform(|p| p.live_control = LiveControl::unavailable("The extension is off."));
    event(&engine, PlatformEvent::LiveControlChanged);
    settle(&engine).await;
    engine.with_platform(|p| {
        p.windows.get_mut(&WindowId(1)).unwrap().state.on_parking_desktop = false;
    });
    engine
}

#[test]
fn losing_live_control_keeps_workspaces_as_they_were_and_moves_nothing() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("control-lost-keeps").await;
        let writes = engine.with_platform(|p| p.writes.len());
        engine.with_platform(|p| p.live_control = LiveControl::unavailable("The extension is off."));
        event(&engine, PlatformEvent::LiveControlChanged);
        settle(&engine).await;
        assert_eq!(engine.with_platform(|p| p.writes.len()), writes, "nothing moved");
        assert_eq!(status_text(&engine), "The extension is off.");
        assert!(!app_state(&engine).workspaces.running);
        assert_eq!(window_counts(&engine)["workspace-2"], 1, "membership is kept");
    });
}

#[test]
fn regaining_live_control_hides_again_what_the_platform_showed_meanwhile() {
    scenario(|| async {
        let engine = control_lost_with_window_one_shown("control-regained").await;
        engine.with_platform(|p| p.live_control = LiveControl::Available);
        event(&engine, PlatformEvent::LiveControlChanged);
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1), "window 1 belongs to workspace 2");
        assert!(!on_parking_desktop(&engine, 2));
        assert!(app_state(&engine).workspaces.running);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
    });
}

#[test]
fn the_platform_starting_over_hides_shown_windows_again() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("platform-restarted").await;
        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(1)).unwrap().state.on_parking_desktop = false;
        });
        event(&engine, PlatformEvent::LiveControlChanged);
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1));
        assert!(app_state(&engine).workspaces.running);
    });
}
