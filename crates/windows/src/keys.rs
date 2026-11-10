//! W3C `KeyboardEvent.code` names mapped to Windows keys: the set-1 scan code of the
//! physical key (0xE0 prefix for extended keys), and for keys whose meaning never depends
//! on the keyboard layout, their fixed virtual key. Layout-dependent keys get their
//! virtual key from the current layout through `MapVirtualKeyExW`.

use windows::Win32::UI::Input::KeyboardAndMouse::{
    HKL, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW, VIRTUAL_KEY, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN,
};

use snappy_core::keys::{KeyCode, Modifier};

/// A physical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Set-1 scan code; extended keys carry 0xE0 in the high byte.
    pub scan: u16,
    /// The virtual key when it doesn't depend on the layout.
    pub fixed: Option<u16>,
}

impl Key {
    pub fn is_extended(self) -> bool {
        self.scan >> 8 == 0xE0
    }
}

/// `(code, scan code, fixed virtual key or 0)`.
const KEYS: &[(&str, u16, u16)] = &[
    ("Escape", 0x01, 0x1B),
    ("Digit1", 0x02, 0),
    ("Digit2", 0x03, 0),
    ("Digit3", 0x04, 0),
    ("Digit4", 0x05, 0),
    ("Digit5", 0x06, 0),
    ("Digit6", 0x07, 0),
    ("Digit7", 0x08, 0),
    ("Digit8", 0x09, 0),
    ("Digit9", 0x0A, 0),
    ("Digit0", 0x0B, 0),
    ("Minus", 0x0C, 0),
    ("Equal", 0x0D, 0),
    ("Backspace", 0x0E, 0x08),
    ("Tab", 0x0F, 0x09),
    ("KeyQ", 0x10, 0),
    ("KeyW", 0x11, 0),
    ("KeyE", 0x12, 0),
    ("KeyR", 0x13, 0),
    ("KeyT", 0x14, 0),
    ("KeyY", 0x15, 0),
    ("KeyU", 0x16, 0),
    ("KeyI", 0x17, 0),
    ("KeyO", 0x18, 0),
    ("KeyP", 0x19, 0),
    ("BracketLeft", 0x1A, 0),
    ("BracketRight", 0x1B, 0),
    ("Enter", 0x1C, 0x0D),
    ("KeyA", 0x1E, 0),
    ("KeyS", 0x1F, 0),
    ("KeyD", 0x20, 0),
    ("KeyF", 0x21, 0),
    ("KeyG", 0x22, 0),
    ("KeyH", 0x23, 0),
    ("KeyJ", 0x24, 0),
    ("KeyK", 0x25, 0),
    ("KeyL", 0x26, 0),
    ("Semicolon", 0x27, 0),
    ("Quote", 0x28, 0),
    ("Backquote", 0x29, 0),
    ("Backslash", 0x2B, 0),
    ("KeyZ", 0x2C, 0),
    ("KeyX", 0x2D, 0),
    ("KeyC", 0x2E, 0),
    ("KeyV", 0x2F, 0),
    ("KeyB", 0x30, 0),
    ("KeyN", 0x31, 0),
    ("KeyM", 0x32, 0),
    ("Comma", 0x33, 0),
    ("Period", 0x34, 0),
    ("Slash", 0x35, 0),
    ("NumpadMultiply", 0x37, 0x6A),
    ("Space", 0x39, 0x20),
    ("F1", 0x3B, 0x70),
    ("F2", 0x3C, 0x71),
    ("F3", 0x3D, 0x72),
    ("F4", 0x3E, 0x73),
    ("F5", 0x3F, 0x74),
    ("F6", 0x40, 0x75),
    ("F7", 0x41, 0x76),
    ("F8", 0x42, 0x77),
    ("F9", 0x43, 0x78),
    ("F10", 0x44, 0x79),
    ("Numpad7", 0x47, 0x67),
    ("Numpad8", 0x48, 0x68),
    ("Numpad9", 0x49, 0x69),
    ("NumpadSubtract", 0x4A, 0x6D),
    ("Numpad4", 0x4B, 0x64),
    ("Numpad5", 0x4C, 0x65),
    ("Numpad6", 0x4D, 0x66),
    ("NumpadAdd", 0x4E, 0x6B),
    ("Numpad1", 0x4F, 0x61),
    ("Numpad2", 0x50, 0x62),
    ("Numpad3", 0x51, 0x63),
    ("Numpad0", 0x52, 0x60),
    ("NumpadDecimal", 0x53, 0x6E),
    ("IntlBackslash", 0x56, 0),
    ("F11", 0x57, 0x7A),
    ("F12", 0x58, 0x7B),
    ("NumpadEqual", 0x59, 0x92),
    ("F13", 0x64, 0x7C),
    ("F14", 0x65, 0x7D),
    ("F15", 0x66, 0x7E),
    ("F16", 0x67, 0x7F),
    ("F17", 0x68, 0x80),
    ("F18", 0x69, 0x81),
    ("F19", 0x6A, 0x82),
    ("F20", 0x6B, 0x83),
    ("Lang2", 0x71, 0),
    ("Lang1", 0x72, 0),
    ("IntlRo", 0x73, 0),
    ("IntlYen", 0x7D, 0),
    ("NumpadComma", 0x7E, 0),
    ("NumpadEnter", 0xE01C, 0x0D),
    ("AudioVolumeMute", 0xE020, 0xAD),
    ("AudioVolumeDown", 0xE02E, 0xAE),
    ("AudioVolumeUp", 0xE030, 0xAF),
    ("NumpadDivide", 0xE035, 0x6F),
    ("NumLock", 0xE045, 0x90),
    ("Home", 0xE047, 0x24),
    ("ArrowUp", 0xE048, 0x26),
    ("PageUp", 0xE049, 0x21),
    ("ArrowLeft", 0xE04B, 0x25),
    ("ArrowRight", 0xE04D, 0x27),
    ("End", 0xE04F, 0x23),
    ("ArrowDown", 0xE050, 0x28),
    ("PageDown", 0xE051, 0x22),
    ("Delete", 0xE053, 0x2E),
    ("ContextMenu", 0xE05D, 0x5D),
];

