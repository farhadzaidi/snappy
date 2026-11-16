//! What a restart, a crash, or a relaunch keeps of workspaces.

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{KeyChord, Modifier};
use snappy_engine::Engine;
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, TABBED, active, all_writes, app_state, command, default_workspaces,
    enable_workspaces, event, frame_of, frozen, hotkey_id, is_parked, live_engine, logged, marks, paths,
    paths_kept, pick_tab, press_default, relaunched, remembered_workspaces, saved_windows, scenario, send_to,
    settle, stopped_for_restart, switch_to, two_zoned_windows, visited_tabs, window_counts, with_window,
    writes_to,
};

#[test]
fn a_parked_window_nobody_remembers_comes_back_on_screen() {
    scenario(|| async {
        let stranded = Rect::new(1439.0, 500.0, 600.0, 400.0);
        let engine = live_engine("stranded-window", with_window(stranded));
        enable_workspaces(&engine);
        let frame = engine.with_platform(|p| p.frame_of(WindowId(1)));
        assert_eq!(
            frame,
            Rect::new(840.0, 500.0, 600.0, 400.0),
            "just inside the right edge"
        );

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "it joined the workspace and parks with it"
        );
    });
}

#[test]
fn a_visible_window_joins_the_active_workspace_whatever_was_remembered() {
    scenario(|| async {
        let name = "visible-joins-active";
        let visible = Rect::new(300.0, 200.0, 600.0, 400.0);
        let paths = paths(name);
        std::fs::create_dir_all(&paths.state_dir).unwrap();
        let saved = serde_json::json!([{
            "key": { "pid": 100, "launchTime": 1.0, "window": "1" },
            "workspaceId": "workspace-2",
            "frame": visible,
            "parkedOrigin": null,
        }]);
        std::fs::write(
            paths.state_dir.join("workspace-membership.json"),
            saved.to_string(),
        )
        .unwrap();

        let engine = Engine::new(with_window(visible), paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        enable_workspaces(&engine);
        settle(&engine).await;
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            visible,
            "never hidden at start"
        );

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert!(
            engine.with_platform(|p| p.frame_of(WindowId(1)).x) >= 1439.0,
            "it belongs to the workspace that was active"
        );
    });
}

#[test]
fn a_window_parked_for_another_workspace_stays_parked_with_its_home() {
    scenario(|| async {
        let name = "parked-keeps-home";
        let home = Rect::new(300.0, 200.0, 600.0, 400.0);
        let parked = Rect::new(1439.0, 500.0, 600.0, 400.0);
        let paths = paths(name);
        std::fs::create_dir_all(&paths.state_dir).unwrap();
        let saved = serde_json::json!([{
            "key": { "pid": 100, "launchTime": 1.0, "window": "1" },
            "workspaceId": "workspace-2",
            "frame": home,
            "parkedOrigin": parked.origin(),
        }]);
        std::fs::write(
            paths.state_dir.join("workspace-membership.json"),
            saved.to_string(),
        )
        .unwrap();

        let engine = Engine::new(with_window(parked), paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        enable_workspaces(&engine);
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            parked,
            "still parked"
        );

        let switch_two = hotkey_id(&engine, &KeyChord::new("Digit2", [Modifier::Ctrl]));
        event(&engine, PlatformEvent::HotKey(switch_two));
        settle(&engine).await;
        assert_eq!(
            engine.with_platform(|p| p.frame_of(WindowId(1))),
            home,
            "back home"
        );
    });
}

/// Window 1 sent to workspace 2, then the daemon stops without quitting. Returns the
/// platform as the next launch finds it.
async fn crashed_with_window_one_parked(name: &str) -> FakePlatform {
    let engine = live_engine(name, two_zoned_windows());
    enable_workspaces(&engine);
    event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
    settle(&engine).await;
    assert!(is_parked(&engine, 1));
    let mut platform = engine.with_platform(|p| p.clone());
    platform.writes.clear();
    platform
}

/// Like `relaunched`, with the debug log on from the start.
fn relaunched_logging(name: &str, platform: FakePlatform) -> Engine<FakePlatform> {
    let engine = Engine::new(platform, paths_kept(name));
    engine.force_debug_log();
    engine.start();
    engine
}

