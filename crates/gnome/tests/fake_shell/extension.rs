//! The extension coming and going: asking to log in again, going live, getting back what
//! it held, and noting other tiling extensions.

use std::sync::PoisonError;

use serde_json::json;
use snappy_core::keys::{KeyChord, Modifier};
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::{
    HotKeys, Input, LiveControl, Platform, PlatformEvent, Remedy, Tray, TrayMenu, WindowId,
};

use crate::support::{Adapter, fake, record};

#[test]
fn an_installed_extension_gnome_hasnt_loaded_asks_to_log_in_again() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(None, None);
    let mut adapter = Adapter::start();
    let LiveControl::Unavailable { reason, remedy } = adapter.platform.live_control() else {
        panic!("the extension isn't answering");
    };
    assert!(reason.starts_with("One last step: log out of "), "{reason}");
    assert_eq!(remedy, Some(Remedy::LogOut));
    adapter.platform.apply_remedy(Remedy::LogOut).unwrap();
    assert_eq!(fake.state().log_outs, 1, "GNOME's log-out was asked for");

    fake.reset(None, Some(2.0));
    fake.extension_state_changed();
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    let LiveControl::Unavailable { reason, .. } = adapter.platform.live_control() else {
        panic!("the extension is off");
    };
    assert!(reason.contains("turned off"), "{reason}");
}

#[test]
fn the_extension_starting_makes_snappy_live_and_gets_back_what_it_held() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    assert_eq!(adapter.platform.live_control(), LiveControl::Available);
    adapter.platform.set_drag_modifier(Modifier::Shift);
    adapter.platform.set_press_monitoring(true);
    assert!(
        adapter
            .platform
            .register(&[(
                3,
                KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT)
            )])
            .is_empty()
    );
    adapter.platform.update(&TrayMenu::default());
    fake.state().requests.clear();

    fake.emit(json!({"type": "ready"}));
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    assert_eq!(adapter.platform.live_control(), LiveControl::Available);
    let monitoring = fake.requests("monitoring");
    assert_eq!(monitoring.last().unwrap()["press"], true);
    assert_eq!(
        monitoring.last().unwrap()["follow"],
        1,
        "Shift keeps dragged windows under the pointer"
    );
    let registered = fake.requests("register");
    assert_eq!(
        registered.last().unwrap()["keys"],
        json!([{"id": 3, "accelerator": "<Control><Alt><Super>0x0a"}])
    );
    assert_eq!(fake.requests("menu").len(), 1);

    fake.reset(Some(PROTOCOL_VERSION + 1), Some(1.0));
    fake.extension_state_changed();
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    let LiveControl::Unavailable { reason, remedy } = adapter.platform.live_control() else {
        panic!("another version answers");
    };
    assert!(reason.starts_with("Snappy was updated."), "{reason}");
    assert_eq!(remedy, Some(Remedy::LogOut));
}

#[test]
fn a_running_tiling_assistant_is_noted_and_its_changes_are_reported() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    assert!(adapter.platform.notes().is_empty());

    fake.state().running = vec!["tiling-assistant@ubuntu.com".into()];
    fake.extension_changed("tiling-assistant@ubuntu.com");
    assert_eq!(adapter.next(), PlatformEvent::NotesChanged);
    let notes = adapter.platform.notes();
    assert_eq!(notes.len(), 1);
    assert!(notes[0].contains("Enhanced Tiling"), "{notes:?}");

    fake.extension_changed("someone-else@example.com");
    assert!(adapter.settle(fake).is_empty(), "other extensions don't matter");
}

#[test]
fn shortcuts_registered_while_gnome_is_away_are_kept_and_grabbed_when_it_returns() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(None, None);
    let mut adapter = Adapter::start();
    let one = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    assert!(
        adapter.platform.register(&[(1, one.clone())]).is_empty(),
        "an unanswered request says nothing about the chord"
    );
    assert!(fake.requests("register").is_empty());

    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.emit(json!({"type": "ready"}));
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    let registered = fake.requests("register");
    assert_eq!(
        registered.last().unwrap()["keys"],
        json!([{"id": 1, "accelerator": "<Control><Alt><Super>0x0a"}])
    );
}

#[test]
fn a_shortcut_taken_while_gnome_is_away_is_let_go_when_it_returns() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(None, None);
    let mut adapter = Adapter::start();
    let one = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    assert!(adapter.platform.register(&[(1, one.clone())]).is_empty());

    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().taken = vec!["<Control><Alt><Super>0x0a".into()];
    fake.emit(json!({"type": "ready"}));
    assert_eq!(adapter.next(), PlatformEvent::LiveControlChanged);
    assert_eq!(fake.requests("register").len(), 1);
    assert!(
        !adapter.platform.is_available(&one),
        "the chord is no longer Snappy's, so it counts as taken"
    );
}

#[test]
fn windows_that_came_and_went_while_the_extension_was_away_are_reported() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().windows = vec![record(1, 100, 0.0), record(2, 200, 700.0)];
    let adapter = Adapter::start();
    assert!(
        adapter.settle(fake).is_empty(),
        "the first list is taken as it is"
    );

    fake.state().windows = vec![record(1, 100, 0.0), record(3, 300, 700.0)];
    fake.emit(json!({"type": "ready"}));
    let events = adapter.settle(fake);
    assert!(
        events.contains(&PlatformEvent::WindowDestroyed(WindowId(2))),
        "{events:?}"
    );
    assert!(
        events.contains(&PlatformEvent::AppTerminated { pid: 200 }),
        "{events:?}"
    );
    assert!(
        events.contains(&PlatformEvent::AppLaunched { pid: 300 }),
        "{events:?}"
    );
    assert!(
        events.contains(&PlatformEvent::WindowCreated { pid: 300 }),
        "{events:?}"
    );
    assert_eq!(events.last(), Some(&PlatformEvent::LiveControlChanged));
}
