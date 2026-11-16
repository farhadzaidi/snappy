//! Caps Lock held as the Snappy key: Snappy's XKB option, refusals, presses, and undoing
//! a crashed run.

use std::sync::PoisonError;

use serde_json::{Value, json};
use snappy_core::keys::{Modifier, Mods};
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::{CapsLock, CapsLockInput, CapsLockRefusal, CapsLockUndo, PlatformEvent};

use crate::support::{Adapter, fake};

/// Snappy's XKB folder in the fake's home.
fn xkb_folder() -> std::path::PathBuf {
    snappy_linux::xkb::user_folder().unwrap()
}

#[test]
fn holding_caps_lock_writes_snappys_option_and_asks_the_extension_with_the_snappy_key() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().keyboard_options = "compose:ralt".into();
    let mut adapter = Adapter::start();
    let control_shift = Mods::from_iter([Modifier::Ctrl, Modifier::Shift]);

    assert_eq!(
        adapter.platform.hold_caps_lock_as(control_shift),
        Ok(CapsLockUndo::default())
    );
    let symbols = std::fs::read_to_string(xkb_folder().join("symbols/snappy")).unwrap();
    assert!(
        symbols.contains("SetMods(modifiers = Control+Shift)"),
        "{symbols}"
    );
    assert!(xkb_folder().join("rules/evdev").exists());
    let hold = fake.requests("capsLockHold");
    assert_eq!(hold.len(), 1);
    assert_eq!(hold[0]["modifiers"], "<Control><Shift>");
    assert_eq!(hold[0]["mask"], 4 | 1);
    let keycodes: Vec<u64> = hold[0]["keycodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert!(
        keycodes.contains(&44) && !keycodes.contains(&66),
        "J but not Caps Lock"
    );

    fake.state().requests.clear();
    fake.emit(json!({"type": "ready"}));
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    assert_eq!(
        fake.requests("capsLockHold"),
        hold,
        "a restarted extension holds it again"
    );

    adapter.platform.toggle_caps_lock().unwrap();
    assert_eq!(fake.requests("capsLockToggle").len(), 1);

    adapter.platform.restore_caps_lock();
    assert_eq!(fake.requests("capsLockRestore").len(), 1);
    assert!(!xkb_folder().join("symbols/snappy").exists());
    assert!(!xkb_folder().join("rules/evdev").exists());
    fake.state().requests.clear();
    fake.emit(json!({"type": "ready"}));
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    assert!(fake.requests("capsLockHold").is_empty(), "nothing to hold again");
}

#[test]
fn a_caps_lock_option_of_the_users_own_refuses_and_leaves_nothing_behind() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().keyboard_options = "ctrl:nocaps,caps:escape".into();
    let mut adapter = Adapter::start();

    let refused = adapter
        .platform
        .hold_caps_lock_as(snappy_core::keys::SnappyKey::DEFAULT.modifiers());
    assert_eq!(
        refused,
        Err(CapsLockRefusal::Refused(
            "Caps Lock is already remapped by the keyboard options ctrl:nocaps and caps:escape. \
             Remove them to use Caps Lock as the Snappy key."
                .to_owned()
        ))
    );
    assert!(fake.requests("capsLockHold").is_empty());
    assert!(!xkb_folder().join("symbols/snappy").exists());
}

#[test]
fn caps_lock_presses_arrive_as_engine_events_without_their_keys() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let adapter = Adapter::start();
    for phase in ["pressed", "otherInput", "released"] {
        fake.emit(json!({"type": "capsLock", "phase": phase}));
    }
    assert_eq!(
        adapter.settle(fake),
        [
            PlatformEvent::CapsLock(CapsLockInput::Pressed),
            PlatformEvent::CapsLock(CapsLockInput::OtherInput),
            PlatformEvent::CapsLock(CapsLockInput::Released),
        ]
    );
}

#[test]
fn a_caps_lock_option_added_while_held_wins_and_caps_lock_is_reported_lost() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    adapter
        .platform
        .hold_caps_lock_as(snappy_core::keys::SnappyKey::DEFAULT.modifiers())
        .unwrap();

    fake.emit(json!({"type": "keyboardOptions", "options": "compose:ralt"}));
    assert!(
        adapter.settle(fake).is_empty(),
        "options that leave Caps Lock alone"
    );
    assert!(fake.requests("capsLockRestore").is_empty());

    fake.emit(json!({"type": "keyboardOptions", "options": "compose:ralt,caps:escape"}));
    assert_eq!(adapter.settle(fake), [PlatformEvent::CapsLockLost]);
    assert_eq!(fake.requests("capsLockRestore").len(), 1);
    assert!(!xkb_folder().join("symbols/snappy").exists());

    fake.emit(json!({"type": "keyboardOptions", "options": "caps:escape,ctrl:nocaps"}));
    assert!(adapter.settle(fake).is_empty(), "nothing held to lose");
}

#[test]
fn undoing_a_crashed_runs_caps_lock_removes_snappys_option_files() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    snappy_linux::xkb::install(
        &xkb_folder(),
        snappy_core::keys::SnappyKey::DEFAULT.modifiers(),
        snappy_linux::xkb::CapsLockSymbol::Nothing,
    )
    .unwrap();
    let mut adapter = Adapter::start();
    adapter.platform.undo_caps_lock(&CapsLockUndo::default());
    assert!(!xkb_folder().join("symbols/snappy").exists());
    assert_eq!(fake.requests("capsLockRestore").len(), 1);
}
