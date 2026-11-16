//! Workspaces on platforms that park windows on a desktop of their own.

use snappy_core::geometry::Point;
use snappy_engine::{Engine, file_names};
use snappy_ipc::Command;
use snappy_platform::{PlatformEvent, WindowId, WindowSystem};

use crate::support::{
    FLOATING, LEFT_ZONE, RIGHT_ZONE, all_writes, app_state, command, enable_workspaces, event, frame_of,
    live_engine, on_parking_desktop, parking_on_a_desktop, paths, paths_kept, relaunched, scenario, settle,
    stopped_for_restart, switch_to, window_counts, window_one_on_the_parking_desktop, with_window, writes_to,
};

#[test]
fn desktop_parking_switches_workspaces_without_moving_windows() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("desktop-switch").await;
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(writes_to(&engine, 1), 0);

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert!(on_parking_desktop(&engine, 2));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(engine.with_platform(|p| p.focused), Some(WindowId(1)));
        assert_eq!(
            engine.with_platform(|p| p.visible_windows(None).len()),
            1,
            "hidden windows leave the visible list"
        );
    });
}

#[test]
fn desktop_parking_hides_again_a_window_its_app_brings_back() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("desktop-contain").await;
        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(1)).unwrap().state.on_parking_desktop = false;
        });
        event(&engine, PlatformEvent::WindowMoved(WindowId(1)));
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn desktop_parking_quitting_brings_every_window_back() {
    scenario(|| async {
        let engine = window_one_on_the_parking_desktop("desktop-quit").await;
        engine.shutdown();
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn desktop_parking_a_restart_leaves_windows_on_the_parking_desktop() {
    scenario(|| async {
        let name = "desktop-restart";
        let engine = window_one_on_the_parking_desktop(name).await;
        let platform = stopped_for_restart(&engine);

        let engine = relaunched(name, platform);
        assert!(app_state(&engine).workspaces.running);
        assert!(on_parking_desktop(&engine, 1));
        assert_eq!(all_writes(&engine), []);
        assert_eq!(window_counts(&engine)["workspace-2"], 1);

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert!(on_parking_desktop(&engine, 2));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
    });
}

#[test]
fn desktop_parking_quitting_brings_windows_back_from_the_parking_desktop_at_their_saved_frame() {
    scenario(|| async {
        let name = "desktop-recovery";
        let engine = window_one_on_the_parking_desktop(name).await;
        let mut platform = engine.with_platform(|p| p.clone());
        std::fs::remove_file(paths_kept(name).membership()).unwrap();
        let moved = LEFT_ZONE.with_origin(Point::new(500.0, 300.0));
        platform.windows.get_mut(&WindowId(1)).unwrap().frame = moved;
        platform.writes.clear();

        let engine = relaunched(name, platform);
        assert!(on_parking_desktop(&engine, 1), "a crash leaves it parked");
        engine.shutdown();
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "at its saved frame");
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn desktop_parking_recovery_leaves_windows_the_user_brought_back() {
    scenario(|| async {
        let name = "desktop-recovery-user";
        let engine = window_one_on_the_parking_desktop(name).await;
        let mut platform = engine.with_platform(|p| p.clone());
        std::fs::remove_file(paths_kept(name).membership()).unwrap();
        let window = platform.windows.get_mut(&WindowId(1)).unwrap();
        window.state.on_parking_desktop = false;
        platform.writes.clear();

        let engine = relaunched(name, platform);
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE);
        assert_eq!(writes_to(&engine, 1), 0, "its frame alone doesn't make it parked");
        assert!(!paths_kept(name).journal().exists());
    });
}

#[test]
fn desktop_parking_startup_keeps_a_window_parked_for_its_own_workspace() {
    scenario(|| async {
        let name = "desktop-startup-owner";
        let paths = paths(name);
        std::fs::create_dir_all(&paths.state_dir).unwrap();
        let saved = serde_json::json!([{
            "key": { "pid": 100, "launchTime": 1.0, "window": "1" },
            "workspaceId": "workspace-2",
            "frame": FLOATING,
            "parkedOrigin": FLOATING.origin(),
        }]);
        std::fs::write(paths.state_dir.join(file_names::MEMBERSHIP), saved.to_string()).unwrap();
        let mut fake = parking_on_a_desktop(with_window(FLOATING));
        fake.windows
            .get_mut(&WindowId(1))
            .unwrap()
            .state
            .on_parking_desktop = true;

        let engine = Engine::new(fake, paths);
        engine.start();
        command(&engine, Command::SetSnapping { enabled: true });
        enable_workspaces(&engine);
        settle(&engine).await;
        assert!(on_parking_desktop(&engine, 1), "still parked");
        assert_eq!(window_counts(&engine).get("workspace-2"), Some(&1));

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), FLOATING);
    });
}

#[test]
fn desktop_parking_startup_brings_back_a_window_nobody_remembers() {
    scenario(|| async {
        let mut fake = parking_on_a_desktop(with_window(FLOATING));
        fake.windows
            .get_mut(&WindowId(1))
            .unwrap()
            .state
            .on_parking_desktop = true;
        let engine = live_engine("desktop-startup-stranded", fake);
        enable_workspaces(&engine);
        settle(&engine).await;
        assert!(!on_parking_desktop(&engine, 1));
        assert_eq!(frame_of(&engine, 1), FLOATING);
        assert_eq!(window_counts(&engine).get("workspace-1"), Some(&1));
    });
}
