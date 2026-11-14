//! Shortcuts as GNOME Shell's accelerators, and its modifier masks as Snappy's modifiers.
//!
//! Accelerators name the physical key by its keycode, so a shortcut stays on the same key on
//! every keyboard layout, as it does on X11.

use snappy_core::keys::{Combo, Modifier, Mods};
use snappy_linux::key_codes;

/// Clutter's modifier bits. Super sets the real Mod4 bit, and GNOME may add its virtual
/// Super bit too; Meta, the virtual bit Alt may set, is not Super.
const SHIFT: u32 = 1 << 0;
const CONTROL: u32 = 1 << 2;
const MOD1: u32 = 1 << 3;
const MOD4: u32 = 1 << 6;
const SUPER: u32 = 1 << 26;

/// Caps Lock's evdev keycode.
const CAPS_LOCK: u8 = 66;
/// The evdev keycodes of both hands' Shift, Control, Alt, and Super keys.
const MODIFIER_KEYS: [u8; 8] = [50, 62, 37, 105, 64, 108, 133, 134];

/// `<Control><Alt><Super><Shift>0x0a` for Snappy + Shift + 1, or `None` for a key with no
/// physical keycode.
pub fn accelerator(combo: &Combo) -> Option<String> {
    let keycode = key_codes::evdev_keycode(combo.code.as_str())?;
    Some(format!("{}0x{keycode:02x}", modifier_prefix(combo.mods)))
}

/// The modifiers of an accelerator, such as `<Control><Alt><Super>`.
pub fn modifier_prefix(mods: Mods) -> String {
    [
        (Modifier::Ctrl, "<Control>"),
        (Modifier::Alt, "<Alt>"),
        (Modifier::Meta, "<Super>"),
        (Modifier::Shift, "<Shift>"),
    ]
    .into_iter()
    .filter(|(modifier, _)| mods.contains(*modifier))
    .map(|(_, name)| name)
    .collect()
}

/// The keycodes to grab while Caps Lock, as the Snappy key, is down: every key but Caps Lock
/// itself and the modifier keys, which may join the Snappy key as they do its own keys.
pub fn keys_while_caps_lock_is_down() -> Vec<u8> {
    (key_codes::EVDEV_OFFSET + 1..=u8::MAX)
        .filter(|keycode| *keycode != CAPS_LOCK && !MODIFIER_KEYS.contains(keycode))
        .collect()
}

/// The mask bits a held `modifier` sets.
pub fn mask(modifier: Modifier) -> u32 {
    match modifier {
        Modifier::Shift => SHIFT,
        Modifier::Ctrl => CONTROL,
        Modifier::Alt => MOD1,
        Modifier::Meta => MOD4,
    }
}

/// The mask bits held `mods` set together.
pub fn mask_of(mods: Mods) -> u32 {
    mods.iter().map(mask).fold(0, |all, bits| all | bits)
}

pub fn mods(mask: u32) -> Mods {
    let mut mods = Mods::NONE;
    for (bits, modifier) in [
        (SHIFT, Modifier::Shift),
        (CONTROL, Modifier::Ctrl),
        (MOD1, Modifier::Alt),
        (MOD4 | SUPER, Modifier::Meta),
    ] {
        if mask & bits != 0 {
            mods = mods.with(modifier);
        }
    }
    mods
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{KeyChord, KeyCode};

    use super::*;

    #[test]
    fn the_snappy_key_expands_to_control_alt_and_super_on_the_physical_key() {
        let combo = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
        assert_eq!(accelerator(&combo).as_deref(), Some("<Control><Alt><Super>0x0a"));
        let shifted =
            KeyChord::snappy("ArrowLeft", [Modifier::Shift]).combo(snappy_core::keys::SnappyKey::DEFAULT);
        assert_eq!(
            accelerator(&shifted).as_deref(),
            Some("<Control><Alt><Super><Shift>0x71")
        );
    }

    #[test]
    fn keys_without_a_keycode_have_no_accelerator() {
        let combo = Combo {
            code: KeyCode::new("NotAKey"),
            mods: Mods::from_iter([Modifier::Ctrl]),
        };
        assert_eq!(accelerator(&combo), None);
    }

    #[test]
    fn each_modifier_has_the_bit_it_sets() {
        for modifier in [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta] {
            assert_eq!(mods(mask(modifier)), Mods::from_iter([modifier]));
        }
    }

    #[test]
    fn the_snappy_keys_modifiers_become_an_accelerator_prefix_and_a_mask() {
        let key = snappy_core::keys::SnappyKey::DEFAULT.modifiers();
        assert_eq!(modifier_prefix(key), "<Control><Alt><Super>");
        assert_eq!(mask_of(key), CONTROL | MOD1 | MOD4);
        assert_eq!(mods(mask_of(key)), key);
    }

    #[test]
    fn caps_lock_and_the_modifier_keys_are_never_grabbed_while_caps_lock_is_down() {
        let keys = keys_while_caps_lock_is_down();
        assert!(!keys.contains(&CAPS_LOCK));
        for modifier in [Modifier::Shift, Modifier::Ctrl, Modifier::Alt, Modifier::Meta] {
            assert!(!keys.contains(&key_codes::modifier_keycode(modifier)));
        }
        for code in ["KeyJ", "ArrowRight", "Digit1", "Space", "F18"] {
            assert!(keys.contains(&key_codes::evdev_keycode(code).unwrap()), "{code}");
        }
    }

    #[test]
    fn modifier_masks_become_snappys_modifiers() {
        assert_eq!(mods(0), Mods::NONE);
        assert_eq!(mods(1), Mods::from_iter([Modifier::Shift]));
        assert_eq!(mods(4 | 8), Mods::from_iter([Modifier::Ctrl, Modifier::Alt]));
        assert_eq!(mods(64), Mods::from_iter([Modifier::Meta]));
        assert_eq!(mods(1 << 26), Mods::from_iter([Modifier::Meta]));
        assert_eq!(mods(1 << 28), Mods::NONE, "the virtual Meta bit is Alt's");
        assert_eq!(
            mods(2 | 16),
            Mods::NONE,
            "Caps Lock and Num Lock aren't modifiers"
        );
    }
}
