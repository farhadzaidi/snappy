//! W3C key codes and modifiers translated to what Carbon hotkeys and NSEvent use.

use snappy_core::keys::{Combo, KeyCode, Modifier, Mods};

use crate::ffi::carbon::{CMD_KEY, CONTROL_KEY, OPTION_KEY, SHIFT_KEY};

/// NSEvent modifier flag bits (`NSEventModifierFlags`).
const EVENT_SHIFT: usize = 1 << 17;
const EVENT_CONTROL: usize = 1 << 18;
const EVENT_OPTION: usize = 1 << 19;
const EVENT_COMMAND: usize = 1 << 20;

/// W3C code to macOS virtual key code.
const TABLE: &[(&str, u32)] = &[
    ("KeyA", 0),
    ("KeyS", 1),
    ("KeyD", 2),
    ("KeyF", 3),
    ("KeyH", 4),
    ("KeyG", 5),
    ("KeyZ", 6),
    ("KeyX", 7),
    ("KeyC", 8),
    ("KeyV", 9),
    ("IntlBackslash", 10),
    ("KeyB", 11),
    ("KeyQ", 12),
    ("KeyW", 13),
    ("KeyE", 14),
    ("KeyR", 15),
    ("KeyY", 16),
    ("KeyT", 17),
    ("Digit1", 18),
    ("Digit2", 19),
    ("Digit3", 20),
    ("Digit4", 21),
    ("Digit6", 22),
    ("Digit5", 23),
    ("Equal", 24),
    ("Digit9", 25),
    ("Digit7", 26),
    ("Minus", 27),
    ("Digit8", 28),
    ("Digit0", 29),
    ("BracketRight", 30),
    ("KeyO", 31),
    ("KeyU", 32),
    ("BracketLeft", 33),
    ("KeyI", 34),
    ("KeyP", 35),
    ("Enter", 36),
    ("KeyL", 37),
    ("KeyJ", 38),
    ("Quote", 39),
    ("KeyK", 40),
    ("Semicolon", 41),
    ("Backslash", 42),
    ("Comma", 43),
    ("Slash", 44),
    ("KeyN", 45),
    ("KeyM", 46),
    ("Period", 47),
    ("Tab", 48),
    ("Space", 49),
    ("Backquote", 50),
    ("Backspace", 51),
    ("Escape", 53),
    ("F17", 64),
    ("NumpadDecimal", 65),
    ("NumpadMultiply", 67),
    ("NumpadAdd", 69),
    ("NumLock", 71),
    ("AudioVolumeUp", 72),
    ("AudioVolumeDown", 73),
    ("AudioVolumeMute", 74),
    ("NumpadDivide", 75),
    ("NumpadEnter", 76),
    ("NumpadSubtract", 78),
    ("F18", 79),
    ("F19", 80),
    ("NumpadEqual", 81),
    ("Numpad0", 82),
    ("Numpad1", 83),
    ("Numpad2", 84),
    ("Numpad3", 85),
    ("Numpad4", 86),
    ("Numpad5", 87),
    ("Numpad6", 88),
    ("Numpad7", 89),
    ("F20", 90),
    ("Numpad8", 91),
    ("Numpad9", 92),
    ("IntlYen", 93),
    ("IntlRo", 94),
    ("NumpadComma", 95),
    ("F5", 96),
    ("F6", 97),
    ("F7", 98),
    ("F3", 99),
    ("F8", 100),
    ("F9", 101),
    ("Lang2", 102),
    ("F11", 103),
    ("Lang1", 104),
    ("F13", 105),
    ("F16", 106),
    ("F14", 107),
    ("F10", 109),
    ("ContextMenu", 110),
    ("F12", 111),
    ("F15", 113),
    ("Help", 114),
    ("Home", 115),
    ("PageUp", 116),
    ("Delete", 117),
    ("F4", 118),
    ("End", 119),
    ("F2", 120),
    ("PageDown", 121),
    ("F1", 122),
    ("ArrowLeft", 123),
    ("ArrowRight", 124),
    ("ArrowDown", 125),
    ("ArrowUp", 126),
];

pub fn virtual_key_code(code: &KeyCode) -> Option<u32> {
    TABLE
        .iter()
        .find(|(name, _)| *name == code.as_str())
        .map(|(_, key)| *key)
}

pub fn carbon_modifiers(mods: Mods) -> u32 {
    [
        (Modifier::Meta, CMD_KEY),
        (Modifier::Shift, SHIFT_KEY),
        (Modifier::Alt, OPTION_KEY),
        (Modifier::Ctrl, CONTROL_KEY),
    ]
    .iter()
    .filter(|(modifier, _)| mods.contains(*modifier))
    .fold(0, |mask, (_, flag)| mask | flag)
}

/// A Carbon registration: key, modifiers, and the hotkey ID derived from the chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CarbonChord {
    pub key_code: u32,
    pub modifiers: u32,
    pub id: u32,
}

