//! Caps Lock held as the Snappy key: its presses, refusals, and giving it back.

use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::json;
use snappy_core::geometry::Point;
use snappy_core::keys::{KeyChord, Modifier, Mods, SnappyKey};
use snappy_gnome::protocol::{CALL_METHOD, INTERFACE, OBJECT_PATH, SHELL_BUS_NAME};
use snappy_linux::xkb::CapsLockKey;
use snappy_linux::{key_codes, xkb};
use snappy_platform::{CapsLock, CapsLockInput, CapsLockRefusal, HotKeyRequest, HotKeys, PlatformEvent};
use zbus::blocking::Connection;

use crate::support::{CAPS_LOCK, Shell, shell};

fn caps_lock(input: CapsLockInput) -> PlatformEvent {
    PlatformEvent::CapsLock(input)
}

/// Only Caps Lock's own events, in order.
fn caps_lock_events(events: &[PlatformEvent]) -> Vec<PlatformEvent> {
    events
        .iter()
        .filter(|event| matches!(event, PlatformEvent::CapsLock(_)))
        .cloned()
        .collect()
}

/// Taps Caps Lock alone, and returns what the adapter reported up to its release.
fn tap_caps_lock(shell: &Shell) -> Vec<PlatformEvent> {
    shell.key(CAPS_LOCK, true);
    std::thread::sleep(Duration::from_millis(60));
    shell.key(CAPS_LOCK, false);
    shell.wait_for("Caps Lock coming up", |event| {
        *event == caps_lock(CapsLockInput::Released)
    })
}

/// Snappy's option files in the session's XKB folder.
fn xkb_folder() -> std::path::PathBuf {
    snappy_linux::xkb::user_folder().unwrap()
}

#[test]
fn caps_lock_held_is_the_snappy_key_and_each_press_is_reported_without_its_keys() {
    let mut shell = shell!();
    let key = SnappyKey::DEFAULT;
    let right = KeyChord::snappy("ArrowRight", []);
    assert!(
        shell
            .platform
            .register(&[HotKeyRequest::new(11, right.combo(key))])
            .is_empty()
    );
    shell.platform.hold_caps_lock_as(key.modifiers()).unwrap();
    assert!(xkb_folder().join("symbols/snappy").exists());
    std::thread::sleep(Duration::from_millis(300));
    shell.drain();
    assert!(!shell.caps_lock_on());

    // Held with an arrow, Caps Lock is the Snappy key, and never turns on.
    shell.key(CAPS_LOCK, true);
    std::thread::sleep(Duration::from_millis(60));
    let arrow = key_codes::evdev_keycode("ArrowRight").unwrap();
    shell.key(arrow, true);
    shell.key(arrow, false);
    shell.key(CAPS_LOCK, false);
    let seen = shell.wait_for("Caps Lock coming up", |event| {
        *event == caps_lock(CapsLockInput::Released)
    });
    assert!(seen.contains(&PlatformEvent::HotKey(11)), "{seen:?}");
    assert_eq!(caps_lock_events(&seen)[0], caps_lock(CapsLockInput::Pressed));
    assert!(!shell.caps_lock_on(), "holding never turns Caps Lock on");

    // Alone, a press is only pressed and released.
    assert_eq!(
        caps_lock_events(&tap_caps_lock(&shell)),
        [
            caps_lock(CapsLockInput::Pressed),
            caps_lock(CapsLockInput::Released)
        ]
    );
    assert!(
        !shell.caps_lock_on(),
        "the adapter only reports; the engine decides"
    );

    // Another key, or a button, while it is down is reported as something else used.
    shell.key(CAPS_LOCK, true);
    std::thread::sleep(Duration::from_millis(60));
    let j = key_codes::evdev_keycode("KeyJ").unwrap();
    shell.key(j, true);
    shell.key(j, false);
    shell.key(CAPS_LOCK, false);
    let seen = shell.wait_for("Caps Lock coming up", |event| {
        *event == caps_lock(CapsLockInput::Released)
    });
    assert_eq!(
        caps_lock_events(&seen),
        [
            caps_lock(CapsLockInput::Pressed),
            caps_lock(CapsLockInput::OtherInput),
            caps_lock(CapsLockInput::Released)
        ]
    );
    shell.pointer(Point::new(1700.0, 900.0));
    shell.key(CAPS_LOCK, true);
    std::thread::sleep(Duration::from_millis(60));
    shell.button(true);
    shell.button(false);
    shell.key(CAPS_LOCK, false);
    let seen = shell.wait_for("Caps Lock coming up", |event| {
        *event == caps_lock(CapsLockInput::Released)
    });
    assert!(
        caps_lock_events(&seen).contains(&caps_lock(CapsLockInput::OtherInput)),
        "{seen:?}"
    );

    // The Snappy key's own modifiers tapped alone are not Caps Lock.
    for modifier in [Modifier::Ctrl, Modifier::Alt, Modifier::Meta] {
        shell.modifier(modifier, true);
    }
    for modifier in [Modifier::Meta, Modifier::Alt, Modifier::Ctrl] {
        shell.modifier(modifier, false);
    }
    assert!(caps_lock_events(&shell.drain()).is_empty());

    // Turning Caps Lock on and off is the adapter's, as is Shift + Caps Lock, the real one.
    shell.platform.toggle_caps_lock().unwrap();
    shell.wait_caps_lock(true);
    shell.platform.toggle_caps_lock().unwrap();
    shell.wait_caps_lock(false);
    shell.modifier(Modifier::Shift, true);
    shell.key(CAPS_LOCK, true);
    shell.key(CAPS_LOCK, false);
    shell.modifier(Modifier::Shift, false);
    shell.wait_caps_lock(true);
    assert!(caps_lock_events(&shell.drain()).is_empty());

    // Changing the Snappy key while Caps Lock is on keeps it on.
    let control_shift = Mods::from_iter([Modifier::Ctrl, Modifier::Shift]);
    shell.platform.hold_caps_lock_as(control_shift).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert!(shell.caps_lock_on());
    let symbols = std::fs::read_to_string(xkb_folder().join("symbols/snappy")).unwrap();
    assert!(symbols.contains("Control+Shift"), "{symbols}");

    // Restored, Caps Lock is a plain Caps Lock again and Snappy's files are gone.
    shell.platform.restore_caps_lock();
    std::thread::sleep(Duration::from_millis(300));
    assert!(!xkb_folder().join("symbols/snappy").exists());
    shell.drain();
    tap(&shell, CAPS_LOCK);
    shell.wait_caps_lock(false);
    tap(&shell, CAPS_LOCK);
    shell.wait_caps_lock(true);
    tap(&shell, CAPS_LOCK);
    shell.wait_caps_lock(false);
    assert!(caps_lock_events(&shell.drain()).is_empty());
}