#[test]
fn a_restart_keeps_every_window_in_its_workspace_and_moves_nothing() {
    scenario(|| async {
        let name = "restart-keeps";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 3)));
        settle(&engine).await;
        assert!(is_parked(&engine, 1) && is_parked(&engine, 2));
        let parked = (frame_of(&engine, 1), frame_of(&engine, 2));
        let platform = stopped_for_restart(&engine);
        assert!(paths_kept(name).journal().exists(), "the journal stays");

        let engine = relaunched_logging(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert_eq!(all_writes(&engine), [], "the launch moves nothing");
        assert_eq!((frame_of(&engine, 1), frame_of(&engine, 2)), parked);
        assert_eq!(active(&engine), "workspace-3");
        let counts = window_counts(&engine);
        assert_eq!(
            (
                counts["workspace-1"],
                counts["workspace-2"],
                counts["workspace-3"]
            ),
            (1, 1, 0)
        );
        let started = logged(name, "workspacesStarted");
        let started = started.last().unwrap();
        assert_eq!(
            started["rejoined"],
            serde_json::json!({ "workspace-1": 1, "workspace-2": 1 })
        );
        assert_eq!(
            (&started["staleJournal"], &started["returned"]),
            (&serde_json::json!(0), &serde_json::json!(0))
        );

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert!(is_parked(&engine, 1));
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert!(is_parked(&engine, 2));
    });
}

