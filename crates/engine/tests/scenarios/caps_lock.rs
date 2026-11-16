//! Caps Lock as the Snappy key.

use snappy_core::keys::{Mods, SnappyKey};
use snappy_engine::{Engine, LogEvent};
use snappy_fake::FakePlatform;
use snappy_ipc::Command;
use snappy_platform::{CapsLockInput, CapsLockRefusal, CapsLockUndo, PlatformEvent};

use crate::support::{
    FLOATING, app_state, command, control_option, event, hotkey_id, live_engine, logged, paths_kept,
    relaunched, reply_to, scenario, snappy, status_text, with_window,
};

const REMAPPED: &str = "Caps Lock is already remapped.";

fn turn_on(engine: &Engine<FakePlatform>) -> Option<String> {
    reply_to(engine, Command::SetCapsLockAsSnappyKey { enabled: true }).map(|refusal| refusal.message)
}

fn held_as(engine: &Engine<FakePlatform>) -> Option<Mods> {
    engine.with_platform(|p| p.caps_lock_held_as)
}

fn toggles(engine: &Engine<FakePlatform>) -> usize {
    engine.with_platform(|p| p.caps_lock_toggles)
}

fn undo_kept(name: &str) -> bool {
    paths_kept(name).caps_lock().exists()
}

/// Press Caps Lock, let `seconds` pass, and let it go.
fn press_for(engine: &Engine<FakePlatform>, seconds: f64) {
    event(engine, PlatformEvent::CapsLock(CapsLockInput::Pressed));
    engine.with_platform(|p| p.time += seconds);
    event(engine, PlatformEvent::CapsLock(CapsLockInput::Released));
}

/// The settings file as saved, with Caps Lock as the Snappy key set to `on`.
fn save_setting(name: &str, on: bool) {
    let path = paths_kept(name).settings;
    let mut saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    saved["capsLockAsSnappyKey"] = on.into();
    std::fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
}

#[test]
fn turning_it_on_holds_caps_lock_as_the_snappy_key_and_keeps_what_undoes_it() {
    scenario(|| async {
        let name = "caps-lock-on";
        let engine = live_engine(name, with_window(FLOATING));
        assert_eq!(held_as(&engine), None, "off by default");

        assert_eq!(turn_on(&engine), None);
        assert!(engine.settings().caps_lock_as_snappy_key);
        assert_eq!(held_as(&engine), Some(SnappyKey::DEFAULT.modifiers()));
        assert!(undo_kept(name));
        let shortcuts = engine.with_platform(|p| p.registered.len());
        assert!(shortcuts > 0, "the shortcuts themselves don't change");
    });
}

#[test]
fn a_caps_lock_the_system_already_remaps_is_refused_and_the_setting_stays_off() {
    scenario(|| async {
        let name = "caps-lock-refused";
        let mut fake = with_window(FLOATING);
        fake.caps_lock_refusal = Some(CapsLockRefusal::Refused(REMAPPED.to_owned()));
        let engine = live_engine(name, fake);

        assert_eq!(turn_on(&engine).as_deref(), Some(REMAPPED));
        assert!(!engine.settings().caps_lock_as_snappy_key);
        assert_eq!(held_as(&engine), None);
        assert!(!undo_kept(name));
        assert_eq!(app_state(&engine).caps_lock_problem, None, "the reply says why");
    });
}

#[test]
fn a_caps_lock_that_waits_for_a_new_login_stays_on_and_says_so_until_it_works() {
    scenario(|| async {
        let name = "caps-lock-after-login";
        let after_login = "Caps Lock becomes the Snappy key after you log out and back in.";
        let mut fake = with_window(FLOATING);
        fake.caps_lock_refusal = Some(CapsLockRefusal::AfterLogin(after_login.to_owned()));
        let engine = live_engine(name, fake);

        assert_eq!(turn_on(&engine), None, "turning it on is accepted");
        assert!(engine.settings().caps_lock_as_snappy_key);
        assert_eq!(held_as(&engine), None, "not yet");
        assert_eq!(
            app_state(&engine).caps_lock_problem.as_deref(),
            Some(after_login),
            "the notice stays beside the setting"
        );
        let status = engine.status().map(|status| status.is_error);
        assert_ne!(status, Some(true), "waiting for a login is no error");

        // The next login starts Snappy again, and the desktop now takes the change.
        let mut system = engine.with_platform(|p| p.clone());
        drop(engine);
        system.caps_lock_refusal = None;
        let engine = relaunched(name, system);
        assert!(held_as(&engine).is_some());
        assert_eq!(app_state(&engine).caps_lock_problem, None);
    });
}

