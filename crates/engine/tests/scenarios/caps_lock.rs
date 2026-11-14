//! Caps Lock as the Snappy key.

use snappy_core::keys::{Modifier, Mods, SnappyKey};
use snappy_engine::Engine;
use snappy_fake::FakePlatform;
use snappy_ipc::{Command, ToSettings};
use snappy_platform::{CapsLockInput, CapsLockUndo, PlatformEvent};

use crate::support::{
    FLOATING, app_state, command, event, hotkey_id, live_engine, logged, paths_kept, relaunched, scenario,
    snappy, with_window,
};

const REMAPPED: &str = "Caps Lock is already remapped.";

fn control_option() -> Mods {
    Mods::from_iter([Modifier::Ctrl, Modifier::Alt])
}

/// Send a command and return the daemon's reply to it.
fn reply_to(engine: &Engine<FakePlatform>, sent: Command) -> Option<String> {
    engine.take_outbox();
    command(engine, sent);
    engine
        .take_outbox()
        .into_iter()
        .find_map(|message| match message {
            ToSettings::Reply { error, .. } => Some(error),
            _ => None,
        })
        .unwrap()
}

fn turn_on(engine: &Engine<FakePlatform>) -> Option<String> {
    reply_to(engine, Command::SetCapsLockAsSnappyKey { enabled: true })
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
        fake.caps_lock_refusal = Some(REMAPPED.to_owned());
        let engine = live_engine(name, fake);

        assert_eq!(turn_on(&engine).as_deref(), Some(REMAPPED));
        assert!(!engine.settings().caps_lock_as_snappy_key);
        assert_eq!(held_as(&engine), None);
        assert!(!undo_kept(name));
        assert_eq!(app_state(&engine).caps_lock_problem, None, "the reply says why");
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

        let tapped = logged(name, "capsLockTapped");
        assert_eq!(tapped.len(), 1);
        let released = logged(name, "capsLockReleased");
        assert_eq!(released.len(), 3);
        assert_eq!(released[0]["otherInput"], true);
        assert!(released.iter().all(|record| record.get("key").is_none()));
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
        system.caps_lock_undo = CapsLockUndo("second run".to_owned());

        let engine = relaunched(name, system);
        assert_eq!(
            engine.with_platform(|p| p.caps_lock_undone.clone()),
            [CapsLockUndo("fake".to_owned())]
        );
        assert_eq!(held_as(&engine), Some(SnappyKey::DEFAULT.modifiers()));
        let kept: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(paths_kept(name).caps_lock()).unwrap()).unwrap();
        assert_eq!(kept["undo"], "second run");
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
        system.caps_lock_refusal = Some(REMAPPED.to_owned());

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