#[test]
fn quitting_after_a_restart_still_returns_parked_windows() {
    scenario(|| async {
        let name = "restart-then-quit";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert!(is_parked(&engine, 1));
        event(&engine, PlatformEvent::QuitRequested);
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn a_crash_leaves_parked_windows_parked_for_the_next_launch_and_user_moved_ones_alone() {
    scenario(|| async {
        let name = "recovery-identity";
        let mut platform = crashed_with_window_one_parked(name).await;
        std::fs::remove_file(paths_kept(name).membership()).unwrap();
        let parked = platform.windows[&WindowId(1)].frame;
        let moved = RIGHT_ZONE.with_origin(Point::new(500.0, 300.0));
        platform.windows.get_mut(&WindowId(2)).unwrap().frame = moved;

        let engine = relaunched(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert_eq!(frame_of(&engine, 1), parked, "rejoined from the journal alone");
        assert_eq!(frame_of(&engine, 2), moved);
        assert_eq!(all_writes(&engine), []);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        assert!(paths_kept(name).journal().exists(), "window 1 is still parked");

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "back at its saved frame");
    });
}

#[test]
fn a_restart_finds_a_parked_window_whose_token_changed_by_its_parked_position() {
    scenario(|| async {
        let name = "recovery-position";
        let mut platform = crashed_with_window_one_parked(name).await;
        let parked = platform.windows.remove(&WindowId(1)).unwrap();
        let parked_frame = parked.frame;
        platform.stacking.retain(|id| *id != WindowId(1));
        platform.open(7, parked);
        platform.open(8, FakeWindow::new("com.example.editor", 100, FLOATING));

        let engine = relaunched(name, platform);
        assert_eq!(frame_of(&engine, 7), parked_frame);
        assert_eq!(frame_of(&engine, 8), FLOATING);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 7), LEFT_ZONE);
        assert!(
            is_parked(&engine, 8),
            "a window it didn't know joined workspace 1"
        );
    });
}

#[test]
fn a_window_parked_for_the_workspace_showing_at_launch_comes_back() {
    scenario(|| async {
        let name = "recovery-active";
        let platform = crashed_with_window_one_parked(name).await;
        let file = paths_kept(name).settings;
        let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        settings["workspaces"]["activeId"] = "workspace-2".into();
        std::fs::write(&file, settings.to_string()).unwrap();

        let engine = relaunched(name, platform);
        assert_eq!(active(&engine), "workspace-2");
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(
            frame_of(&engine, 2),
            RIGHT_ZONE,
            "left alone, in the workspace showing"
        );
        assert_eq!(window_counts(&engine)["workspace-2"], 2);
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn a_start_time_read_back_a_little_off_is_still_the_same_window() {
    scenario(|| async {
        let name = "restart-rounded-start";
        let engine = live_engine(name, two_zoned_windows());
        enable_workspaces(&engine);
        let mut platform = stopped_for_restart(&engine);
        for window in platform.windows.values_mut() {
            window.launch_time += 0.0004;
        }

        let engine = relaunched(name, platform);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        let saved = saved_windows(&paths_kept(name).membership());
        assert_eq!(saved.len(), 2, "each window is saved once");

        event(&engine, PlatformEvent::WindowDestroyed(WindowId(1)));
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(
            remembered_workspaces(name).keys().copied().collect::<Vec<_>>(),
            [2]
        );
    });
}

#[test]
fn a_process_id_reused_by_another_launch_never_takes_over_a_parked_window() {
    scenario(|| async {
        let name = "recovery-reused-pid";
        let mut platform = crashed_with_window_one_parked(name).await;
        // The editor quit, and a new launch got the same process ID and a window at the
        // spot where the old one was parked.
        let window = platform.windows.get_mut(&WindowId(1)).unwrap();
        window.launch_time = 2.0;
        let stranded = window.frame;

        let engine = relaunched_logging(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert!(
            !is_parked(&engine, 1),
            "brought on screen like any stranded window"
        );
        assert_eq!(frame_of(&engine, 1).size(), stranded.size());
        assert_eq!(window_counts(&engine)["workspace-2"], 0);
        assert!(!paths_kept(name).journal().exists());
        let started = logged(name, "workspacesStarted");
        let started = started.last().unwrap();
        assert_eq!(started["staleJournal"], 1);
        assert_eq!(started["staleMembership"], 1);
        assert_eq!(started["rejoined"], serde_json::json!({}));
    });
}

#[test]
fn a_restart_keeps_a_parked_tab_group_together() {
    scenario(|| async {
        let name = "restart-tabs";
        let engine = visited_tabs(name, TABBED);
        press_default(&engine, "Slash").await;
        assert!(is_parked(&engine, 3));
        let parked = frame_of(&engine, 3);
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert_eq!(all_writes(&engine), []);
        assert_eq!(frame_of(&engine, 3), parked);
        assert_eq!(window_counts(&engine)["workspace-2"], 1, "the group counts once");
        assert_eq!(window_counts(&engine)["workspace-1"], 1);

        press_default(&engine, "Digit2").await;
        assert_eq!(frame_of(&engine, 3), TABBED);
        pick_tab(&engine, 1);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        press_default(&engine, "Digit1").await;
        assert!(is_parked(&engine, 1), "the tab showing now parks with its group");
        assert_eq!(remembered_workspaces(name)[&1], "workspace-2");
    });
}

#[test]
fn the_selection_is_not_kept_across_a_restart() {
    scenario(|| async {
        let name = "restart-selection";
        let engine = default_workspaces(name, two_zoned_windows());
        press_default(&engine, "Enter").await;
        assert_eq!(marks(&engine), [LEFT_ZONE]);
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert_eq!(marks(&engine), []);
        press_default(&engine, "Quote").await;
        assert!(
            is_parked(&engine, 1) && is_parked(&engine, 2),
            "nothing selected goes along"
        );
    });
}

#[test]
fn an_unreadable_journal_is_set_aside_and_workspaces_start_from_membership() {
    scenario(|| async {
        let name = "recovery-unreadable";
        let platform = crashed_with_window_one_parked(name).await;
        let journal = paths_kept(name).journal();
        std::fs::write(&journal, b"{ broken").unwrap();

        let engine = relaunched_logging(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert!(
            journal
                .with_file_name("workspace-journal.unreadable.json")
                .exists()
        );
        let unreadable = logged(name, "stateFileUnreadable");
        assert_eq!(unreadable.last().unwrap()["file"], "workspace-journal.json");
        assert!(is_parked(&engine, 1), "membership still knows its workspace");
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
        assert_eq!(saved_windows(&journal).len(), 1, "journaled again");
    });
}

#[test]
fn a_parked_window_that_closed_while_its_app_ran_on_is_forgotten_at_launch() {
    scenario(|| async {
        let name = "recovery-closed";
        let mut platform = crashed_with_window_one_parked(name).await;
        platform.windows.remove(&WindowId(1));
        platform.stacking.retain(|id| *id != WindowId(1));
        platform.open(9, FakeWindow::new("com.example.editor", 100, FLOATING));

        let engine = relaunched_logging(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert!(!engine.status().is_some_and(|status| status.is_error));
        assert!(!paths_kept(name).journal().exists());
        assert_eq!(writes_to(&engine, 9), 0);
        assert_eq!(window_counts(&engine)["workspace-1"], 2);
        let started = logged(name, "workspacesStarted");
        assert_eq!(started.last().unwrap()["closed"], 1);
    });
}

#[test]
fn a_window_that_cant_come_back_keeps_workspaces_off_and_its_entry_until_it_can() {
    scenario(|| async {
        let name = "recovery-pending";
        let mut platform = crashed_with_window_one_parked(name).await;
        platform.windows.get_mut(&WindowId(1)).unwrap().frozen = true;
        let file = paths_kept(name).settings;
        let mut settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        settings["workspaces"]["activeId"] = "workspace-2".into();
        std::fs::write(&file, settings.to_string()).unwrap();

        let engine = relaunched(name, platform);
        assert!(!app_state(&engine).workspaces.running);
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            (
                "Some hidden windows couldn't be brought back. Check Accessibility, then quit and reopen Snappy to retry.",
                true
            )
        );
        assert!(paths_kept(name).journal().exists(), "kept for the next attempt");

        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            (
                "A hidden window couldn't be brought back yet. Turn workspaces off again to retry.",
                true
            )
        );
        assert!(engine.settings().workspaces.enabled);

        frozen(&engine, 1, false);
        command(&engine, Command::SetWorkspacesEnabled { enabled: false });
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert!(!engine.settings().workspaces.enabled);
        assert!(!paths_kept(name).journal().exists());
    });
}
