//! Registering Snappy's shortcuts with KDE's shortcut service.

use snappy_core::keys::{KeyChord, Modifier, Os};
use snappy_core::settings::Settings;
use snappy_platform::{HotKeys, PlatformEvent};

use crate::input::input;
use crate::support::kwin;

#[test]
fn a_registered_shortcut_reports_its_id_and_kwins_own_chords_are_taken() {
    let mut kwin = kwin!();
    let chord = KeyChord::snappy("Digit1", []);
    let refused = kwin
        .platform
        .register(&[(41, chord.combo(snappy_core::keys::SnappyKey::DEFAULT))]);
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
        kwin.platform
            .register(&[(42, close.combo(snappy_core::keys::SnappyKey::DEFAULT))]),
        vec![42]
    );
    kwin.platform.unregister_all();
}

#[test]
fn the_default_shortcuts_are_free() {
    let mut kwin = kwin!();
    let defaults = Settings::defaults(Os::Linux);
    let bound = snappy_core::shortcuts::bound(&defaults);
    let wanted: Vec<_> = (0u32..)
        .zip(&bound)
        .map(|(id, shortcut)| (id, shortcut.chord.combo(defaults.snappy_key)))
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
        kwin.platform.register(&[(1, one.clone()), (2, two.clone())]),
        Vec::<u32>::new()
    );
    // The engine registers everything again whenever shortcuts or displays change.
    assert_eq!(
        kwin.platform.register(&[(1, one.clone()), (2, two.clone())]),
        Vec::<u32>::new()
    );
    // Shortcuts trade keys when the user swaps them.
    assert_eq!(
        kwin.platform.register(&[(1, two.clone()), (2, one.clone())]),
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