#[test]
fn a_shortcut_completed_by_caps_lock_is_let_go_as_its_key_or_caps_lock_comes_up() {
    let mut shell = shell!();
    let key = SnappyKey::DEFAULT;
    let send_to_back = HotKeyRequest {
        reports_release: true,
        ..HotKeyRequest::new(9, KeyChord::snappy("KeyB", []).combo(key))
    };
    assert!(shell.platform.register(&[send_to_back]).is_empty());
    shell.platform.hold_caps_lock_as(key.modifiers()).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    shell.drain();
    let released = |event: &PlatformEvent| *event == PlatformEvent::HotKeyReleased(9);

    let key_b = key_codes::evdev_keycode("KeyB").unwrap();
    shell.key(CAPS_LOCK, true);
    std::thread::sleep(Duration::from_millis(60));
    shell.key(key_b, true);
    std::thread::sleep(Duration::from_millis(60));
    shell.key(key_b, false);
    let pressed = shell.wait_for("its release as B comes up", released);
    assert!(pressed.contains(&PlatformEvent::HotKey(9)), "{pressed:?}");
    shell.key(CAPS_LOCK, false);
    std::thread::sleep(Duration::from_millis(300));
    shell.drain();

    shell.key(CAPS_LOCK, true);
    std::thread::sleep(Duration::from_millis(60));
    shell.key(key_b, true);
    shell.wait_for("the shortcut", |event| *event == PlatformEvent::HotKey(9));
    shell.key(CAPS_LOCK, false);
    shell.wait_for("its release as Caps Lock comes up", released);
    shell.key(key_b, false);

    shell.platform.restore_caps_lock();
    shell.platform.unregister_all();
    std::thread::sleep(Duration::from_millis(300));
}

/// Sets one of GNOME's input source settings, and gives GNOME Shell time to build the keymap.
fn input_sources(key: &str, value: &str) {
    let status = Command::new("gsettings")
        .args(["set", "org.gnome.desktop.input-sources", key, value])
        .status()
        .unwrap();
    assert!(status.success());
    std::thread::sleep(Duration::from_millis(300));
}

/// What GNOME Shell's keymap, as it hands it to Wayland clients, makes of Caps Lock.
fn keymap_caps_lock() -> Option<CapsLockKey> {
    xkb::caps_lock_key(&snappy_linux::wayland::keymap().unwrap())
}

#[test]
fn a_caps_lock_the_users_keyboard_options_remap_is_refused_naming_the_option() {
    let mut shell = shell!();
    let options = |value: &str| input_sources("xkb-options", value);
    options("['caps:escape', 'compose:ralt']");
    let refused = shell.platform.hold_caps_lock_as(SnappyKey::DEFAULT.modifiers());
    options("[]");
    assert_eq!(
        refused.unwrap_err(),
        CapsLockRefusal::Refused(
            "Caps Lock is already remapped by the keyboard option caps:escape. Remove it to use Caps \
             Lock as the Snappy key."
                .to_owned()
        )
    );
    assert!(!xkb_folder().join("symbols/snappy").exists());

    // One added while Caps Lock is held wins, and the engine hears Caps Lock was lost.
    shell
        .platform
        .hold_caps_lock_as(SnappyKey::DEFAULT.modifiers())
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    options("['compose:ralt']");
    assert!(!shell.drain().contains(&PlatformEvent::CapsLockLost));
    options("['compose:ralt', 'caps:escape']");
    shell.wait_for("Caps Lock lost", |event| *event == PlatformEvent::CapsLockLost);
    assert!(!xkb_folder().join("symbols/snappy").exists());
    assert_eq!(keymap_caps_lock(), Some(CapsLockKey::Remapped), "theirs");
    options("[]");
}

