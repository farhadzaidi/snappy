//! Shortcuts as KDE's shortcut service takes them: a Qt key code with Qt's modifier bits.
//!
//! KDE matches shortcuts by the symbol a key types, not by the physical key, so a chord is
//! named by the symbol its key types on a US layout, as KDE's own defaults are.

use snappy_core::keys::{Combo, Modifier};
use snappy_linux::key_codes;

const SHIFT: i32 = 0x0200_0000;
const CONTROL: i32 = 0x0400_0000;
const ALT: i32 = 0x0800_0000;
const META: i32 = 0x1000_0000;
const KEYPAD: i32 = 0x2000_0000;

/// Qt's codes for keys without a printable symbol, by X keysym.
const SPECIAL: &[(u32, i32)] = &[
    (0xff1b, 0x0100_0000),      // Escape
    (0xff09, 0x0100_0001),      // Tab
    (0xff08, 0x0100_0003),      // Backspace
    (0xff0d, 0x0100_0004),      // Return
    (0xff63, 0x0100_0006),      // Insert
    (0xffff, 0x0100_0007),      // Delete
    (0xff50, 0x0100_0010),      // Home
    (0xff57, 0x0100_0011),      // End
    (0xff51, 0x0100_0012),      // Left
    (0xff52, 0x0100_0013),      // Up
    (0xff53, 0x0100_0014),      // Right
    (0xff54, 0x0100_0015),      // Down
    (0xff55, 0x0100_0016),      // Page Up
    (0xff56, 0x0100_0017),      // Page Down
    (0xff7f, 0x0100_0025),      // Num Lock
    (0xff67, 0x0100_0055),      // Menu
    (0xff6a, 0x0100_0058),      // Help
    (0x1008_ff11, 0x0100_0070), // Volume Down
    (0x1008_ff12, 0x0100_0071), // Volume Mute
    (0x1008_ff13, 0x0100_0072), // Volume Up
];

/// Keypad keysyms and the key Qt reports with its keypad bit.
const KEYPAD_KEYS: &[(u32, i32)] = &[
    (0xff8d, 0x0100_0005), // Enter
    (0xffaa, 0x2a),        // *
    (0xffab, 0x2b),        // +
    (0xffac, 0x2c),        // ,
    (0xffad, 0x2d),        // -
    (0xffae, 0x2e),        // .
    (0xffaf, 0x2f),        // /
    (0xffbd, 0x3d),        // =
];

const F1: u32 = 0xffbe;
const F35: u32 = 0xffe0;
const KEYPAD_0: u32 = 0xffb0;
const KEYPAD_9: u32 = 0xffb9;

/// The Qt key a US layout gives `keysym`, with the keypad bit for keypad keys.
fn qt_key(keysym: u32) -> Option<i32> {
    if let Some((_, key)) = SPECIAL.iter().find(|(sym, _)| *sym == keysym) {
        return Some(*key);
    }
    if let Some((_, key)) = KEYPAD_KEYS.iter().find(|(sym, _)| *sym == keysym) {
        return Some(*key | KEYPAD);
    }
    match keysym {
        F1..=F35 => Some(0x0100_0030 + i32::try_from(keysym - F1).ok()?),
        KEYPAD_0..=KEYPAD_9 => Some((0x30 + i32::try_from(keysym - KEYPAD_0).ok()?) | KEYPAD),
        // Latin-1: Qt names letters by their capitals.
        0x61..=0x7a => Some(i32::try_from(keysym).ok()? - 0x20),
        0x20..=0xff => i32::try_from(keysym).ok(),
        _ => None,
    }
}

/// The chord as one Qt key code, or `None` for a key without a US symbol.
pub fn key_code(combo: &Combo) -> Option<i32> {
    let key = qt_key(key_codes::us_keysym(combo.code.as_str())?)?;
    let mods = [
        (Modifier::Shift, SHIFT),
        (Modifier::Ctrl, CONTROL),
        (Modifier::Alt, ALT),
        (Modifier::Meta, META),
    ]
    .into_iter()
    .filter(|(modifier, _)| combo.mods.contains(*modifier))
    .fold(0, |mods, (_, bit)| mods | bit);
    Some(key | mods)
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{KeyChord, KeyCode, Mods};

    use super::*;

    #[test]
    fn the_snappy_key_expands_to_control_alt_and_meta() {
        let combo = KeyChord::snappy("Digit1", []).combo();
        assert_eq!(key_code(&combo), Some(CONTROL | ALT | META | 0x31));
        let shifted = KeyChord::snappy("ArrowLeft", [Modifier::Shift]).combo();
        assert_eq!(
            key_code(&shifted),
            Some(SHIFT | CONTROL | ALT | META | 0x0100_0012)
        );
    }

    #[test]
    fn letters_are_capitals_and_keypad_keys_carry_the_keypad_bit() {
        let combo = |code: &str| Combo {
            code: KeyCode::new(code),
            mods: Mods::NONE,
        };
        assert_eq!(key_code(&combo("KeyA")), Some(0x41));
        assert_eq!(key_code(&combo("Space")), Some(0x20));
        assert_eq!(key_code(&combo("Slash")), Some(0x2f));
        assert_eq!(key_code(&combo("F12")), Some(0x0100_003b));
        assert_eq!(key_code(&combo("Numpad5")), Some(0x35 | KEYPAD));
        assert_eq!(key_code(&combo("NumpadEnter")), Some(0x0100_0005 | KEYPAD));
        assert_eq!(key_code(&combo("NotAKey")), None);
    }
}
