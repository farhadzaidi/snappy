//! On macOS, which can't lower another app's window: raising the windows over the window
//! sent back, awaiting each step in the window list, and leaving out what can't be raised.

use std::time::Duration;

use snappy_core::geometry::Rect;
use snappy_engine::LogEvent;
use snappy_fake::{FakePlatform, FakeWindow};
use snappy_platform::{PlatformEvent, WindowId};
use tokio::time::Instant;

use super::support::{
    cascade, focused_windows, logging_engine, press, raised_windows, send_to_back_key, sent, stack,
    three_cascaded, top_of_left_zone,
};
use crate::support::{LEFT_ZONE, event, live_engine, logged, scenario, status_text};

#[test]
fn a_press_raises_every_window_that_shows_where_the_window_was_on_macos() {
    scenario(|| async {
        let name = "send-to-back-cascade";
        let engine = logging_engine(name, three_cascaded());
        press(&engine).await;
        assert_eq!(focused_windows(&engine), [2]);
        assert_eq!(raised_windows(&engine), [3], "never the window sent back");
        assert_eq!(stack(&engine), [2, 3, 1, 9]);
        assert_eq!(logged(name, LogEvent::SentToBack)[0]["raises"], 2);
    });
}

#[test]
fn a_press_while_the_last_ones_raises_are_awaited_waits_its_turn() {
    scenario(|| async {
        let name = "send-to-back-press-mid-raise";
        let mut fake = three_cascaded();
        fake.stack_lag = 5;
        let engine = logging_engine(name, fake);
        let id = send_to_back_key(&engine);
        event(&engine, PlatformEvent::HotKey(id));
        event(&engine, PlatformEvent::HotKeyReleased(id));
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(
            sent(name).is_empty(),
            "the first press's raises are still awaited"
        );
        event(&engine, PlatformEvent::HotKey(id));
        event(&engine, PlatformEvent::HotKeyReleased(id));
        engine.idle().await;
        assert_eq!(sent(name), [(1, 2), (2, 3)]);
        assert_eq!(stack(&engine), [3, 2, 1, 9]);
    });
}

#[test]
fn a_window_that_cant_be_raised_is_left_out_and_the_others_go_on() {
    scenario(|| async {
        let name = "send-to-back-raise-refused";
        let mut fake = three_cascaded();
        fake.windows.get_mut(&WindowId(3)).unwrap().hung = true;
        let engine = logging_engine(name, fake);
        press(&engine).await;
        assert_eq!(raised_windows(&engine), [3]);
        assert_eq!(stack(&engine), [2, 1, 3, 9]);
        let record = logged(name, LogEvent::SentToBack).pop().unwrap();
        assert_eq!((&record["focus"], &record["reached"]), (&2.into(), &true.into()));
    });
}

#[test]
fn a_press_whose_window_focused_never_shows_in_front_stops_there_and_says_so() {
    scenario(|| async {
        let name = "send-to-back-focus-unshown";
        let mut fake = three_cascaded();
        fake.stack_lag = usize::MAX;
        let engine = logging_engine(name, fake);
        press(&engine).await;
        assert_eq!(focused_windows(&engine), [2]);
        assert!(raised_windows(&engine).is_empty(), "no raise goes on without it");
        assert_eq!(
            status_text(&engine),
            "The window behind didn't come to the front. Try again."
        );
        let record = logged(name, LogEvent::SentToBack).pop().unwrap();
        assert_eq!(record["level"], "warn");
        assert!(sent(name).is_empty());
    });
}

#[test]
fn a_press_out_of_time_while_its_focus_is_awaited_still_sends_every_raise() {
    scenario(|| async {
        let name = "send-to-back-out-of-time-focusing";
        let mut fake = three_cascaded();
        fake.stack_lag = usize::MAX;
        let engine = logging_engine(name, fake);
        event(&engine, PlatformEvent::HotKey(send_to_back_key(&engine)));
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(focused_windows(&engine), [2], "the focus is awaited");
        // Past the press's whole time while its focus is awaited, as when reading the
        // windows took most of it.
        tokio::time::advance(Duration::from_millis(900)).await;
        engine.idle().await;
        assert_eq!(raised_windows(&engine), [3]);
        assert_eq!(stack(&engine), [2, 3, 1, 9]);
        assert!(!engine.status().is_some_and(|status| status.is_error));
        let record = logged(name, LogEvent::SentToBack).pop().unwrap();
        assert_eq!(
            (&record["reached"], &record["level"]),
            (&false.into(), &"warn".into())
        );
        assert_eq!(sent(name), [(1, 2)]);
    });
}

#[test]
fn raises_of_one_apps_windows_go_out_together_and_only_the_last_is_awaited() {
    scenario(|| async {
        let mut fake = cascade(&["com.example.terminal"; 3]);
        // Each raise shows about 240 ms late.
        fake.stack_lag = 30;
        let engine = live_engine("send-to-back-one-app", fake);
        let started = Instant::now();
        press(&engine).await;
        assert_eq!(focused_windows(&engine), [2]);
        assert_eq!(raised_windows(&engine), [3, 2]);
        assert_eq!(stack(&engine), [2, 3, 1, 9]);
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "one wait, not one per raise: {:?}",
            started.elapsed()
        );
    });
}

