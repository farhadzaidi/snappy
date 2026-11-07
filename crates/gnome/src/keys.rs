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

/// `<Control><Alt><Super><Shift>0x0a` for Snappy + Shift + 1, or `None` for a key with no
/// physical keycode.
pub fn accelerator(combo: &Combo) -> Option<String> {
    let keycode = key_codes::evdev_keycode(combo.code.as_str())?;
    let mut accelerator = String::new();
    for (modifier, name) in [
        (Modifier::Ctrl, "<Control>"),
        (Modifier::Alt, "<Alt>"),
        (Modifier::Meta, "<Super>"),
        (Modifier::Shift, "<Shift>"),
    ] {
        if combo.mods.contains(modifier) {
            accelerator.push_str(name);
        }
    }
    accelerator.push_str(&format!("0x{keycode:02x}"));
    Some(accelerator)
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
        let combo = KeyChord::snappy("Digit1", []).combo();
        assert_eq!(accelerator(&combo).as_deref(), Some("<Control><Alt><Super>0x0a"));
        let shifted = KeyChord::snappy("ArrowLeft", [Modifier::Shift]).combo();
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
