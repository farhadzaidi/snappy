//! Keeping parked windows hidden when apps or the system show them again, including while
//! displays change, as when a Mac wakes and its external display comes back.

use std::time::Duration;

use snappy_core::geometry::Point;
use snappy_core::workspace::{ParkingPolicy, parking_origin};
use snappy_engine::{Engine, LogEvent};
use snappy_fake::FakePlatform;
use snappy_platform::{PlatformEvent, WindowId};

use crate::support::{
    RIGHT_ZONE, active, enable_workspaces, event, frame_of, frozen, is_parked, live_engine, logged, scenario,
    send_to, settle, side_display, switch_to, two_zoned_windows,
};

/// Workspaces on, with window 2 sent to workspace 2 and window 1 focused, and the debug log
/// on.
async fn window_two_parked(name: &str) -> Engine<FakePlatform> {
    let engine = live_engine(name, two_zoned_windows());
    engine.force_debug_log();
    enable_workspaces(&engine);
    engine.with_platform(|p| p.focused = Some(WindowId(2)));
    event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
    settle(&engine).await;
    assert!(is_parked(&engine, 2));
    engine.with_platform(|p| p.focused = Some(WindowId(1)));
    engine
}

/// The system shows window 2 again and, while it rearranges displays, ignores moves.
fn shown_by_the_system(engine: &Engine<FakePlatform>) {
    engine.with_platform(|p| p.windows.get_mut(&WindowId(2)).unwrap().frame = RIGHT_ZONE);
    frozen(engine, 2, true);
    event(engine, PlatformEvent::WindowMoved(WindowId(2)));
}

/// The side display arrives, and the system shows window 2 again while it settles.
fn side_display_connects(engine: &Engine<FakePlatform>) {
    engine.with_platform(|p| p.displays.push(side_display()));
    event(engine, PlatformEvent::DisplaysChanged);
    shown_by_the_system(engine);
}

/// Wait out a display update and its retry, then containment's own retry, which together
/// take longer than one `settle`.
async fn settle_and_retry(engine: &Engine<FakePlatform>) {
    settle(engine).await;
    settle(engine).await;
}

/// Where window 2 parks with the main and side displays.
fn parked_with_both_displays(engine: &Engine<FakePlatform>) -> Point {
    let frames: Vec<_> = engine.with_platform(|p| p.displays.iter().map(|display| display.frame).collect());
    parking_origin(ParkingPolicy::CornerSliver, frame_of(engine, 2), &frames).unwrap()
}

/// The warnings the engine posted for the user, oldest first.
fn warnings(name: &str) -> Vec<String> {
    logged(name, LogEvent::Status)
        .iter()
        .filter(|record| record["level"] == "warn")
        .map(|record| record["message"].as_str().unwrap().to_owned())
        .collect()
}