#[test]
fn a_quick_tap_alone_toggles_caps_lock_and_a_hold_never_does() {
    scenario(|| async {
        let name = "caps-lock-taps";
        let engine = live_engine(name, with_window(FLOATING));
        engine.force_debug_log();
        turn_on(&engine);

        press_for(&engine, 0.15);
        assert_eq!(toggles(&engine), 1, "a tap");

        event(&engine, PlatformEvent::CapsLock(CapsLockInput::Pressed));
        event(&engine, PlatformEvent::CapsLock(CapsLockInput::OtherInput));
        event(&engine, PlatformEvent::CapsLock(CapsLockInput::Released));
        assert_eq!(toggles(&engine), 1, "another key or a button was used");

        press_for(&engine, 1.0);
        assert_eq!(toggles(&engine), 1, "held too long to be a tap");

        event(&engine, PlatformEvent::CapsLock(CapsLockInput::Pressed));
        let id = hotkey_id(&engine, &snappy("Digit2"));
        event(&engine, PlatformEvent::HotKey(id));
        event(&engine, PlatformEvent::CapsLock(CapsLockInput::Released));
        assert_eq!(toggles(&engine), 1, "a Snappy shortcut fired meanwhile");

        event(&engine, PlatformEvent::CapsLock(CapsLockInput::Released));
        assert_eq!(toggles(&engine), 1, "a release without its press");

        let tapped = logged(name, LogEvent::CapsLockTapped);
        assert_eq!(tapped.len(), 1);
        let released = logged(name, LogEvent::CapsLockReleased);
        assert_eq!(released.len(), 3);
        assert_eq!(released[0]["otherInput"], true);
        assert!(released.iter().all(|record| record.get("key").is_none()));
    });
}

#[test]
fn a_tap_the_system_doesnt_take_is_logged_as_failed_not_as_a_tap() {
    scenario(|| async {
        let name = "caps-lock-tap-failed";
        let engine = live_engine(name, with_window(FLOATING));
        engine.force_debug_log();
        turn_on(&engine);
        engine.with_platform(|p| p.caps_lock_toggle_failure = Some("no keyboard".to_owned()));
        press_for(&engine, 0.1);

        assert!(logged(name, LogEvent::CapsLockTapped).is_empty());
        let failed = logged(name, LogEvent::CapsLockTapFailed);
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0]["reason"], "no keyboard");
    });
}

#[test]
fn presses_while_it_is_off_do_nothing() {
    scenario(|| async {
        let engine = live_engine("caps-lock-off-presses", with_window(FLOATING));
        press_for(&engine, 0.1);
        assert_eq!(toggles(&engine), 0);
    });
}

#[test]
fn a_new_snappy_key_holds_caps_lock_as_its_modifiers() {
    scenario(|| async {
        let engine = live_engine("caps-lock-new-key", with_window(FLOATING));
        turn_on(&engine);
        let error = reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: control_option(),
            },
        );
        assert_eq!(error, None);
        assert_eq!(held_as(&engine), Some(control_option()));
    });
}

#[test]
fn turning_it_off_restores_caps_lock_and_forgets_what_undoes_it() {
    scenario(|| async {
        let name = "caps-lock-off";
        let engine = live_engine(name, with_window(FLOATING));
        turn_on(&engine);
        assert_eq!(
            reply_to(&engine, Command::SetCapsLockAsSnappyKey { enabled: false }),
            None
        );
        assert!(!engine.settings().caps_lock_as_snappy_key);
        assert_eq!(held_as(&engine), None);
        assert!(!undo_kept(name));
        press_for(&engine, 0.1);
        assert_eq!(toggles(&engine), 0);
    });
}

#[test]
fn quitting_and_stopping_for_a_restart_both_restore_caps_lock() {
    scenario(|| async {
        for (name, stop) in [
            ("caps-lock-quit", PlatformEvent::QuitRequested),
            ("caps-lock-restart", PlatformEvent::RestartRequested),
        ] {
            let engine = live_engine(name, with_window(FLOATING));
            turn_on(&engine);
            event(&engine, stop);
            assert_eq!(held_as(&engine), None, "{name}");
            assert!(!undo_kept(name), "{name}");
        }
    });
}

#[test]
fn what_a_crashed_run_left_is_undone_at_the_next_start_before_holding_again() {
    scenario(|| async {
        let name = "caps-lock-crash";
        let engine = live_engine(name, with_window(FLOATING));
        turn_on(&engine);
        // A crash: nothing restores Caps Lock, and the next run starts on the same system.
        let mut system = engine.with_platform(|p| p.clone());
        drop(engine);
        assert!(undo_kept(name));
        system.caps_lock_undo = CapsLockUndo("second run".into());

        let engine = relaunched(name, system);
        assert_eq!(
            engine.with_platform(|p| p.caps_lock_undone.clone()),
            [CapsLockUndo("fake".into())]
        );
        assert_eq!(held_as(&engine), Some(SnappyKey::DEFAULT.modifiers()));
        let kept: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(paths_kept(name).caps_lock()).unwrap()).unwrap();
        assert_eq!(kept["undo"], "second run");
    });
}

