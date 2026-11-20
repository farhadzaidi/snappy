//! Grabbing shortcuts, and which chords the window manager leaves free.

use std::time::Duration;

use snappy_core::keys::{Combo, KeyChord, Os};
use snappy_core::settings::{Settings, Workspace};
use snappy_platform::{HotKeyRequest, HotKeys, PlatformEvent};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};

use crate::support::{Desktop, SNAPPY_KEY, desktop, key};

#[test]
fn shortcuts_fire_with_and_without_num_lock() {
    let mut desktop = desktop!();
    let chord = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
    assert!(
        desktop
            .platform
            .register(&[HotKeyRequest::new(7, chord)])
            .is_empty()
    );

    let snappy_one = [key::CONTROL_L, key::ALT_L, key::SUPER_L, key::DIGIT_1];
    desktop.tap(&snappy_one);
    assert_eq!(
        desktop.expect(|event| matches!(event, PlatformEvent::HotKey(_))),
        Some(PlatformEvent::HotKey(7))
    );

    desktop.tap(&[key::NUM_LOCK]);
    desktop.tap(&snappy_one);
    let fired = desktop.expect(|event| matches!(event, PlatformEvent::HotKey(_)));
    desktop.tap(&[key::NUM_LOCK]);
    assert_eq!(fired, Some(PlatformEvent::HotKey(7)));

    desktop.platform.unregister_all();
    desktop.tap(&snappy_one);
    assert!(
        !desktop
            .drain()
            .iter()
            .any(|event| matches!(event, PlatformEvent::HotKey(_)))
    );
}

/// Longer than the X server's delay before a held key repeats.
const PAST_REPEAT_DELAY: Duration = Duration::from_millis(900);

/// The shortcut presses and releases among the events that arrive next.
fn shortcut_events(desktop: &Desktop) -> Vec<PlatformEvent> {
    desktop
        .drain()
        .into_iter()
        .filter(|event| matches!(event, PlatformEvent::HotKey(_) | PlatformEvent::HotKeyReleased(_)))
        .collect()
}

#[test]
fn a_shortcut_that_reports_its_release_is_let_go_once_whatever_its_key_repeats() {
    let mut desktop = desktop!();
    let send_to_back = HotKeyRequest {
        reports_release: true,
        ..HotKeyRequest::new(9, Combo::new("KeyB", SNAPPY_KEY))
    };
    let focus = HotKeyRequest::new(10, Combo::new("KeyJ", SNAPPY_KEY));
    assert!(desktop.platform.register(&[send_to_back, focus]).is_empty());
    desktop.drain();
    let snappy_key = [key::CONTROL_L, key::ALT_L, key::SUPER_L];

    desktop.tap(&[key::CONTROL_L, key::ALT_L, key::SUPER_L, key::KEY_B]);
    assert_eq!(
        shortcut_events(&desktop),
        [PlatformEvent::HotKey(9), PlatformEvent::HotKeyReleased(9)],
        "a quick press"
    );

    for modifier in snappy_key {
        desktop.key(modifier, true);
    }
    desktop.key(key::KEY_B, true);
    std::thread::sleep(PAST_REPEAT_DELAY);
    desktop.key(key::KEY_B, false);
    for modifier in snappy_key.into_iter().rev() {
        desktop.key(modifier, false);
    }
    assert_eq!(
        shortcut_events(&desktop),
        [PlatformEvent::HotKey(9), PlatformEvent::HotKeyReleased(9)],
        "a hold"
    );

    desktop.tap(&[key::CONTROL_L, key::ALT_L, key::SUPER_L, key::KEY_J]);
    assert_eq!(
        shortcut_events(&desktop),
        [PlatformEvent::HotKey(10)],
        "only shortcuts that ask report their release"
    );
    desktop.platform.unregister_all();
}

#[test]
fn chords_grabbed_elsewhere_are_unavailable() {
    let mut desktop = desktop!();
    let taken = Combo::new("KeyK", SNAPPY_KEY);
    let free = Combo::new("KeyJ", SNAPPY_KEY);
    assert!(desktop.platform.is_available(&taken));

    // Another client takes Ctrl+Alt+Super+K.
    let modifiers = ModMask::CONTROL | ModMask::M1 | ModMask::M4;
    desktop
        .client
        .grab_key(
            false,
            desktop.root,
            modifiers,
            45,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .unwrap()
        .check()
        .unwrap();

    assert!(!desktop.platform.is_available(&taken));
    assert!(desktop.platform.is_available(&free));
    assert_eq!(
        desktop.platform.register(&[
            HotKeyRequest::new(1, taken.clone()),
            HotKeyRequest::new(2, free.clone())
        ]),
        [1]
    );
    // Registered chords count as available to Snappy itself, and probing keeps them.
    assert!(desktop.platform.is_available(&free));
    assert!(
        desktop
            .platform
            .register(&[HotKeyRequest::new(2, free)])
            .is_empty()
    );
    desktop.platform.unregister_all();
    desktop.client.ungrab_key(45, desktop.root, modifiers).unwrap();
    desktop.client.flush().unwrap();
    assert!(desktop.platform.is_available(&taken));
}

/// Every default chord, moving between zones, to the nearest empty zone, focus, sending to
/// the back, the selection, the workspace steps, and switching to workspaces 1 to 9, is free
/// under the session's window manager, Xfwm4 with its default key bindings.
#[test]
fn the_default_shortcuts_are_free() {
    let mut desktop = desktop!();
    let settings = Settings::defaults(Os::Linux);
    let mut chords: Vec<KeyChord> = settings
        .navigation
        .chords()
        .map(|(_, chord)| chord.clone())
        .collect();
    chords.extend(settings.workspaces.steps().map(|(_, chord)| chord.clone()));
    chords.extend((1..=9).filter_map(|n| Workspace::numbered(format!("workspace-{n}"), n).switch_shortcut));
    assert_eq!(chords.len(), 5 + 7 + 4 + 9);
    for code in ["Backslash", "KeyB"] {
        let default = KeyChord::snappy(code, []).combo(snappy_core::keys::SnappyKey::DEFAULT);
        assert!(
            chords
                .iter()
                .any(|chord| chord.combo(snappy_core::keys::SnappyKey::DEFAULT) == default),
            "{code}"
        );
    }
    let taken: Vec<String> = chords
        .iter()
        .filter(|chord| {
            !desktop
                .platform
                .is_available(&chord.combo(snappy_core::keys::SnappyKey::DEFAULT))
        })
        .map(|chord| chord.long_label(Os::Linux))
        .collect();
    assert!(taken.is_empty(), "the window manager holds {taken:?}");
}
