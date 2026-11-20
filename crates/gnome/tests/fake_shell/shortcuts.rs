//! Shortcut grabs and the drag key, as the extension is asked to hold them.

use std::sync::PoisonError;

use serde_json::{Value, json};
use snappy_core::keys::{KeyChord, Modifier};
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::{HotKeyRequest, HotKeys, Input};

use crate::support::{Adapter, fake};

#[test]
fn shortcuts_sharing_an_accelerator_share_a_grab_and_refusals_name_every_id() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    fake.state().taken = vec!["<Super>0x26".into()];
    let mut adapter = Adapter::start();
    let one = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    let super_a = KeyChord::new("KeyA", [Modifier::Meta]).combo(snappy_core::keys::SnappyKey::DEFAULT);
    let refused = adapter.platform.register(&[
        HotKeyRequest::new(1, one.clone()),
        HotKeyRequest::new(2, one.clone()),
        HotKeyRequest::new(3, super_a.clone()),
        HotKeyRequest::new(4, super_a.clone()),
    ]);
    assert_eq!(refused, vec![3, 4]);
    let keys = fake.requests("register").last().unwrap()["keys"].clone();
    assert_eq!(
        keys.as_array().unwrap().len(),
        2,
        "one grab per accelerator: {keys}"
    );

    fake.state().requests.clear();
    assert!(
        adapter.platform.is_available(&one),
        "Snappy's own shortcut is free to it"
    );
    assert!(
        fake.requests("isFree").is_empty(),
        "no probe for its own shortcut"
    );
    assert!(!adapter.platform.is_available(&super_a));
}

#[test]
fn a_shortcut_that_reports_its_release_sends_the_modifiers_that_let_it_go() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    let key = snappy_core::keys::SnappyKey::DEFAULT;
    let send_to_back = HotKeyRequest {
        reports_release: true,
        ..HotKeyRequest::new(9, KeyChord::snappy("KeyB", []).combo(key))
    };
    let focus = HotKeyRequest::new(10, KeyChord::snappy("KeyJ", []).combo(key));
    assert!(adapter.platform.register(&[send_to_back, focus]).is_empty());
    let keys = fake.requests("register").last().unwrap()["keys"].clone();
    let control_alt_super = 4 | 8 | 64;
    assert_eq!(
        keys,
        json!([
            {
                "id": 9,
                "accelerator": "<Control><Alt><Super>0x38",
                "release": {"keycode": 0x38, "mask": control_alt_super},
            },
            {"id": 10, "accelerator": "<Control><Alt><Super>0x2c"},
        ])
    );
}

#[test]
fn changing_the_drag_key_tells_the_extension_once() {
    let fake = fake!();
    let _turn = fake.turn.lock().unwrap_or_else(PoisonError::into_inner);
    fake.reset(Some(PROTOCOL_VERSION), Some(1.0));
    let mut adapter = Adapter::start();
    adapter.platform.set_press_monitoring(true);
    fake.state().requests.clear();
    adapter.platform.set_drag_modifier(Modifier::Ctrl);
    adapter.platform.set_drag_modifier(Modifier::Ctrl);
    let follows: Vec<Value> = fake
        .requests("monitoring")
        .iter()
        .map(|request| request["follow"].clone())
        .collect();
    assert_eq!(follows, vec![json!(4)], "Ctrl's mask, sent once");
}