fn outcomes(name: &str) -> Vec<(u64, String)> {
    logged(name, LogEvent::Containment)
        .iter()
        .map(|record| {
            (
                record["attempt"].as_u64().unwrap(),
                record["outcome"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

#[test]
fn a_window_shown_while_displays_change_is_parked_again_once_they_settle_without_a_warning() {
    let name = "contain-display-change";
    scenario(|| async {
        let engine = window_two_parked(name).await;
        side_display_connects(&engine);
        assert_eq!(
            warnings(name),
            Vec::<String>::new(),
            "nothing is said while displays settle"
        );
        assert_eq!(
            frame_of(&engine, 2),
            RIGHT_ZONE,
            "left alone while displays settle"
        );

        frozen(&engine, 2, false);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2).origin(), parked_with_both_displays(&engine));
        assert_eq!(warnings(name), Vec::<String>::new());
        assert_eq!(
            outcomes(name),
            [],
            "the display update parked it, not containment"
        );
        let updates = logged(name, LogEvent::DisplaysUpdated);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0]["held"], 1);
        assert_eq!(updates[0]["moved"], 1);
    });
}

#[test]
fn a_wake_with_displays_coming_back_in_steps_never_warns_and_ends_with_windows_parked() {
    let name = "contain-wake";
    scenario(|| async {
        let engine = window_two_parked(name).await;
        engine.with_platform(|p| p.displays.push(side_display()));
        wake(&engine).await;
        frozen(&engine, 2, false);
        settle(&engine).await;

        assert_eq!(frame_of(&engine, 2).origin(), parked_with_both_displays(&engine));
        assert_eq!(warnings(name), Vec::<String>::new());
        assert_eq!(active(&engine), "workspace-1");
        assert_eq!(outcomes(name), [], "nothing was tried while displays settled");
        assert_eq!(
            logged(name, LogEvent::DisplaysUpdated).len(),
            2,
            "one update per settled change"
        );
    });
}

/// What a Mac does on waking with an external display: the displays change, then two
/// seconds later the desktop and the displays again, and each time the system shows a
/// parked window and ignores moves for a moment.
async fn wake(engine: &Engine<FakePlatform>) {
    event(engine, PlatformEvent::DisplaysChanged);
    shown_by_the_system(engine);
    tokio::time::sleep(Duration::from_millis(100)).await;
    frozen(engine, 2, false);
    tokio::time::sleep(Duration::from_secs(2)).await;
    event(engine, PlatformEvent::NativeDesktopChanged);
    event(engine, PlatformEvent::DisplaysChanged);
    shown_by_the_system(engine);
}

#[test]
fn a_window_that_wont_stay_hidden_after_displays_settle_is_reported_once_with_what_to_do() {
    let name = "contain-refused";
    scenario(|| async {
        let engine = window_two_parked(name).await;
        side_display_connects(&engine);
        settle_and_retry(&engine).await;

        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        let notice = "A window from Workspace 2 won't stay hidden; switch there and back to hide it.";
        assert_eq!(warnings(name), [notice]);
        assert!(
            engine.status().unwrap().is_error,
            "it stays until it no longer holds"
        );
        assert_eq!(
            outcomes(name),
            [(1, "retrying".to_owned()), (2, "stillShowing".to_owned())]
        );
        let records = logged(name, LogEvent::Containment);
        assert!(
            records
                .iter()
                .all(|record| record["app"] == "com.example.browser")
        );
        assert_eq!(
            logged(name, LogEvent::DisplaysUpdated).len(),
            2,
            "the update and its retry"
        );
    });
}

#[test]
fn switching_there_and_back_hides_the_window_and_takes_the_notice_back() {
    let name = "contain-switch-back";
    scenario(|| async {
        let engine = window_two_parked(name).await;
        side_display_connects(&engine);
        settle_and_retry(&engine).await;
        assert!(engine.status().unwrap().is_error);

        frozen(&engine, 2, false);
        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 2)));
        settle(&engine).await;
        let status = engine.status().unwrap();
        assert!(
            !status.is_error,
            "the notice is gone once the window's workspace shows"
        );
        assert!(!status.notify);

        event(&engine, PlatformEvent::HotKey(switch_to(&engine, 1)));
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2).origin(), parked_with_both_displays(&engine));
        assert_eq!(warnings(name).len(), 1);
    });
}

#[test]
fn an_app_that_gives_in_on_the_second_try_is_never_reported() {
    let name = "contain-second-try";
    scenario(|| async {
        let engine = window_two_parked(name).await;
        let parked = frame_of(&engine, 2);
        shown_by_the_system(&engine);
        assert_eq!(frame_of(&engine, 2), RIGHT_ZONE);
        assert_eq!(warnings(name), Vec::<String>::new());

        frozen(&engine, 2, false);
        settle(&engine).await;
        assert_eq!(frame_of(&engine, 2), parked);
        assert_eq!(warnings(name), Vec::<String>::new());
        assert_eq!(
            outcomes(name),
            [(1, "retrying".to_owned()), (2, "hiddenAgain".to_owned())]
        );
    });
}

#[test]
fn a_window_that_closes_while_reported_takes_the_notice_with_it() {
    let name = "contain-closed";
    scenario(|| async {
        let engine = window_two_parked(name).await;
        shown_by_the_system(&engine);
        settle(&engine).await;
        assert!(engine.status().unwrap().is_error);

        engine.with_platform(|p| p.windows.remove(&WindowId(2)));
        event(&engine, PlatformEvent::WindowDestroyed(WindowId(2)));
        assert!(!engine.status().unwrap().is_error);
    });
}
