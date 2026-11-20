//! A press held long enough shows what's behind, and letting go puts the window back,
//! unless something else acted on the windows meanwhile.

use snappy_core::keys::Mods;
use snappy_engine::{Engine, LogEvent};
use snappy_fake::{FakePlatform, FakeWindow, StackChange};
use snappy_platform::{PlatformEvent, WindowId};

use super::support::{
    PAST_HOLD, each_way, focused_windows, frontmost, hold, let_go, logging_engine, lowering, lowering_only,
    press, put_back, send_to_back_key, sent, stack, stack_changes, three_cascaded, three_stacked, way,
    writes,
};
use crate::support::{LEFT_ZONE, RIGHT_ZONE, event, focused, hotkey_id, logged, press_on, scenario, snappy};

#[test]
fn holding_shows_whats_behind_and_letting_go_puts_every_window_back_as_it_was() {
    for fake in each_way(three_cascaded()) {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-hold-{way}");
            let engine = logging_engine(&name, fake.clone());
            let before = stack(&engine);
            hold(&engine).await;
            assert_eq!(sent(&name), [(1, 2)], "{way}: sent as it was pressed");
            assert_eq!(frontmost(&engine), 2, "{way}");
            let_go(&engine).await;
            assert_eq!(stack(&engine), before, "{way}");
            assert_eq!(focused(&engine), Some(WindowId(1)), "{way}");
            assert_eq!(put_back(&name), [1], "{way}");
            assert!(writes(&engine).is_empty(), "{way}: nothing moves");
        });
    }
}

#[test]
fn a_window_held_back_from_behind_another_goes_back_under_it_where_the_platform_can_restack() {
    for (fake, order, restacked) in [
        (three_stacked(), [2, 1, 3, 4], None),
        (lowering(three_stacked()), [1, 2, 3, 4], Some(true)),
        (lowering_only(three_stacked()), [2, 1, 3, 4], None),
    ] {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-hold-under-{way}");
            let engine = logging_engine(&name, fake.clone());
            // Focus moves to window 2 without raising it, as some apps' own windows do.
            engine.with_platform(|p| p.focused = Some(WindowId(2)));
            hold(&engine).await;
            let_go(&engine).await;
            assert_eq!(stack(&engine), order, "{way}");
            assert_eq!(focused(&engine), Some(WindowId(2)), "{way}");
            let record = logged(&name, LogEvent::PutBack).pop().unwrap();
            assert_eq!(
                (&record["under"], record["restacked"].as_bool()),
                (&1.into(), restacked),
                "{way}: only Windows and X11 can put it under 1; elsewhere it stays in front, focused"
            );
            let asked_to_restack = stack_changes(&engine)
                .iter()
                .any(|(_, change)| matches!(change, StackChange::StackBelow(_)));
            assert_eq!(asked_to_restack, restacked.is_some(), "{way}");
        });
    }
}

#[test]
fn letting_go_mid_press_puts_the_window_back_once_the_press_is_done() {
    scenario(|| async {
        let name = "send-to-back-let-go-mid-press";
        let mut fake = three_cascaded();
        // Each raise shows about 240 ms late, so the send outlasts the hold.
        fake.stack_lag = 30;
        let engine = logging_engine(name, fake);
        let before = stack(&engine);
        event(&engine, PlatformEvent::HotKey(send_to_back_key(&engine)));
        tokio::time::sleep(PAST_HOLD).await;
        assert!(sent(name).is_empty(), "the press's raises are still awaited");
        let_go(&engine).await;
        assert_eq!(sent(name), [(1, 2)]);
        assert_eq!(focused_windows(&engine), [2, 1]);
        assert_eq!(stack(&engine), before);
    });
}

#[test]
fn a_change_of_desktop_after_letting_go_mid_press_leaves_the_window_at_the_back() {
    scenario(|| async {
        let name = "send-to-back-let-go-mid-press-desktop";
        let mut fake = three_cascaded();
        fake.stack_lag = 30;
        let engine = logging_engine(name, fake);
        event(&engine, PlatformEvent::HotKey(send_to_back_key(&engine)));
        tokio::time::sleep(PAST_HOLD).await;
        event(&engine, PlatformEvent::HotKeyReleased(send_to_back_key(&engine)));
        event(&engine, PlatformEvent::NativeDesktopChanged);
        engine.idle().await;
        assert_eq!(sent(name), [(1, 2)]);
        assert!(put_back(name).is_empty());
        assert_eq!(
            focused_windows(&engine),
            [2],
            "never back to 1, on the desktop left"
        );
    });
}

