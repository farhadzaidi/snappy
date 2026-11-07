//! W3C `KeyboardEvent.code` names mapped to X keys: the evdev keycode of the physical
//! key, and the keysym a US layout gives it. Keysym 0 means no unambiguous US keysym.

/// `(code, evdev keycode, US keysym)`. Evdev keycodes are Linux input codes plus 8.
const KEYS: &[(&str, u8, u32)] = &[
    ("Escape", 9, 0xff1b),
    ("Digit1", 10, 0x31),
    ("Digit2", 11, 0x32),
    ("Digit3", 12, 0x33),
    ("Digit4", 13, 0x34),
    ("Digit5", 14, 0x35),
    ("Digit6", 15, 0x36),
    ("Digit7", 16, 0x37),
    ("Digit8", 17, 0x38),
    ("Digit9", 18, 0x39),
    ("Digit0", 19, 0x30),
    ("Minus", 20, 0x2d),
    ("Equal", 21, 0x3d),
    ("Backspace", 22, 0xff08),
    ("Tab", 23, 0xff09),
    ("KeyQ", 24, 0x71),
    ("KeyW", 25, 0x77),
    ("KeyE", 26, 0x65),
    ("KeyR", 27, 0x72),
    ("KeyT", 28, 0x74),
    ("KeyY", 29, 0x79),
    ("KeyU", 30, 0x75),
    ("KeyI", 31, 0x69),
    ("KeyO", 32, 0x6f),
    ("KeyP", 33, 0x70),
    ("BracketLeft", 34, 0x5b),
    ("BracketRight", 35, 0x5d),
    ("Enter", 36, 0xff0d),
    ("KeyA", 38, 0x61),
    ("KeyS", 39, 0x73),
    ("KeyD", 40, 0x64),
    ("KeyF", 41, 0x66),
    ("KeyG", 42, 0x67),
    ("KeyH", 43, 0x68),
    ("KeyJ", 44, 0x6a),
    ("KeyK", 45, 0x6b),
    ("KeyL", 46, 0x6c),
    ("Semicolon", 47, 0x3b),
    ("Quote", 48, 0x27),
    ("Backquote", 49, 0x60),
    ("Backslash", 51, 0x5c),
    ("KeyZ", 52, 0x7a),
    ("KeyX", 53, 0x78),
    ("KeyC", 54, 0x63),
    ("KeyV", 55, 0x76),
    ("KeyB", 56, 0x62),
    ("KeyN", 57, 0x6e),
    ("KeyM", 58, 0x6d),
    ("Comma", 59, 0x2c),
    ("Period", 60, 0x2e),
    ("Slash", 61, 0x2f),
    ("NumpadMultiply", 63, 0xffaa),
    ("Space", 65, 0x20),
    ("F1", 67, 0xffbe),
    ("F2", 68, 0xffbf),
    ("F3", 69, 0xffc0),
    ("F4", 70, 0xffc1),
    ("F5", 71, 0xffc2),
    ("F6", 72, 0xffc3),
    ("F7", 73, 0xffc4),
    ("F8", 74, 0xffc5),
    ("F9", 75, 0xffc6),
    ("F10", 76, 0xffc7),
    ("NumLock", 77, 0xff7f),
    ("Numpad7", 79, 0xffb7),
    ("Numpad8", 80, 0xffb8),
    ("Numpad9", 81, 0xffb9),
    ("NumpadSubtract", 82, 0xffad),
    ("Numpad4", 83, 0xffb4),
    ("Numpad5", 84, 0xffb5),
    ("Numpad6", 85, 0xffb6),
    ("NumpadAdd", 86, 0xffab),
    ("Numpad1", 87, 0xffb1),
    ("Numpad2", 88, 0xffb2),
    ("Numpad3", 89, 0xffb3),
    ("Numpad0", 90, 0xffb0),
    ("NumpadDecimal", 91, 0xffae),
    ("IntlBackslash", 94, 0x3c),
    ("F11", 95, 0xffc8),
    ("F12", 96, 0xffc9),
    ("IntlRo", 97, 0),
    ("NumpadEnter", 104, 0xff8d),
    ("NumpadDivide", 106, 0xffaf),
    ("Home", 110, 0xff50),
    ("ArrowUp", 111, 0xff52),
    ("PageUp", 112, 0xff55),
    ("ArrowLeft", 113, 0xff51),
    ("ArrowRight", 114, 0xff53),
    ("End", 115, 0xff57),
    ("ArrowDown", 116, 0xff54),
    ("PageDown", 117, 0xff56),
    ("Delete", 119, 0xffff),
    ("AudioVolumeMute", 121, 0x1008_ff12),
    ("AudioVolumeDown", 122, 0x1008_ff11),
    ("AudioVolumeUp", 123, 0x1008_ff13),
    ("NumpadEqual", 125, 0xffbd),
    ("NumpadComma", 129, 0xffac),
    ("Lang1", 130, 0xff31),
    ("Lang2", 131, 0xff34),
    ("IntlYen", 132, 0xa5),
    ("ContextMenu", 135, 0xff67),
    ("Help", 146, 0xff6a),
    ("F13", 191, 0xffca),
    ("F14", 192, 0xffcb),
    ("F15", 193, 0xffcc),
    ("F16", 194, 0xffcd),
    ("F17", 195, 0xffce),
    ("F18", 196, 0xffcf),
    ("F19", 197, 0xffd0),
    ("F20", 198, 0xffd1),
];

