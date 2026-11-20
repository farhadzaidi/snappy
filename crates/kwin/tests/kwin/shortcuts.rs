//! Registering Snappy's shortcuts with KDE's shortcut service.

use snappy_core::keys::{KeyChord, Modifier, Os};
use snappy_core::settings::Settings;
use snappy_platform::{HotKeyRequest, HotKeys, PlatformEvent};

use crate::input::input;
use crate::support::kwin;

#[test]
fn a_registered_shortcut_reports_its_id_and_kwins_own_chords_are_taken() {
    let mut kwin = kwin!();
    let chord = KeyChord::snappy("Digit1", []);
    let refused = kwin.platform.register(&[HotKeyRequest::new(
        41,
        chord.combo(snappy_core::keys::SnappyKey::DEFAULT),
    )]);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(
        kwin.platform
            .is_available(&chord.combo(snappy_core::keys::SnappyKey::DEFAULT)),
        "Snappy's own shortcut counts as free"
    );
    kwin.drain();

    let mut input = input();
    input.press_chord(&chord);
    assert_eq!(
        kwin.next(|event| matches!(event, PlatformEvent::HotKey(_))),
        PlatformEvent::HotKey(41)
    );

    // KWin closes windows with Alt+F4.
    let close = KeyChord::new("F4", [Modifier::Alt]);
    assert!(
        !kwin
            .platform
            .is_available(&close.combo(snappy_core::keys::SnappyKey::DEFAULT))
    );
    assert_eq!(
        kwin.platform.register(&[HotKeyRequest::new(
            42,
            close.combo(snappy_core::keys::SnappyKey::DEFAULT)
        )]),
        vec![42]
    );
    kwin.platform.unregister_all();
}

#[test]
fn a_shortcut_that_reports_its_release_is_let_go_once_whatever_its_key_repeats() {
    let mut kwin = kwin!();
    let chord = KeyChord::snappy("KeyB", []);
    let combo = chord.combo(snappy_core::keys::SnappyKey::DEFAULT);
    let send_to_back = HotKeyRequest {
        reports_release: true,
        ..HotKeyRequest::new(9, combo.clone())
    };
    assert!(kwin.platform.register(&[send_to_back]).is_empty());
    kwin.drain();
    let shortcut =
        |event: &PlatformEvent| matches!(event, PlatformEvent::HotKey(_) | PlatformEvent::HotKeyReleased(_));

    let mut input = input();
    input.press_chord(&chord);
    assert_eq!(kwin.next(shortcut), PlatformEvent::HotKey(9));
    assert_eq!(kwin.next(shortcut), PlatformEvent::HotKeyReleased(9));

    let modifiers: Vec<Modifier> = combo.mods.iter().collect();
    for modifier in &modifiers {
        input.modifier(*modifier, true);
    }
    let key_b = snappy_linux::key_codes::evdev_keycode("KeyB").unwrap();
    input.key(key_b, true);
    // Past KDE's delay before a held key repeats.
    std::thread::sleep(std::time::Duration::from_millis(900));
    input.key(key_b, false);
    for modifier in modifiers.iter().rev() {
        input.modifier(*modifier, false);
    }
    assert_eq!(kwin.next(shortcut), PlatformEvent::HotKey(9));
    assert_eq!(kwin.next(shortcut), PlatformEvent::HotKeyReleased(9));
    drop(input);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert!(
        !std::iter::from_fn(|| kwin.try_next()).any(|event| shortcut(&event)),
        "one press and one release"
    );
    kwin.platform.unregister_all();
}

