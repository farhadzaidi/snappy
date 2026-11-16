//! Registering shortcuts, which chords GNOME leaves free, and presses arriving as hotkeys.

use snappy_core::keys::{KeyChord, Modifier, Os};
use snappy_core::settings::{Settings, Workspace};
use snappy_platform::{HotKeys, PlatformEvent};

use crate::support::shell;

/// Every default chord, moving between zones, to the nearest empty zone, focus and the
/// selection, the workspace steps, and switching to workspaces 1 to 9, is free in GNOME.
#[test]
fn the_default_shortcuts_are_free() {
    let mut shell = shell!();
    let settings = Settings::defaults(Os::Linux);
    let mut chords: Vec<KeyChord> = settings
        .navigation
        .chords()
        .map(|(_, chord)| chord.clone())
        .collect();
    chords.extend(settings.workspaces.steps().map(|(_, chord)| chord.clone()));
    chords.extend((1..=9).filter_map(|n| Workspace::numbered(format!("workspace-{n}"), n).switch_shortcut));
    assert_eq!(chords.len(), 5 + 6 + 4 + 9);
    let clear_selection = KeyChord::snappy("Backslash", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    assert!(
        chords
            .iter()
            .any(|chord| chord.combo(snappy_core::keys::SnappyKey::DEFAULT) == clear_selection)
    );
    let taken: Vec<String> = chords
        .iter()
        .filter(|chord| {
            !shell
                .platform
                .is_available(&chord.combo(snappy_core::keys::SnappyKey::DEFAULT))
        })
        .map(|chord| chord.long_label(Os::Linux))
        .collect();
    assert!(taken.is_empty(), "GNOME holds {taken:?}");
}

#[test]
fn chords_gnome_binds_are_reported_taken_and_snappys_own_are_free() {
    let mut shell = shell!();
    let super_a = KeyChord::new("KeyA", [Modifier::Meta]).combo(snappy_core::keys::SnappyKey::DEFAULT);
    let ctrl_super_one = KeyChord::new("Digit1", [Modifier::Ctrl, Modifier::Meta])
        .combo(snappy_core::keys::SnappyKey::DEFAULT);
    assert!(
        !shell.platform.is_available(&super_a),
        "Super+A shows the app grid"
    );
    assert!(
        !shell.platform.is_available(&ctrl_super_one),
        "Super+Ctrl+1 opens a new window"
    );
    let own = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    assert!(shell.platform.register(&[(4, own.clone())]).is_empty());
    assert!(shell.platform.is_available(&own));
    assert_eq!(
        shell.platform.register(&[(5, super_a)]),
        vec![5],
        "a chord GNOME binds is refused"
    );
}

#[test]
fn a_shortcut_press_arrives_as_its_hotkey_once() {
    let mut shell = shell!();
    let chord = KeyChord::snappy("Digit2", []);
    assert!(
        shell
            .platform
            .register(&[(7, chord.combo(snappy_core::keys::SnappyKey::DEFAULT))])
            .is_empty()
    );
    shell.drain();
    shell.press_chord(&chord);
    shell.wait_for("the shortcut", |event| *event == PlatformEvent::HotKey(7));
    let more: Vec<PlatformEvent> = shell
        .drain()
        .into_iter()
        .filter(|event| matches!(event, PlatformEvent::HotKey(_)))
        .collect();
    assert!(more.is_empty(), "one press is one shortcut: {more:?}");

    shell.platform.unregister_all();
    shell.press_chord(&chord);
    assert!(
        !shell.drain().contains(&PlatformEvent::HotKey(7)),
        "released shortcuts do nothing"
    );
}