#[test]
fn a_record_an_earlier_build_kept_as_text_is_undone_as_the_value_it_holds() {
    scenario(|| async {
        let name = "caps-lock-text-record";
        let engine = live_engine(name, with_window(FLOATING));
        turn_on(&engine);
        let system = engine.with_platform(|p| p.clone());
        drop(engine);
        std::fs::write(paths_kept(name).caps_lock(), r#"{"undo":"{\"keycode\":66}"}"#).unwrap();

        let engine = relaunched(name, system);
        assert_eq!(
            engine.with_platform(|p| p.caps_lock_undone.clone()),
            [CapsLockUndo(serde_json::json!({ "keycode": 66 }))]
        );
    });
}

#[test]
fn what_a_crashed_run_left_is_undone_even_once_the_setting_is_off() {
    scenario(|| async {
        let name = "caps-lock-crash-off";
        let engine = live_engine(name, with_window(FLOATING));
        turn_on(&engine);
        let mut system = engine.with_platform(|p| p.clone());
        drop(engine);
        system.caps_lock_held_as = None;
        save_setting(name, false);

        let engine = relaunched(name, system);
        assert_eq!(engine.with_platform(|p| p.caps_lock_undone.len()), 1);
        assert_eq!(held_as(&engine), None);
        assert!(!undo_kept(name));
    });
}

#[test]
fn without_permission_the_setting_is_kept_and_caps_lock_is_held_once_access_is_granted() {
    scenario(|| async {
        let mut fake = with_window(FLOATING);
        fake.granted = false;
        let engine = live_engine("caps-lock-permission", fake);

        assert_eq!(turn_on(&engine), None);
        assert!(engine.settings().caps_lock_as_snappy_key);
        assert_eq!(held_as(&engine), None);

        engine.with_platform(|p| p.granted = true);
        event(&engine, PlatformEvent::PermissionChanged);
        assert_eq!(held_as(&engine), Some(SnappyKey::DEFAULT.modifiers()));
    });
}

#[test]
fn a_refusal_at_start_is_shown_with_the_setting_and_tried_again_when_the_key_changes() {
    scenario(|| async {
        let name = "caps-lock-refused-at-start";
        let engine = live_engine(name, with_window(FLOATING));
        turn_on(&engine);
        let mut system = engine.with_platform(|p| p.clone());
        event(&engine, PlatformEvent::QuitRequested);
        drop(engine);
        system.caps_lock_refusal = Some(CapsLockRefusal::Refused(REMAPPED.to_owned()));

        let engine = relaunched(name, system);
        assert!(engine.settings().caps_lock_as_snappy_key, "the setting is kept");
        assert_eq!(held_as(&engine), None);
        assert_eq!(app_state(&engine).caps_lock_problem.as_deref(), Some(REMAPPED));

        engine.with_platform(|p| p.caps_lock_refusal = None);
        reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: control_option(),
            },
        );
        assert_eq!(held_as(&engine), Some(control_option()));
        assert_eq!(app_state(&engine).caps_lock_problem, None);
    });
}

/// The engine keeps the hold while recording on every platform. Where Caps Lock holds the
/// modifiers themselves, macOS and Linux, Caps Lock + J then reaches the recorder as the
/// Snappy key + J; Windows' hook runs Snappy's shortcuts itself and gives other apps,
/// the recorder included, nothing (see docs/platforms/windows.md).
#[test]
fn recording_a_shortcut_keeps_caps_lock_as_the_snappy_key() {
    scenario(|| async {
        let engine = live_engine("caps-lock-recording", FakePlatform::default());
        assert_eq!(turn_on(&engine), None);
        command(&engine, Command::BeginRecording);
        assert_eq!(
            held_as(&engine),
            Some(SnappyKey::DEFAULT.modifiers()),
            "still held while recording"
        );
        assert!(engine.with_platform(|p| p.registered.is_empty()));
        command(&engine, Command::EndRecording);
        assert_eq!(held_as(&engine), Some(SnappyKey::DEFAULT.modifiers()));
    });
}

/// Something else changes the keyboard settings under Snappy, and the platform lets go of
/// Caps Lock; on the next hold it answers with `refusal`, if any.
fn lose_caps_lock(engine: &Engine<FakePlatform>, refusal: Option<&str>) {
    engine.with_platform(|p| {
        p.caps_lock_held_as = None;
        p.caps_lock_refusal = refusal.map(|reason| CapsLockRefusal::Refused(reason.to_owned()));
        p.caps_lock_undo = CapsLockUndo("after the loss".into());
    });
    event(engine, PlatformEvent::CapsLockLost);
}