#[test]
fn letting_go_of_the_snappy_key_before_the_key_reports_the_release_and_the_next_press() {
    let mut kwin = kwin!();
    let chord = KeyChord::snappy("KeyB", []);
    let combo = chord.combo(snappy_core::keys::SnappyKey::DEFAULT);
    let send_to_back = HotKeyRequest {
        reports_release: true,
        ..HotKeyRequest::new(9, combo.clone())
    };
    assert!(kwin.platform.register(&[send_to_back]).is_empty());
    kwin.drain();
    let shortcut =
        |event: &PlatformEvent| matches!(event, PlatformEvent::HotKey(_) | PlatformEvent::HotKeyReleased(_));

    let mut input = input();
    let modifiers: Vec<Modifier> = combo.mods.iter().collect();
    for modifier in &modifiers {
        input.modifier(*modifier, true);
    }
    let key_b = snappy_linux::key_codes::evdev_keycode("KeyB").unwrap();
    input.key(key_b, true);
    std::thread::sleep(std::time::Duration::from_millis(100));
    for modifier in modifiers.iter().rev() {
        input.modifier(*modifier, false);
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    input.key(key_b, false);
    assert_eq!(kwin.next(shortcut), PlatformEvent::HotKey(9));
    assert_eq!(kwin.next(shortcut), PlatformEvent::HotKeyReleased(9));

    input.press_chord(&chord);
    assert_eq!(
        kwin.next(shortcut),
        PlatformEvent::HotKey(9),
        "the next press isn't taken for a repeat"
    );
    assert_eq!(kwin.next(shortcut), PlatformEvent::HotKeyReleased(9));
    kwin.platform.unregister_all();
}

#[test]
fn the_default_shortcuts_are_free() {
    let mut kwin = kwin!();
    let defaults = Settings::defaults(Os::Linux);
    let bound = snappy_core::shortcuts::bound(&defaults);
    let wanted: Vec<_> = (0u32..)
        .zip(&bound)
        .map(|(id, shortcut)| HotKeyRequest::new(id, shortcut.chord.combo(defaults.snappy_key)))
        .collect();
    let refused = kwin.platform.register(&wanted);
    let taken: Vec<&str> = refused
        .iter()
        .map(|&id| bound[id as usize].title.as_str())
        .collect();
    assert!(taken.is_empty(), "KWin already holds {taken:?}");
    kwin.platform.unregister_all();
}

#[test]
fn snappys_own_shortcuts_never_count_as_taken() {
    let mut kwin = kwin!();
    let one = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    let two = KeyChord::snappy("Digit2", []).combo(snappy_core::keys::SnappyKey::DEFAULT);

    // A run that crashed leaves its actions with KDE's shortcut service.
    let bus = zbus::blocking::Connection::session().unwrap();
    let left_behind = vec!["snappy", "shortcut-7", "Snappy", "Snappy shortcut 7"];
    let key = snappy_kwin::reference::key_code(&one).unwrap();
    bus.call_method(
        Some("org.kde.kglobalaccel"),
        "/kglobalaccel",
        Some("org.kde.KGlobalAccel"),
        "doRegister",
        &(left_behind.clone(),),
    )
    .unwrap();
    bus.call_method(
        Some("org.kde.kglobalaccel"),
        "/kglobalaccel",
        Some("org.kde.KGlobalAccel"),
        "setShortcutKeys",
        &(left_behind, vec![(vec![key],)], 6u32),
    )
    .unwrap();
    assert!(
        kwin.platform.is_available(&one),
        "a key Snappy itself holds is free"
    );

    assert_eq!(
        kwin.platform.register(&[
            HotKeyRequest::new(1, one.clone()),
            HotKeyRequest::new(2, two.clone())
        ]),
        Vec::<u32>::new()
    );
    // The engine registers everything again whenever shortcuts or displays change.
    assert_eq!(
        kwin.platform.register(&[
            HotKeyRequest::new(1, one.clone()),
            HotKeyRequest::new(2, two.clone())
        ]),
        Vec::<u32>::new()
    );
    // Shortcuts trade keys when the user swaps them.
    assert_eq!(
        kwin.platform.register(&[
            HotKeyRequest::new(1, two.clone()),
            HotKeyRequest::new(2, one.clone())
        ]),
        Vec::<u32>::new()
    );
    assert!(kwin.platform.is_available(&one) && kwin.platform.is_available(&two));
    kwin.drain();

    let mut input = input();
    input.press_chord(&KeyChord::snappy("Digit1", []));
    assert_eq!(
        kwin.next(|event| matches!(event, PlatformEvent::HotKey(_))),
        PlatformEvent::HotKey(2)
    );
    drop(input);
    kwin.platform.unregister_all();
}