impl CarbonChord {
    /// `None` for keys macOS can't register.
    pub fn from_combo(combo: &Combo) -> Option<Self> {
        let key_code = virtual_key_code(&combo.code)?;
        let modifiers = carbon_modifiers(combo.mods);
        // Key codes stay below 128 and modifier masks start at bit 8, so the two never overlap.
        Some(Self {
            key_code,
            modifiers,
            id: modifiers | key_code,
        })
    }
}

/// The modifiers held, from an NSEvent's raw modifier flags.
pub fn mods_from_event_flags(flags: usize) -> Mods {
    [
        (EVENT_SHIFT, Modifier::Shift),
        (EVENT_CONTROL, Modifier::Ctrl),
        (EVENT_OPTION, Modifier::Alt),
        (EVENT_COMMAND, Modifier::Meta),
    ]
    .iter()
    .filter(|(bit, _)| flags & bit != 0)
    .map(|(_, modifier)| *modifier)
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(name: &str) -> KeyCode {
        KeyCode::new(name)
    }

    #[test]
    fn letters_digits_and_arrows_map_to_their_virtual_codes() {
        assert_eq!(virtual_key_code(&code("KeyA")), Some(0));
        assert_eq!(virtual_key_code(&code("KeyZ")), Some(6));
        assert_eq!(virtual_key_code(&code("Digit1")), Some(18));
        assert_eq!(virtual_key_code(&code("Digit5")), Some(23));
        assert_eq!(virtual_key_code(&code("Digit6")), Some(22));
        assert_eq!(virtual_key_code(&code("BracketRight")), Some(30));
        assert_eq!(virtual_key_code(&code("ArrowLeft")), Some(123));
        assert_eq!(virtual_key_code(&code("ArrowUp")), Some(126));
        assert_eq!(virtual_key_code(&code("Escape")), Some(53));
    }

    #[test]
    fn function_and_keypad_keys_follow_the_table() {
        assert_eq!(virtual_key_code(&code("F1")), Some(122));
        assert_eq!(virtual_key_code(&code("F12")), Some(111));
        assert_eq!(virtual_key_code(&code("F20")), Some(90));
        assert_eq!(virtual_key_code(&code("Numpad0")), Some(82));
        assert_eq!(virtual_key_code(&code("NumpadEnter")), Some(76));
        assert_eq!(virtual_key_code(&code("ContextMenu")), Some(110));
    }

    #[test]
    fn unknown_codes_have_no_virtual_code() {
        assert_eq!(virtual_key_code(&code("NotAKey")), None);
    }

    #[test]
    fn every_mapped_code_is_a_known_shortcut_key() {
        let names = TABLE.iter().map(|(name, _)| *name);
        assert!(names.clone().all(|name| code(name).is_known()));
        assert_eq!(
            TABLE.len(),
            names.collect::<std::collections::BTreeSet<_>>().len()
        );
    }

    #[test]
    fn virtual_codes_are_unique() {
        let codes: std::collections::BTreeSet<u32> = TABLE.iter().map(|(_, key)| *key).collect();
        assert_eq!(codes.len(), TABLE.len());
    }

    #[test]
    fn modifiers_map_to_carbon_masks() {
        let hyper = Mods::from_iter([Modifier::Ctrl, Modifier::Alt, Modifier::Meta]);
        assert_eq!(carbon_modifiers(hyper), CONTROL_KEY | OPTION_KEY | CMD_KEY);
        assert_eq!(carbon_modifiers(Mods::from_iter([Modifier::Shift])), SHIFT_KEY);
        assert_eq!(carbon_modifiers(Mods::NONE), 0);
    }

    #[test]
    fn hotkey_ids_are_derived_from_the_chord() {
        let hyper = [Modifier::Ctrl, Modifier::Alt, Modifier::Meta];
        let one = CarbonChord::from_combo(&Combo::new("Digit1", hyper));
        let two = CarbonChord::from_combo(&Combo::new("Digit2", hyper));
        assert_ne!(one.map(|c| c.id), two.map(|c| c.id));
        assert_eq!(
            one,
            Some(CarbonChord {
                key_code: 18,
                modifiers: CONTROL_KEY | OPTION_KEY | CMD_KEY,
                id: CONTROL_KEY | OPTION_KEY | CMD_KEY | 18,
            })
        );
    }

    #[test]
    fn chords_on_unmappable_keys_cannot_register() {
        assert_eq!(
            CarbonChord::from_combo(&Combo::new("Nope", [Modifier::Ctrl])),
            None
        );
    }

    #[test]
    fn event_flags_become_mods() {
        let mods = mods_from_event_flags(EVENT_SHIFT | EVENT_COMMAND | 1 << 16);
        assert_eq!(mods, Mods::from_iter([Modifier::Shift, Modifier::Meta]));
        assert_eq!(
            mods_from_event_flags(EVENT_CONTROL | EVENT_OPTION),
            Mods::from_iter([Modifier::Ctrl, Modifier::Alt])
        );
        assert!(mods_from_event_flags(0).is_empty());
    }
}
