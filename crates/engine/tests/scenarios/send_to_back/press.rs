//! Pressing and letting go: which shortcut reports its release, when a press counts as
//! one, and which window the next press sends while focus catches up.

use snappy_engine::Input;
use snappy_ipc::Command;
use snappy_platform::{CapsLockInput, PlatformEvent, WindowId, WindowSystem};
use tokio::time::Instant;

use super::support::{
    PAST_HOLD, focused_windows, frontmost, hold, let_go, logging_engine, lowering, press, put_back,
    send_to_back_key, sent, stack, three_stacked,
};
use crate::support::{command, event, live_engine, scenario, status_text};

#[test]
fn only_sending_to_the_back_reports_its_release() {
    scenario(|| async {
        let engine = live_engine("send-to-back-registered", three_stacked());
        let reporting = engine.with_platform(|p| p.reporting_release.clone());
        assert_eq!(
            reporting.into_iter().collect::<Vec<_>>(),
            [send_to_back_key(&engine)]
        );
    });
}

#[test]
fn a_press_sends_the_window_back_before_it_is_let_go() {
    scenario(|| async {
        let name = "send-to-back-press-alone";
        let engine = logging_engine(name, three_stacked());
        event(&engine, PlatformEvent::HotKey(send_to_back_key(&engine)));
        engine.idle().await;
        assert_eq!(sent(name), [(1, 2)]);
        let_go(&engine).await;
        assert_eq!(
            sent(name),
            [(1, 2)],
            "a quick press does nothing more as it's let go"
        );
        assert!(put_back(name).is_empty());
    });
}

#[test]
fn a_press_before_focus_follows_the_last_one_sends_the_window_it_focused() {
    scenario(|| async {
        let name = "send-to-back-focus-late";
        let engine = logging_engine(name, lowering(three_stacked()));
        press(&engine).await;
        // The platform still reports focus on window 1, as it does for a moment after.
        engine.with_platform(|p| p.focused = Some(WindowId(1)));
        press(&engine).await;
        assert_eq!(sent(name), [(1, 2), (2, 3)]);
        assert_eq!(stack(&engine), [3, 4, 1, 2]);
    });
}

#[test]
fn a_focus_change_reported_after_a_press_leaves_the_next_on_the_focused_window() {
    scenario(|| async {
        let name = "send-to-back-focus-moved-on";
        let engine = logging_engine(name, lowering(three_stacked()));
        press(&engine).await;
        // Focus goes to window 1 without raising it, as focus following the pointer does.
        engine.with_platform(|p| p.focused = Some(WindowId(1)));
        event(
            &engine,
            PlatformEvent::FocusChanged {
                pid: 100,
                by_activation: false,
            },
        );
        press(&engine).await;
        assert_eq!(sent(name), [(1, 2)], "window 1, at the back, covers nothing");
        assert_eq!(status_text(&engine), "There's no window behind this one.");
    });
}

#[test]
fn a_window_sent_back_and_clicked_to_the_front_is_sent_back_again() {
    scenario(|| async {
        let name = "send-to-back-clicked-back";
        let engine = logging_engine(name, three_stacked());
        press(&engine).await;
        engine.with_platform(|p| p.focus(WindowId(1)));
        press(&engine).await;
        assert_eq!(sent(name), [(1, 2), (1, 2)]);
    });
}

#[test]
fn a_press_is_timed_from_when_it_arrived_not_when_the_engine_got_to_it() {
    scenario(|| async {
        let name = "send-to-back-press-arrived-earlier";
        let engine = logging_engine(name, three_stacked());
        tokio::time::sleep(PAST_HOLD).await;
        // The press arrived while the engine was busy, and was let go a hold later.
        engine.handle(Input::Platform {
            event: PlatformEvent::HotKey(send_to_back_key(&engine)),
            received: Instant::now() - PAST_HOLD,
        });
        let_go(&engine).await;
        assert_eq!(sent(name), [(1, 2)]);
        assert_eq!(put_back(name), [1], "a hold, though handled at once");
    });
}

#[test]
fn a_press_whose_release_went_unseen_counts_as_quick() {
    scenario(|| async {
        let name = "send-to-back-unseen-release";
        let engine = logging_engine(name, lowering(three_stacked()));
        hold(&engine).await;
        event(&engine, PlatformEvent::HotKey(send_to_back_key(&engine)));
        let_go(&engine).await;
        assert_eq!(sent(name), [(1, 2), (2, 3)]);
        assert!(put_back(name).is_empty());
    });
}

#[test]
fn caps_lock_as_the_snappy_key_presses_and_holds_alike_and_never_toggles() {
    scenario(|| async {
        let name = "send-to-back-caps-lock";
        let engine = logging_engine(name, three_stacked());
        command(&engine, Command::SetCapsLockAsSnappyKey { enabled: true });
        let caps = |input| event(&engine, PlatformEvent::CapsLock(input));

        caps(CapsLockInput::Pressed);
        press(&engine).await;
        caps(CapsLockInput::Released);
        assert_eq!(focused_windows(&engine), [2]);

        caps(CapsLockInput::Pressed);
        hold(&engine).await;
        let_go(&engine).await;
        caps(CapsLockInput::Released);
        assert_eq!(put_back(name), [2]);
        assert_eq!(frontmost(&engine), 2);
        assert_eq!(engine.with_platform(|p| p.caps_lock_toggles), 0);
    });
}
