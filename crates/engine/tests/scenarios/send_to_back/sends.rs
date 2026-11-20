//! Each press sends the focused window behind the windows it covers and focuses a window
//! over where it was, in turn with other operations, on every platform.

use std::time::Duration;

use snappy_core::geometry::Rect;
use snappy_core::send_to_back::Restacking;
use snappy_engine::LogEvent;
use snappy_fake::{FakePlatform, FakeWindow, StackChange};
use snappy_platform::{PlatformEvent, WindowId, WindowSystem};

use super::support::{
    each_way, frontmost, logging_engine, lowering, press, send_to_back_key, sent, stack, stack_changes,
    three_cascaded, three_stacked, top_of_left_zone, way, writes,
};
use crate::support::{
    LEFT_ZONE, RIGHT_ZONE, enable_workspaces, event, focused, frame_of, hotkey_id, is_parked, live_engine,
    logged, scenario, send_to, settle, snappy, status_text, two_zoned_windows,
};

#[test]
fn a_press_sends_the_focused_window_behind_the_windows_it_covers_and_focuses_the_one_in_front() {
    for fake in each_way(three_stacked()) {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-one-press-{way}");
            let engine = logging_engine(&name, fake.clone());
            press(&engine).await;
            assert_eq!(frontmost(&engine), 2, "{way}");
            assert_eq!(focused(&engine), Some(WindowId(2)), "{way}");
            assert!(writes(&engine).is_empty(), "{way}: nothing moves");
            let records = logged(&name, LogEvent::SentToBack);
            assert_eq!(records.len(), 1, "{way}");
            assert_eq!(
                (
                    &records[0]["window"],
                    &records[0]["focus"],
                    &records[0]["covered"]
                ),
                (&1.into(), &2.into(), &2.into()),
                "{way}: not 4, in the other zone"
            );
            if fake.restacking == Restacking::RaiseOnly {
                assert_eq!(stack(&engine), [2, 1, 3, 4], "3 is hidden behind 2 either way");
                assert_eq!(stack_changes(&engine), [(2, StackChange::Focus)]);
                assert_eq!(
                    (&records[0]["raises"], &records[0]["reached"]),
                    (&1.into(), &true.into())
                );
            } else {
                assert_eq!(stack(&engine), [2, 3, 4, 1]);
                assert_eq!(
                    stack_changes(&engine),
                    [(1, StackChange::Lower), (2, StackChange::Focus)]
                );
                assert!(records[0]["reached"].is_null());
            }
        });
    }
}

#[test]
fn presses_go_round_every_window_stacked_there_or_on_macos_every_one_that_shows() {
    for (stacked, fake, round) in [
        ("stacked", three_stacked(), [2, 1, 2, 1]),
        ("stacked", lowering(three_stacked()), [2, 3, 1, 2]),
        ("cascaded", three_cascaded(), [2, 3, 2, 1]),
    ] {
        let way = way(&fake);
        scenario(|| async {
            let engine = live_engine(&format!("send-to-back-presses-{stacked}-{way}"), fake.clone());
            let mut front = Vec::new();
            for _ in 0..4 {
                press(&engine).await;
                front.push(frontmost(&engine));
            }
            assert_eq!(front, round, "{stacked}, {way}");
        });
    }
}

#[test]
fn quick_presses_each_send_a_window_to_the_back_in_turn() {
    for (fake, order) in [
        (three_stacked(), [1, 2, 3, 4]),
        (lowering(three_stacked()), [3, 4, 1, 2]),
    ] {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-quick-presses-{way}");
            let engine = logging_engine(&name, fake.clone());
            let id = send_to_back_key(&engine);
            for _ in 0..2 {
                event(&engine, PlatformEvent::HotKey(id));
                event(&engine, PlatformEvent::HotKeyReleased(id));
            }
            engine.idle().await;
            let second = if fake.restacking == Restacking::RaiseOnly {
                1
            } else {
                3
            };
            assert_eq!(sent(&name), [(1, 2), (2, second)], "{way}");
            assert_eq!(stack(&engine), order, "{way}");
        });
    }
}

#[test]
fn a_press_while_another_operation_is_under_way_goes_after_it() {
    scenario(|| async {
        let name = "send-to-back-press-after-snap";
        let mut fake = three_stacked();
        fake.restore_delay = Duration::from_millis(300);
        fake.windows.get_mut(&WindowId(4)).unwrap().frame = Rect::new(800.0, 100.0, 400.0, 400.0);
        fake.focused = Some(WindowId(4));
        let engine = logging_engine(name, fake);
        event(
            &engine,
            PlatformEvent::HotKey(hotkey_id(&engine, &snappy("Digit2"))),
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert_eq!(
            writes(&engine).first(),
            Some(&(4, "beginBoundsChange")),
            "the snap is under way"
        );
        engine.with_platform(|p| p.focused = Some(WindowId(1)));
        press(&engine).await;
        assert_eq!(sent(name), [(1, 2)]);
        assert_eq!(frontmost(&engine), 2);
    });
}

#[test]
fn a_focused_window_behind_another_leaves_focus_to_the_window_in_front_of_it() {
    for (fake, order) in [
        (three_stacked(), [1, 2, 3, 4]),
        (lowering(three_stacked()), [1, 3, 4, 2]),
    ] {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-focus-not-in-front-{way}");
            let engine = logging_engine(&name, fake.clone());
            // Focus moves to window 2 without raising it, as some apps' own windows do.
            engine.with_platform(|p| p.focused = Some(WindowId(2)));
            press(&engine).await;
            assert_eq!(sent(&name), [(2, 1)], "{way}: 1 is over 2's frame, in front");
            assert_eq!(stack(&engine), order, "{way}");
        });
    }
}