#[test]
fn a_layout_that_makes_caps_lock_another_key_is_refused() {
    let mut shell = shell!();
    let key = SnappyKey::DEFAULT.modifiers();
    assert_eq!(keymap_caps_lock(), Some(CapsLockKey::Plain));

    input_sources("sources", "[('xkb', 'us+colemak')]");
    assert_eq!(keymap_caps_lock(), Some(CapsLockKey::Remapped));
    let refused = shell.platform.hold_caps_lock_as(key);
    input_sources("sources", "[('xkb', 'us')]");
    assert_eq!(
        refused.unwrap_err(),
        CapsLockRefusal::Refused(xkb::LAYOUT_REMAPS_CAPS_LOCK.to_owned())
    );
    assert!(!xkb_folder().join("symbols/snappy").exists());

    // Snappy's own definition in the keymap is no reason to refuse a new Snappy key.
    shell.platform.hold_caps_lock_as(key).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(keymap_caps_lock(), Some(CapsLockKey::Snappys));
    let control_shift = Mods::from_iter([Modifier::Ctrl, Modifier::Shift]);
    shell.platform.hold_caps_lock_as(control_shift).unwrap();
    shell.platform.restore_caps_lock();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(keymap_caps_lock(), Some(CapsLockKey::Plain));
}

#[test]
fn the_extension_gives_caps_lock_back_when_snappy_goes_away() {
    let shell = shell!();
    std::fs::create_dir_all(xkb_folder()).unwrap();
    snappy_linux::xkb::install(
        &xkb_folder(),
        SnappyKey::DEFAULT.modifiers(),
        snappy_linux::xkb::CapsLockSymbol::Nothing,
    )
    .unwrap();
    {
        let other = Connection::session().unwrap();
        let request = json!({
            "op": "capsLockHold", "modifiers": "<Control><Alt><Super>", "mask": 76, "keycodes": [44],
        });
        other
            .call_method(
                Some(SHELL_BUS_NAME),
                OBJECT_PATH,
                Some(INTERFACE),
                CALL_METHOD,
                &(request.to_string(),),
            )
            .unwrap();
        std::thread::sleep(Duration::from_millis(300));
        tap(&shell, CAPS_LOCK);
        std::thread::sleep(Duration::from_millis(200));
        assert!(!shell.caps_lock_on(), "Caps Lock is the Snappy key while held");
    }
    std::thread::sleep(Duration::from_millis(500));
    tap(&shell, CAPS_LOCK);
    shell.wait_caps_lock(true);
    tap(&shell, CAPS_LOCK);
    shell.wait_caps_lock(false);
    snappy_linux::xkb::uninstall(&xkb_folder()).unwrap();
}

#[test]
fn a_caps_lock_press_whose_modifiers_never_show_ends_on_its_own() {
    let mut shell = shell!();
    shell
        .platform
        .hold_caps_lock_as(SnappyKey::DEFAULT.modifiers())
        .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    // Snappy's files change under GNOME Shell to make Caps Lock set Control alone, and an
    // options change builds the keymap from them, so no sample ever sees the Snappy key's
    // modifiers, as with a keymap that lacks Snappy's option.
    snappy_linux::xkb::install(
        &xkb_folder(),
        Mods::from_iter([Modifier::Ctrl]),
        snappy_linux::xkb::CapsLockSymbol::Nothing,
    )
    .unwrap();
    input_sources("xkb-options", "['compose:ralt']");
    shell.drain();

    // A press whose release went unseen ends when Caps Lock goes down again.
    tap(&shell, CAPS_LOCK);
    std::thread::sleep(Duration::from_millis(100));
    let second = Instant::now();
    tap(&shell, CAPS_LOCK);
    assert_eq!(
        caps_lock_events(&shell.drain()),
        [
            caps_lock(CapsLockInput::Pressed),
            caps_lock(CapsLockInput::Released),
            caps_lock(CapsLockInput::Pressed)
        ]
    );
    // And one that stays open ends after its limit, too late to be a tap.
    shell.wait_for("the open press ending", |event| {
        *event == caps_lock(CapsLockInput::Released)
    });
    assert!(
        second.elapsed() >= Duration::from_millis(900),
        "{:?}",
        second.elapsed()
    );
    input_sources("xkb-options", "[]");
    shell.platform.restore_caps_lock();
}

/// Presses and releases a key at a hand's pace.
fn tap(shell: &Shell, keycode: u8) {
    shell.key(keycode, true);
    std::thread::sleep(Duration::from_millis(60));
    shell.key(keycode, false);
}
