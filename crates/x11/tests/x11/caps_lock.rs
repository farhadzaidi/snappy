//! Caps Lock held as the Snappy key: its presses, taps, and giving it back.

use snappy_core::keys::{Combo, Mods};
use snappy_platform::{CapsLock, CapsLockInput, CapsLockRefusal, HotKeys, PlatformEvent};
use x11rb::protocol::xproto::ConnectionExt as _;

use crate::support::{Desktop, SNAPPY_KEY, desktop, key, wait_until};

/// Caps Lock's keysyms, the modifier rows holding it, and whether it repeats, as core
/// clients see them.
#[derive(Debug, PartialEq)]
struct CapsLockKey {
    keysyms: Vec<u32>,
    rows: Vec<usize>,
    repeats: bool,
}

impl Desktop {
    fn caps_lock_key(&self) -> CapsLockKey {
        let keysyms = self
            .client
            .get_keyboard_mapping(key::CAPS_LOCK, 1)
            .unwrap()
            .reply()
            .unwrap()
            .keysyms;
        let mapping = self.client.get_modifier_mapping().unwrap().reply().unwrap();
        let per_row = usize::from(mapping.keycodes_per_modifier());
        let rows = (0..8)
            .filter(|row| mapping.keycodes[row * per_row..(row + 1) * per_row].contains(&key::CAPS_LOCK))
            .collect();
        let control = self.client.get_keyboard_control().unwrap().reply().unwrap();
        let repeats =
            control.auto_repeats[usize::from(key::CAPS_LOCK / 8)] & (1 << (key::CAPS_LOCK % 8)) != 0;
        CapsLockKey {
            keysyms,
            rows,
            repeats,
        }
    }

    fn caps_lock_on(&self) -> bool {
        let pointer = self.client.query_pointer(self.root).unwrap().reply().unwrap();
        pointer.mask.contains(x11rb::protocol::xproto::KeyButMask::LOCK)
    }

    fn caps_lock_events(&self) -> Vec<CapsLockInput> {
        self.drain()
            .into_iter()
            .filter_map(|event| match event {
                PlatformEvent::CapsLock(input) => Some(input),
                _ => None,
            })
            .collect()
    }

    /// Load the session's keymap again, as desktops do on a layout change, with `options`.
    fn setxkbmap(&self, options: &str) {
        let display = std::env::var("SNAPPY_X11_TEST_DISPLAY").unwrap();
        let status = std::process::Command::new("setxkbmap")
            .args([
                "-display", &display, "-rules", "evdev", "-model", "pc105", "-layout", "us",
            ])
            .args(["-option", "", "-option", options])
            .status()
            .unwrap();
        assert!(status.success());
    }
}

const VOID_SYMBOL: u32 = 0x00ff_ffff;
const CAPS_LOCK_KEYSYM: u32 = 0xffe5;
const ESCAPE_KEYSYM: u32 = 0xff1b;
/// Control, Mod1, and Mod4, in `GetModifierMapping` order.
const SNAPPY_KEY_ROWS: [usize; 3] = [2, 3, 6];

#[test]
fn caps_lock_held_is_the_snappy_key_a_lone_tap_comes_alone_and_restoring_gives_it_back() {
    let mut desktop = desktop!();
    let before = desktop.caps_lock_key();
    assert_eq!(before.keysyms.first(), Some(&CAPS_LOCK_KEYSYM));
    assert_eq!(before.rows, [1], "a Lock key");

    let snappy_key: Mods = SNAPPY_KEY.into_iter().collect();
    desktop.platform.hold_caps_lock_as(snappy_key).unwrap();
    let held = desktop.caps_lock_key();
    assert!(
        held.keysyms
            .iter()
            .all(|keysym| [VOID_SYMBOL, 0].contains(keysym)),
        "{held:?}"
    );
    assert_eq!(held.rows, SNAPPY_KEY_ROWS);
    assert!(!held.repeats);
    let to_right = Combo::new("ArrowRight", SNAPPY_KEY);
    assert!(desktop.platform.register(&[(3, to_right)]).is_empty());
    desktop.drain();

    desktop.key(key::CAPS_LOCK, true);
    desktop.tap(&[key::RIGHT]);
    desktop.key(key::CAPS_LOCK, false);
    let events = desktop.drain();
    assert!(events.contains(&PlatformEvent::HotKey(3)), "{events:?}");
    let caps: Vec<&PlatformEvent> = events
        .iter()
        .filter(|event| matches!(event, PlatformEvent::CapsLock(_)))
        .collect();
    assert_eq!(
        caps,
        [
            &PlatformEvent::CapsLock(CapsLockInput::Pressed),
            &PlatformEvent::CapsLock(CapsLockInput::OtherInput),
            &PlatformEvent::CapsLock(CapsLockInput::Released)
        ]
    );
    assert!(!desktop.caps_lock_on(), "holding never locks");

    desktop.tap(&[key::CAPS_LOCK]);
    assert_eq!(
        desktop.caps_lock_events(),
        [CapsLockInput::Pressed, CapsLockInput::Released]
    );
    assert!(!desktop.caps_lock_on());
    desktop.key(key::CAPS_LOCK, true);
    desktop.button(true);
    desktop.button(false);
    desktop.key(key::CAPS_LOCK, false);
    assert_eq!(
        desktop.caps_lock_events(),
        [
            CapsLockInput::Pressed,
            CapsLockInput::OtherInput,
            CapsLockInput::Released
        ],
        "a mouse button is something else used"
    );

    desktop.platform.toggle_caps_lock().unwrap();
    assert!(wait_until(|| desktop.caps_lock_on()));
    desktop.platform.toggle_caps_lock().unwrap();
    assert!(wait_until(|| !desktop.caps_lock_on()));

    desktop.platform.unregister_all();
    desktop.platform.restore_caps_lock();
    assert_eq!(desktop.caps_lock_key(), before);
    desktop.tap(&[key::CAPS_LOCK]);
    assert!(
        wait_until(|| desktop.caps_lock_on()),
        "Caps Lock locks as usual again"
    );
    assert!(desktop.caps_lock_events().is_empty());
    desktop.tap(&[key::CAPS_LOCK]);
    assert!(wait_until(|| !desktop.caps_lock_on()));
}