/// A window floating in front, over three windows in zones of their own, top left, bottom
/// left, and bottom middle, front to back: Terminal windows 1, 2, and 4, and a browser's 3.
fn floating_over_three_zones() -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(4, FakeWindow::new("com.apple.Terminal", 100, TOP_LEFT));
    fake.open(3, FakeWindow::new("com.google.Chrome", 300, BOTTOM_LEFT));
    fake.open(2, FakeWindow::new("com.apple.Terminal", 100, BOTTOM_MIDDLE));
    fake.open(1, FakeWindow::new("com.apple.Terminal", 100, FLOATING));
    fake
}

const TOP_LEFT: Rect = Rect::new(0.0, 25.0, 480.0, 437.0);
const BOTTOM_LEFT: Rect = Rect::new(0.0, 462.0, 480.0, 438.0);
const BOTTOM_MIDDLE: Rect = Rect::new(480.0, 462.0, 480.0, 438.0);
const FLOATING: Rect = Rect::new(240.0, 200.0, 600.0, 500.0);

#[test]
fn a_floating_window_goes_behind_the_zones_it_covers_and_the_second_press_leaves_it_behind_the_others() {
    for fake in each_way(floating_over_three_zones()) {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-floating-{way}");
            let engine = logging_engine(&name, fake.clone());
            press(&engine).await;
            assert_eq!(sent(&name), [(1, 2)], "{way}");
            let first = stack(&engine);
            let place = |id: u64| first.iter().position(|window| *window == id).unwrap();
            assert!(
                [2, 3, 4].iter().all(|id| place(*id) < place(1)),
                "{way}: {first:?}"
            );
            press(&engine).await;
            assert_eq!(
                sent(&name),
                [(1, 2), (2, 3)],
                "{way}: focusing the floating window would raise it over 3 and 4"
            );
            let second = stack(&engine);
            let place = |id: u64| second.iter().position(|window| *window == id).unwrap();
            assert!(
                place(3) < place(1) && place(4) < place(1) && place(1) < place(2),
                "{way}: {second:?}"
            );
            assert_eq!(focused(&engine), Some(WindowId(3)), "{way}");
        });
    }
}

/// Window 1 and 2 stacked in the left zone, front to back, with 1 focused and 2 filling
/// only its top half, window 3 in the right zone, and window 4 over both zones, behind them
/// all.
fn one_over_both_zones() -> FakePlatform {
    let mut fake = FakePlatform::default();
    fake.open(4, FakeWindow::new("com.example.viewer", 400, OVER_BOTH));
    fake.open(
        2,
        FakeWindow::new("com.example.browser", 200, top_of_left_zone(0.5)),
    );
    fake.open(3, FakeWindow::new("com.example.notes", 300, RIGHT_ZONE));
    fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
    fake
}

const OVER_BOTH: Rect = Rect::new(200.0, 100.0, 1000.0, 600.0);

#[test]
fn a_window_in_front_of_a_covered_one_stays_in_front_of_it() {
    for fake in each_way(one_over_both_zones()) {
        let way = way(&fake);
        scenario(|| async {
            let engine = live_engine(&format!("send-to-back-over-both-{way}"), fake.clone());
            press(&engine).await;
            assert_eq!(stack(&engine), [2, 3, 4, 1], "{way}: 3 is still in front of 4");
        });
    }
}

#[test]
fn a_window_of_another_workspace_its_app_shows_again_is_never_covered() {
    scenario(|| async {
        let name = "send-to-back-other-workspace";
        let engine = logging_engine(name, three_stacked());
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(3)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        assert!(is_parked(&engine, 3));
        engine.with_platform(|p| {
            p.windows.get_mut(&WindowId(3)).unwrap().frame = LEFT_ZONE;
            p.focus(WindowId(1));
        });
        for _ in 0..3 {
            press(&engine).await;
        }
        assert_eq!(sent(name), [(1, 2), (2, 1), (1, 2)]);
    });
}

#[test]
fn a_window_parked_at_a_corner_overlaps_one_there_by_a_sliver_and_is_never_covered() {
    scenario(|| async {
        let name = "send-to-back-corner-sliver";
        let display = FakePlatform::main_display().frame;
        let corner = Rect::new(display.max_x() - 600.0, display.max_y() - 400.0, 600.0, 400.0);
        let mut fake = FakePlatform::default();
        fake.open(2, FakeWindow::new("com.example.browser", 200, corner));
        fake.open(1, FakeWindow::new("com.example.editor", 100, corner));
        let engine = logging_engine(name, fake);
        enable_workspaces(&engine);
        engine.with_platform(|p| p.focused = Some(WindowId(2)));
        event(&engine, PlatformEvent::HotKey(send_to(&engine, 2)));
        settle(&engine).await;
        let sliver = frame_of(&engine, 2).intersection(display).unwrap();
        assert!(
            sliver.intersection(corner).is_some(),
            "parked at the corner, it overlaps window 1"
        );
        engine.with_platform(|p| p.focus(WindowId(1)));
        press(&engine).await;
        assert!(sent(name).is_empty());
        assert_eq!(status_text(&engine), "There's no window behind this one.");
    });
}

#[test]
fn a_window_covering_nothing_stays_where_it_is_and_says_so_quietly() {
    scenario(|| async {
        let name = "send-to-back-nothing";
        let engine = logging_engine(name, two_zoned_windows());
        press(&engine).await;
        assert!(sent(name).is_empty());
        assert!(stack_changes(&engine).is_empty());
        assert_eq!(status_text(&engine), "There's no window behind this one.");
        assert!(!engine.status().unwrap().is_error);
    });
}