/// Windows 1, 2, and 3 of one app, front to back, with 1 filling the left zone and focused,
/// 2 over the top of it, and 3 over its top half, and another app's window 4 over its
/// bottom half, beside 3.
fn one_apps_windows_beside_another_apps() -> FakePlatform {
    let bottom_half = Rect::new(
        LEFT_ZONE.x,
        LEFT_ZONE.y + LEFT_ZONE.height * 0.5,
        LEFT_ZONE.width,
        LEFT_ZONE.height * 0.5,
    );
    let mut fake = FakePlatform::default();
    fake.open(4, FakeWindow::new("com.example.browser", 200, bottom_half));
    fake.open(
        3,
        FakeWindow::new("com.example.terminal", 100, top_of_left_zone(0.5)),
    );
    fake.open(
        2,
        FakeWindow::new("com.example.terminal", 100, top_of_left_zone(0.3)),
    );
    fake.open(1, FakeWindow::new("com.example.terminal", 100, LEFT_ZONE));
    fake
}

#[test]
fn a_window_focused_never_shown_after_its_apps_raises_went_out_leaves_no_raise_unsent() {
    scenario(|| async {
        let name = "send-to-back-focus-unshown-after-raises";
        let mut fake = one_apps_windows_beside_another_apps();
        fake.stack_lag = usize::MAX;
        let engine = logging_engine(name, fake);
        press(&engine).await;
        assert_eq!(focused_windows(&engine), [2]);
        assert_eq!(
            raised_windows(&engine),
            [3, 2, 4],
            "3 and 2 went out with the focus, unawaited, so 4 goes too"
        );
        assert_eq!(stack(&engine), [2, 4, 3, 1]);
        let record = logged(name, LogEvent::SentToBack).pop().unwrap();
        assert_eq!(
            (&record["reached"], &record["level"]),
            (&false.into(), &"warn".into())
        );
        assert_eq!(sent(name), [(1, 2)]);
    });
}

/// Windows 1 to 6, each of its own app, in a cascade.
fn six_cascaded() -> FakePlatform {
    let apps: Vec<String> = (1..=6).map(|id| format!("com.example.app{id}")).collect();
    let apps: Vec<&str> = apps.iter().map(String::as_str).collect();
    cascade(&apps)
}

#[test]
fn a_press_out_of_time_still_sends_every_raise_and_logs_the_order_unseen() {
    scenario(|| async {
        let name = "send-to-back-out-of-time";
        let mut fake = six_cascaded();
        // Each raise shows about 240 ms late, so the fourth would end past the limit.
        fake.stack_lag = 30;
        let engine = logging_engine(name, fake);
        press(&engine).await;
        assert_eq!(focused_windows(&engine), [2]);
        assert_eq!(raised_windows(&engine), [6, 5, 4, 3]);
        assert_eq!(
            stack(&engine),
            [2, 3, 4, 5, 6, 1, 9],
            "none buried behind one raised before it"
        );
        let record = logged(name, LogEvent::SentToBack).pop().unwrap();
        assert_eq!(
            (&record["reached"], &record["level"]),
            (&false.into(), &"warn".into())
        );
    });
}

#[test]
fn a_window_closing_mid_press_never_stops_the_window_focused_coming_to_the_front() {
    scenario(|| async {
        let name = "send-to-back-closed-mid-press";
        let mut fake = three_cascaded();
        fake.stack_lag = 2;
        let engine = logging_engine(name, fake);
        let id = send_to_back_key(&engine);
        event(&engine, PlatformEvent::HotKey(id));
        event(&engine, PlatformEvent::HotKeyReleased(id));
        tokio::time::sleep(Duration::from_millis(1)).await;
        engine.with_platform(|p| {
            p.windows.remove(&WindowId(3));
            p.stacking.retain(|window| *window != WindowId(3));
        });
        engine.idle().await;
        assert_eq!(sent(name), [(1, 2)]);
        assert_eq!(stack(&engine), [2, 1, 9]);
        assert!(!engine.status().is_some_and(|status| status.is_error));
        let record = logged(name, LogEvent::SentToBack).pop().unwrap();
        assert_eq!(record["reached"], false, "the order with window 3 never showed");
    });
}

#[test]
fn an_app_whose_window_cant_be_raised_is_left_out_with_all_its_windows() {
    scenario(|| async {
        let name = "send-to-back-app-hung";
        let mut fake = cascade(&[
            "com.example.editor",
            "com.example.browser",
            "com.example.terminal",
            "com.example.terminal",
        ]);
        for id in [3, 4] {
            fake.windows.get_mut(&WindowId(id)).unwrap().hung = true;
        }
        let engine = logging_engine(name, fake);
        press(&engine).await;
        assert_eq!(
            raised_windows(&engine),
            [4],
            "never 3, whose app didn't answer for 4"
        );
        assert_eq!(stack(&engine), [2, 1, 3, 4, 9]);
        assert_eq!(sent(name), [(1, 2)]);
    });
}
