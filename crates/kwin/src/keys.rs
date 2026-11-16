//! Shortcuts as KDE's shortcut service takes them: a Qt key code with Qt's modifier bits.
//!
//! KDE matches shortcuts by the symbol a key types, not by the physical key, so a chord is
//! named by the symbol its key types on a US layout, as KDE's own defaults are. With Shift on
//! a key whose symbol Shift changes, KWin takes Shift as used up by the symbol: Shift + 2
//! reaches the shortcut service as `@` alone, so it is registered that way. Letters keep
//! their Shift.

use snappy_core::keys::{Combo, Modifier, Mods};
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

/// The modifiers alone, as KDE's shortcut service takes a modifier-only shortcut.
pub fn modifiers_only(mods: Mods) -> i32 {
    [
        (Modifier::Shift, SHIFT),
        (Modifier::Ctrl, CONTROL),
        (Modifier::Alt, ALT),
        (Modifier::Meta, META),
    ]
    .into_iter()
    .filter(|(modifier, _)| mods.contains(*modifier))
    .fold(0, |bits, (_, bit)| bits | bit)
}

/// What Shift makes of the US layout's symbol keys other than letters, as Qt keys.
const SHIFTED: &[(&str, char)] = &[
    ("Digit1", '!'),
    ("Digit2", '@'),
    ("Digit3", '#'),
    ("Digit4", '$'),
    ("Digit5", '%'),
    ("Digit6", '^'),
    ("Digit7", '&'),
    ("Digit8", '*'),
    ("Digit9", '('),
    ("Digit0", ')'),
    ("Minus", '_'),
    ("Equal", '+'),
    ("BracketLeft", '{'),
    ("BracketRight", '}'),
    ("Semicolon", ':'),
    ("Quote", '"'),
    ("Backquote", '~'),
    ("Backslash", '|'),
    ("Comma", '<'),
    ("Period", '>'),
    ("Slash", '?'),
];

/// The chord as one Qt key code, or `None` for a key without a US symbol.
pub fn key_code(combo: &Combo) -> Option<i32> {
    let code = combo.code.as_str();
    if combo.mods.contains(Modifier::Shift)
        && let Some((_, symbol)) = SHIFTED.iter().find(|(shifted, _)| *shifted == code)
    {
        let without_shift = combo.mods.without(Mods::from_iter([Modifier::Shift]));
        return Some(*symbol as i32 | modifiers_only(without_shift));
    }
    let key = qt_key(key_codes::us_keysym(code)?)?;
    Some(key | modifiers_only(combo.mods))
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{KeyChord, KeyCode};

    use super::*;

    #[test]
    fn shift_on_a_symbol_key_is_the_shifted_symbol_as_kwin_reports_it() {
        let combo = |code: &str, mods: &[Modifier]| Combo {
            code: KeyCode::new(code),
            mods: Mods::from_iter(mods.iter().copied()),
        };
        let send = combo("Digit2", &[Modifier::Ctrl, Modifier::Shift, Modifier::Meta]);
        assert_eq!(key_code(&send), Some(CONTROL | META | '@' as i32));
        let slash = combo("Slash", &[Modifier::Shift, Modifier::Alt]);
        assert_eq!(key_code(&slash), Some(ALT | '?' as i32));
        // Letters and keys without a shifted symbol keep Shift.
        assert_eq!(key_code(&combo("KeyW", &[Modifier::Shift])), Some(SHIFT | 0x57));
        assert_eq!(
            key_code(&combo("ArrowLeft", &[Modifier::Shift])),
            Some(SHIFT | 0x0100_0012)
        );
    }

    #[test]
    fn the_snappy_key_expands_to_control_alt_and_meta() {
        let combo = KeyChord::snappy("Digit1", []).combo(snappy_core::keys::SnappyKey::DEFAULT);
        assert_eq!(key_code(&combo), Some(CONTROL | ALT | META | 0x31));
        let shifted =
            KeyChord::snappy("ArrowLeft", [Modifier::Shift]).combo(snappy_core::keys::SnappyKey::DEFAULT);
        assert_eq!(
            key_code(&shifted),
            Some(SHIFT | CONTROL | ALT | META | 0x0100_0012)
        );
    }

    #[test]
    fn every_default_shortcut_has_a_key_code_and_return_and_backslash_are_qts() {
        let mut settings = snappy_core::settings::Settings::defaults(snappy_core::keys::Os::Linux);
        settings.workspaces.items = (1..=9)
            .map(|n| snappy_core::settings::Workspace::numbered(format!("workspace-{n}"), n))
            .collect();
        for combo in snappy_core::shortcuts::bound_combos(&settings) {
            assert!(key_code(&combo).is_some(), "{combo:?}");
        }
        let select = settings
            .navigation
            .select_window
            .unwrap()
            .combo(snappy_core::keys::SnappyKey::DEFAULT);
        assert_eq!(key_code(&select), Some(CONTROL | ALT | META | 0x0100_0004));
        let clear = settings
            .navigation
            .clear_selection
            .unwrap()
            .combo(snappy_core::keys::SnappyKey::DEFAULT);
        assert_eq!(key_code(&clear), Some(CONTROL | ALT | META | 0x5c));
    }

    #[test]
    fn a_modifier_only_shortcut_is_the_modifier_bits_alone() {
        assert_eq!(
            modifiers_only(snappy_core::keys::SnappyKey::DEFAULT.modifiers()),
            CONTROL | ALT | META
        );
        assert_eq!(modifiers_only(Mods::NONE), 0);
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
