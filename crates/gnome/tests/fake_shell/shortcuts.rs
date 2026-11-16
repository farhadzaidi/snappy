//! Shortcut grabs and the drag key, as the extension is asked to hold them.

use std::sync::PoisonError;

use serde_json::{Value, json};
use snappy_core::keys::{KeyChord, Modifier};
use snappy_gnome::protocol::PROTOCOL_VERSION;
use snappy_platform::{HotKeys, Input};

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
        (1, one.clone()),
        (2, one.clone()),
        (3, super_a.clone()),
        (4, super_a.clone()),
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