#[test]
fn caps_lock_lost_to_a_reloaded_keymap_is_held_again_with_a_fresh_undo_record() {
    scenario(|| async {
        let name = "caps-lock-lost";
        let engine = live_engine(name, with_window(FLOATING));
        engine.force_debug_log();
        turn_on(&engine);
        lose_caps_lock(&engine, None);

        assert_eq!(held_as(&engine), Some(SnappyKey::DEFAULT.modifiers()));
        let kept: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(paths_kept(name).caps_lock()).unwrap()).unwrap();
        assert_eq!(
            kept["undo"], "after the loss",
            "a crash from here undoes the new hold"
        );
        assert_eq!(logged(name, LogEvent::CapsLockLost).len(), 1);
        assert_eq!(app_state(&engine).caps_lock_problem, None);
        press_for(&engine, 0.1);
        assert_eq!(toggles(&engine), 1, "taps work as before");
    });
}

#[test]
fn caps_lock_lost_to_settings_that_remap_it_stays_off_and_says_why() {
    scenario(|| async {
        let name = "caps-lock-lost-remapped";
        let engine = live_engine(name, with_window(FLOATING));
        turn_on(&engine);
        lose_caps_lock(&engine, Some(REMAPPED));

        assert_eq!(held_as(&engine), None);
        assert!(!undo_kept(name), "nothing of Snappy's is left to undo");
        assert!(engine.settings().caps_lock_as_snappy_key, "the setting is kept");
        assert_eq!(app_state(&engine).caps_lock_problem.as_deref(), Some(REMAPPED));
        assert_eq!(status_text(&engine), REMAPPED);
        press_for(&engine, 0.1);
        assert_eq!(toggles(&engine), 0);

        engine.with_platform(|p| p.caps_lock_refusal = None);
        event(&engine, PlatformEvent::CapsLockLost);
        assert_eq!(held_as(&engine), None, "only a hold in place can be lost");
        reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: control_option(),
            },
        );
        assert_eq!(
            held_as(&engine),
            Some(control_option()),
            "tried again once something changes"
        );
    });
}

const SANDBOXED: &str = "Apps from the App Store can't remap Caps Lock, so it can't be the Snappy key.";

/// A platform built without Caps Lock as the Snappy key. Its hold refuses too, as the
/// adapter's backstop, so any attempt would show as a problem and an error.
fn without_caps_lock() -> FakePlatform {
    let mut fake = with_window(FLOATING);
    fake.caps_lock_unavailable = Some(SANDBOXED);
    fake.caps_lock_refusal = Some(CapsLockRefusal::Refused(SANDBOXED.to_owned()));
    fake
}

#[test]
fn a_build_without_caps_lock_says_so_up_front_and_refuses_turning_it_on() {
    scenario(|| async {
        let engine = live_engine("caps-lock-unavailable", without_caps_lock());
        let state = app_state(&engine);
        assert_eq!(state.caps_lock_unavailable.as_deref(), Some(SANDBOXED));
        assert_eq!(state.caps_lock_problem, None);

        assert_eq!(turn_on(&engine).as_deref(), Some(SANDBOXED));
        assert!(!engine.settings().caps_lock_as_snappy_key);
        assert_eq!(held_as(&engine), None);
        assert_eq!(app_state(&engine).caps_lock_problem, None, "the reply says why");
    });
}

#[test]
fn a_build_without_caps_lock_treats_a_stored_setting_as_off_without_an_error() {
    scenario(|| async {
        let name = "caps-lock-unavailable-stored";
        let engine = live_engine(name, with_window(FLOATING));
        event(&engine, PlatformEvent::QuitRequested);
        drop(engine);
        save_setting(name, true);

        let engine = relaunched(name, without_caps_lock());
        assert!(engine.settings().caps_lock_as_snappy_key, "the setting is kept");
        assert_eq!(held_as(&engine), None);
        assert!(
            engine.status().is_none_or(|status| !status.is_error),
            "nothing to announce"
        );
        let state = app_state(&engine);
        assert_eq!(state.caps_lock_problem, None);
        assert_eq!(state.caps_lock_unavailable.as_deref(), Some(SANDBOXED));

        reply_to(
            &engine,
            Command::SetSnappyKey {
                modifiers: control_option(),
            },
        );
        assert_eq!(held_as(&engine), None, "nor when the key changes");
        assert!(engine.status().is_none_or(|status| !status.is_error));
    });
}

#[test]
fn where_caps_lock_works_the_platform_declares_nothing() {
    scenario(|| async {
        let engine = live_engine("caps-lock-available", with_window(FLOATING));
        assert_eq!(app_state(&engine).caps_lock_unavailable, None);
    });
}
