//! The extension's top bar menu, and what happens when Snappy or the extension goes away.

use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::json;
use snappy_core::keys::KeyChord;
use snappy_gnome::protocol::{CALL_METHOD, EXTENSION_UUID, INTERFACE, OBJECT_PATH, SHELL_BUS_NAME};
use snappy_platform::{HotKeyRequest, HotKeys, LiveControl, Platform, PlatformEvent, Tray, TrayMenu};
use zbus::blocking::Connection;

use crate::support::{WAIT, shell};

#[test]
fn the_menu_appears_in_the_top_bar_and_its_choices_come_back() {
    let mut shell = shell!();
    shell.platform.update(&TrayMenu {
        live: true,
        tooltip: "Snappy".into(),
        ..TrayMenu::default()
    });
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(shell.helper(json!({"op": "indicator"})), true);
    shell.drain();
    assert_eq!(
        shell.helper(json!({"op": "activateMenuItem", "title": "Open Snappy…"})),
        true
    );
    shell.wait_for("the menu choice", |event| {
        *event == PlatformEvent::OpenAppRequested
    });
}

#[test]
fn the_extension_lets_go_of_shortcuts_when_snappy_goes_away() {
    let mut shell = shell!();
    let combo = KeyChord::snappy("Digit3", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    let accelerator = snappy_gnome::reference::accelerator(&combo).unwrap();
    {
        let other = Connection::session().unwrap();
        let request = json!({"op": "register", "keys": [{"id": 1, "accelerator": accelerator}]});
        other
            .call_method(
                Some(SHELL_BUS_NAME),
                OBJECT_PATH,
                Some(INTERFACE),
                CALL_METHOD,
                &(request.to_string(),),
            )
            .unwrap();
        assert!(
            !shell.platform.is_available(&combo),
            "the other connection holds it"
        );
    }
    let deadline = Instant::now() + WAIT;
    while !shell.platform.is_available(&combo) {
        assert!(Instant::now() < deadline, "the shortcut stayed grabbed");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn turning_the_extension_off_and_on_reports_why_and_restores_what_it_held() {
    let mut shell = shell!();
    let chord = KeyChord::snappy("Digit4", []);
    assert!(
        shell
            .platform
            .register(&[HotKeyRequest::new(
                9,
                chord.combo(snappy_core::keys::SnappyKey::DEFAULT)
            )])
            .is_empty()
    );
    shell.drain();

    let toggle = |command: &str| {
        let status = Command::new("gnome-extensions")
            .args([command, EXTENSION_UUID])
            .status()
            .unwrap();
        assert!(status.success(), "gnome-extensions {command}");
    };
    toggle("disable");
    shell.wait_for("live control changing", |event| {
        *event == PlatformEvent::LiveControlChanged
    });
    let reason = shell.platform.live_control();
    assert!(
        matches!(&reason, LiveControl::Unavailable { reason: message, .. } if message.contains("turned off")),
        "{reason:?}"
    );

    toggle("enable");
    let deadline = Instant::now() + WAIT;
    while !shell.platform.live_control().is_available() {
        assert!(Instant::now() < deadline, "the extension never answered again");
        std::thread::sleep(Duration::from_millis(50));
    }
    shell.drain();
    shell.press_chord(&chord);
    shell.wait_for("the shortcut, grabbed again", |event| {
        *event == PlatformEvent::HotKey(9)
    });
}