/// The physical key for `code`, or `None` for keys PC keyboards don't have.
pub fn key(code: &KeyCode) -> Option<Key> {
    KEYS.iter()
        .find(|(name, _, _)| *name == code.as_str())
        .map(|(_, scan, fixed)| Key {
            scan: *scan,
            fixed: (*fixed != 0).then_some(*fixed),
        })
}

/// The virtual key `code` produces in keyboard layout `layout`.
pub fn virtual_key(code: &KeyCode, layout: HKL) -> Option<u16> {
    let key = key(code)?;
    if let Some(fixed) = key.fixed {
        return Some(fixed);
    }
    // SAFETY: a plain lookup; an unknown layout handle yields 0.
    let vk = unsafe { MapVirtualKeyExW(u32::from(key.scan), MAPVK_VSC_TO_VK_EX, Some(layout)) };
    u16::try_from(vk).ok().filter(|vk| *vk != 0)
}

/// The left-hand key for each modifier.
pub fn modifier_key(modifier: Modifier) -> VIRTUAL_KEY {
    match modifier {
        Modifier::Shift => VK_LSHIFT,
        Modifier::Ctrl => VK_LCONTROL,
        Modifier::Alt => VK_LMENU,
        Modifier::Meta => VK_LWIN,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digits_and_letters_follow_the_layout() {
        let one = key(&KeyCode::new("Digit1")).unwrap();
        assert_eq!(one.scan, 0x02);
        assert_eq!(one.fixed, None);
        assert!(!one.is_extended());
    }

    #[test]
    fn arrows_are_extended_keys_with_fixed_virtual_keys() {
        let left = key(&KeyCode::new("ArrowLeft")).unwrap();
        assert!(left.is_extended());
        assert_eq!(left.fixed, Some(0x25));
        // The numeric keypad's 4 shares the scan code without the prefix.
        assert_eq!(key(&KeyCode::new("Numpad4")).unwrap().fixed, Some(0x64));
    }

    #[test]
    fn every_mapped_code_is_a_known_shortcut_key() {
        assert!(KEYS.iter().all(|(name, _, _)| KeyCode::new(*name).is_known()));
        let names: std::collections::BTreeSet<&str> = KEYS.iter().map(|(name, _, _)| *name).collect();
        assert_eq!(names.len(), KEYS.len());
        assert_eq!(key(&KeyCode::new("Help")), None);
    }

    #[test]
    fn scan_codes_are_unique() {
        let mut scans: Vec<u16> = KEYS.iter().map(|(_, scan, _)| *scan).collect();
        scans.sort_unstable();
        scans.dedup();
        assert_eq!(scans.len(), KEYS.len());
    }
}
