//! Sending the focused window to another workspace.

use snappy_core::geometry::Rect;
use snappy_core::keys::{KeyChord, Modifier};
use snappy_engine::Input;
use snappy_fake::FakeWindow;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    FLOATING, LEFT_ZONE, active, default_workspaces, enable_workspaces, event, focus, focused, four_windows,
    frame_of, hotkey_id, is_parked, live_engine, place, press_default, scenario, send_to, settle,
    two_zoned_windows, window_counts, with_settings_window, with_window, writes_to,
};

#[test]
fn send_and_switch_sends_the_window_then_follows_it() {
    scenario(|| async {
        let engine = live_engine("send-and-switch", two_zoned_windows());
        enable_workspaces(&engine);
        let chord = KeyChord::new("Digit2", [Modifier::Ctrl, Modifier::Alt]);
        event(&engine, PlatformEvent::HotKey(hotkey_id(&engine, &chord)));
        settle(&engine).await;
        assert_eq!(engine.settings().workspaces.active_id, "workspace-2");
        assert_eq!(frame_of(&engine, 1), LEFT_ZONE, "the sent window shows");
        assert!(is_parked(&engine, 2));
        assert_eq!(window_counts(&engine)["workspace-2"], 1);
    });
}

#[test]
fn sending_to_the_next_workspace_three_times_sends_three_windows_and_stays() {
    scenario(|| async {
        let engine = default_workspaces("send-and-stay", four_windows());
        for window in 1..=4 {
            focus(&engine, window);
        }

        for expected_focus in [3, 2, 1] {
            press_default(&engine, "Slash").await;
            assert_eq!(active(&engine), "workspace-1", "the send stays");
            assert_eq!(focused(&engine), Some(WindowId(expected_focus)));
        }
        assert!([2, 3, 4].into_iter().all(|window| is_parked(&engine, window)));
        assert!(!is_parked(&engine, 1));
        assert_eq!(window_counts(&engine)["workspace-2"], 3);
        assert_eq!(window_counts(&engine)["workspace-1"], 1);

        press_default(&engine, "Digit2").await;
        assert!([2, 3, 4].into_iter().all(|window| !is_parked(&engine, window)));
    });
}

#[test]
fn sending_to_the_previous_workspace_wraps_to_the_last_and_stays() {
    scenario(|| async {
        let engine = default_workspaces("send-previous", two_zoned_windows());
        press_default(&engine, "Period").await;
        assert_eq!(active(&engine), "workspace-1");
        assert!(is_parked(&engine, 1));
        assert_eq!(window_counts(&engine)["workspace-3"], 1);
        assert_eq!(focused(&engine), Some(WindowId(2)));
    });
}

#[test]
fn sending_the_last_window_away_leaves_focus_alone() {
    scenario(|| async {
        let engine = default_workspaces("send-last", with_window(Rect::new(300.0, 200.0, 600.0, 400.0)));
        press_default(&engine, "Slash").await;
        assert!(is_parked(&engine, 1));
        assert_eq!(active(&engine), "workspace-1");
        assert_eq!(focused(&engine), Some(WindowId(1)), "nothing else was focused");
    });
}

#[test]
fn send_moves_the_focused_window_only_from_the_active_workspace() {
    scenario(|| async {
        let (mut fake, settings_frame) = with_settings_window();
        fake.open(1, FakeWindow::new("com.example.editor", 100, FLOATING));
        fake.focused = Some(WindowId(9));
        let engine = live_engine("send-focused", fake);
        engine.handle(Input::SettingsAppStarted { pid: 4242 });
        enable_workspaces(&engine);

        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert!(
            is_parked(&engine, 9),
            "the settings window is sent like any other"
        );
        assert!(!is_parked(&engine, 1));
        assert_eq!(
            engine.with_platform(|p| p.focused),
            Some(WindowId(1)),
            "focus stays on this workspace"
        );

        place(&engine, 9, settings_frame);
        engine.with_platform(|p| p.focused = Some(WindowId(9)));
        let written = writes_to(&engine, 9);
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 3)));
        settle(&engine).await;
        let status = engine.status().unwrap();
        assert_eq!(
            (status.message.as_str(), status.is_error),
            ("Focus a window on the current workspace first.", true)
        );
        assert_eq!(writes_to(&engine, 9), written);
    });
}