#[test]
fn what_a_crashed_run_left_is_put_back_from_its_record() {
    let mut desktop = desktop!();
    let before = desktop.caps_lock_key();
    let snappy_key: Mods = SNAPPY_KEY.into_iter().collect();
    let undo = desktop.platform.hold_caps_lock_as(snappy_key).unwrap();
    assert_ne!(desktop.caps_lock_key(), before);

    desktop.platform.undo_caps_lock(&undo);
    assert_eq!(desktop.caps_lock_key(), before);
    desktop.platform.undo_caps_lock(&undo);
    assert_eq!(desktop.caps_lock_key(), before, "undoing twice changes nothing");
    desktop.platform.restore_caps_lock();
    assert_eq!(desktop.caps_lock_key(), before);

    // A start after a crash holds Caps Lock right after undoing, before the server's notice
    // of the undo reaches the event thread.
    desktop.platform.hold_caps_lock_as(snappy_key).unwrap();
    desktop.platform.restore_caps_lock();
    assert_eq!(desktop.caps_lock_key(), before);
}

#[test]
fn a_caps_lock_option_is_refused_and_a_reloaded_keymap_lets_go_of_caps_lock() {
    let mut desktop = desktop!();
    let snappy_key: Mods = SNAPPY_KEY.into_iter().collect();
    let escape_refusal = "Caps Lock is already remapped by the keyboard option caps:escape. Remove it to use Caps Lock as the Snappy key.";
    desktop.setxkbmap("caps:escape");
    let refused = desktop.platform.hold_caps_lock_as(snappy_key);
    desktop.setxkbmap("");
    assert_eq!(
        refused.unwrap_err(),
        CapsLockRefusal::Refused(escape_refusal.to_owned())
    );
    let before = desktop.caps_lock_key();

    // A reloaded keymap's own Caps Lock stays; the engine hears it was lost and holds again.
    desktop.platform.hold_caps_lock_as(snappy_key).unwrap();
    desktop.drain();
    desktop.setxkbmap("");
    assert!(wait_until(|| desktop
        .drain()
        .contains(&PlatformEvent::CapsLockLost)));
    assert_eq!(desktop.caps_lock_key(), before);
    desktop.platform.hold_caps_lock_as(snappy_key).unwrap();
    assert_eq!(desktop.caps_lock_key().rows, SNAPPY_KEY_ROWS, "held again");

    // One whose options remap Caps Lock keeps their Caps Lock, and holding is refused.
    desktop.setxkbmap("caps:escape");
    assert!(wait_until(|| desktop
        .drain()
        .contains(&PlatformEvent::CapsLockLost)));
    assert_eq!(desktop.caps_lock_key().keysyms.first(), Some(&ESCAPE_KEYSYM));
    assert_eq!(
        desktop.platform.hold_caps_lock_as(snappy_key).unwrap_err(),
        CapsLockRefusal::Refused(escape_refusal.to_owned())
    );
    desktop.platform.restore_caps_lock();
    assert_eq!(
        desktop.caps_lock_key().keysyms.first(),
        Some(&ESCAPE_KEYSYM),
        "theirs"
    );
    desktop.setxkbmap("");
    assert_eq!(desktop.caps_lock_key(), before);
}