pub const ESCAPE: &str = "Escape";

/// The physical key's evdev keycode.
pub fn evdev_keycode(code: &str) -> Option<u8> {
    KEYS.iter()
        .find(|(name, ..)| *name == code)
        .map(|(_, keycode, _)| *keycode)
}

/// The keysym a US layout produces on the key, unshifted.
pub fn us_keysym(code: &str) -> Option<u32> {
    KEYS.iter()
        .find(|(name, ..)| *name == code)
        .map(|(.., keysym)| *keysym)
        .filter(|keysym| *keysym != 0)
}

pub mod keysym {
    pub const ESCAPE: u32 = 0xff1b;
    pub const LEFT: u32 = 0xff51;
    pub const NUM_LOCK: u32 = 0xff7f;
    pub const SHIFT_L: u32 = 0xffe1;
    pub const CONTROL_L: u32 = 0xffe3;
    pub const ALT_L: u32 = 0xffe9;
    pub const ALT_R: u32 = 0xffea;
    pub const META_L: u32 = 0xffe7;
    pub const META_R: u32 = 0xffe8;
    pub const SUPER_L: u32 = 0xffeb;
    pub const SUPER_R: u32 = 0xffec;
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::KeyCode;

    use super::*;

    #[test]
    fn every_known_shortcut_key_has_a_physical_keycode() {
        let missing: Vec<&str> = KNOWN
            .iter()
            .copied()
            .filter(|code| evdev_keycode(code).is_none())
            .collect();
        assert!(missing.is_empty(), "no keycode for {missing:?}");
    }

    #[test]
    fn table_entries_are_known_codes_and_unique() {
        for (index, (code, keycode, _)) in KEYS.iter().enumerate() {
            assert!(KeyCode::new(*code).is_known(), "{code} is not a shortcut key");
            assert!(
                KEYS[index + 1..]
                    .iter()
                    .all(|(other, k, _)| other != code && k != keycode),
                "{code} repeats"
            );
        }
    }

    #[test]
    fn common_keys_map_to_evdev_and_us_keysyms() {
        assert_eq!(evdev_keycode("KeyA"), Some(38));
        assert_eq!(evdev_keycode("Digit1"), Some(10));
        assert_eq!(evdev_keycode("ArrowLeft"), Some(113));
        assert_eq!(us_keysym("KeyA"), Some(0x61));
        assert_eq!(us_keysym("ArrowRight"), Some(0xff53));
        assert_eq!(us_keysym("IntlRo"), None);
        assert_eq!(evdev_keycode("NotAKey"), None);
    }

    /// Every code `snappy-core` accepts, which the table must cover.
    const KNOWN: &[&str] = &[
        "KeyA",
        "KeyB",
        "KeyC",
        "KeyD",
        "KeyE",
        "KeyF",
        "KeyG",
        "KeyH",
        "KeyI",
        "KeyJ",
        "KeyK",
        "KeyL",
        "KeyM",
        "KeyN",
        "KeyO",
        "KeyP",
        "KeyQ",
        "KeyR",
        "KeyS",
        "KeyT",
        "KeyU",
        "KeyV",
        "KeyW",
        "KeyX",
        "KeyY",
        "KeyZ",
        "Digit0",
        "Digit1",
        "Digit2",
        "Digit3",
        "Digit4",
        "Digit5",
        "Digit6",
        "Digit7",
        "Digit8",
        "Digit9",
        "Minus",
        "Equal",
        "BracketLeft",
        "BracketRight",
        "Backslash",
        "Semicolon",
        "Quote",
        "Comma",
        "Period",
        "Slash",
        "Backquote",
        "IntlBackslash",
        "IntlYen",
        "IntlRo",
        "Enter",
        "Tab",
        "Space",
        "Backspace",
        "Delete",
        "Escape",
        "Home",
        "End",
        "PageUp",
        "PageDown",
        "Help",
        "ContextMenu",
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "F1",
        "F2",
        "F3",
        "F4",
        "F5",
        "F6",
        "F7",
        "F8",
        "F9",
        "F10",
        "F11",
        "F12",
        "F13",
        "F14",
        "F15",
        "F16",
        "F17",
        "F18",
        "F19",
        "F20",
        "Numpad0",
        "Numpad1",
        "Numpad2",
        "Numpad3",
        "Numpad4",
        "Numpad5",
        "Numpad6",
        "Numpad7",
        "Numpad8",
        "Numpad9",
        "NumpadDecimal",
        "NumpadMultiply",
        "NumpadAdd",
        "NumpadSubtract",
        "NumpadDivide",
        "NumpadEnter",
        "NumpadEqual",
        "NumpadComma",
        "NumLock",
        "Lang1",
        "Lang2",
        "AudioVolumeUp",
        "AudioVolumeDown",
        "AudioVolumeMute",
    ];
}