#[test]
fn a_window_held_back_with_only_windows_beside_it_in_front_goes_back_in_front_of_the_one_focused() {
    for fake in each_way({
        let mut fake = FakePlatform::default();
        fake.open(2, FakeWindow::new("com.example.browser", 200, LEFT_ZONE));
        fake.open(1, FakeWindow::new("com.example.editor", 100, LEFT_ZONE));
        fake.open(4, FakeWindow::new("com.example.notes", 400, RIGHT_ZONE));
        fake
    }) {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-hold-beside-{way}");
            let engine = logging_engine(&name, fake.clone());
            // Focus is on window 1 behind 4, in the other zone, as focus following the
            // pointer leaves it.
            engine.with_platform(|p| p.focused = Some(WindowId(1)));
            hold(&engine).await;
            assert_eq!(frontmost(&engine), 2, "{way}: focused, 2 came in front of 4");
            let_go(&engine).await;
            assert_eq!(stack(&engine), [1, 2, 4], "{way}: not under 4, behind 2");
            assert_eq!(focused(&engine), Some(WindowId(1)), "{way}");
            let record = logged(&name, LogEvent::PutBack).pop().unwrap();
            assert!(record["under"].is_null(), "{way}");
        });
    }
}

#[test]
fn a_press_after_letting_go_before_focus_follows_sends_the_window_put_back() {
    for fake in each_way(three_stacked()) {
        let way = way(&fake);
        scenario(|| async {
            let name = format!("send-to-back-press-after-put-back-{way}");
            let engine = logging_engine(&name, fake.clone());
            hold(&engine).await;
            let_go(&engine).await;
            assert_eq!(frontmost(&engine), 1, "{way}");
            // The platform still reports focus on window 2, as it does for a moment after.
            engine.with_platform(|p| p.focused = Some(WindowId(2)));
            press(&engine).await;
            assert_eq!(sent(&name), [(1, 2), (1, 2)], "{way}");
            assert_eq!(put_back(&name), [1], "{way}");
        });
    }
}

#[test]
fn a_click_or_another_shortcut_while_holding_leaves_the_window_at_the_back() {
    let click = |engine: &Engine<FakePlatform>| press_on(engine, LEFT_ZONE.center(), Mods::NONE, WindowId(2));
    let shortcut = |engine: &Engine<FakePlatform>| {
        event(
            engine,
            PlatformEvent::HotKey(hotkey_id(engine, &snappy("ArrowRight"))),
        )
    };
    let desktop = |engine: &Engine<FakePlatform>| event(engine, PlatformEvent::NativeDesktopChanged);
    for (what, interrupt) in [
        ("click", &click as &dyn Fn(&Engine<FakePlatform>)),
        ("shortcut", &shortcut),
        ("desktop", &desktop),
    ] {
        scenario(|| async {
            let name = format!("send-to-back-hold-interrupted-{what}");
            let engine = logging_engine(&name, lowering(three_stacked()));
            hold(&engine).await;
            interrupt(&engine);
            let_go(&engine).await;
            assert!(put_back(&name).is_empty(), "{what}");
            assert!(!focused_windows(&engine).contains(&1), "{what}");
        });
    }
}

#[test]
fn only_letting_go_of_send_to_back_puts_its_window_back() {
    scenario(|| async {
        let name = "send-to-back-hold-other-release";
        let engine = logging_engine(name, lowering(three_stacked()));
        hold(&engine).await;
        let other = hotkey_id(&engine, &snappy("ArrowRight"));
        event(&engine, PlatformEvent::HotKeyReleased(other));
        engine.idle().await;
        assert!(put_back(name).is_empty(), "another shortcut's release");
        let_go(&engine).await;
        assert_eq!(put_back(name), [1]);
    });
}
