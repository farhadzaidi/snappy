//! Shortcuts macOS keeps for itself, such as Mission Control's Control-arrow Space switching
//! (System Settings → Keyboard → Keyboard Shortcuts). Carbon lets Snappy register the same
//! keys, but macOS takes the presses first, so these count as unavailable.

use objc2::runtime::AnyObject;
use objc2_foundation::{NSArray, NSDictionary, NSNumber, NSString, NSUserDefaults, ns_string};

use crate::keycodes::CarbonChord;

/// The modifier bits of a symbolic hotkey: Shift, Control, Option, Command. The function
/// and keypad bits that arrow keys carry don't count.
const MODIFIER_MASK: u64 = 0x1E_0000;
/// Carbon's modifier bits for Shift, Control, Option, and Command, in that order.
const CARBON: [u32; 4] = [1 << 9, 1 << 12, 1 << 11, 1 << 8];
/// The matching symbolic hotkey bits.
const SYMBOLIC: [u64; 4] = [0x2_0000, 0x4_0000, 0x8_0000, 0x10_0000];

/// One of macOS's own shortcuts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemShortcut {
    pub id: u32,
    pub enabled: bool,
    pub key_code: u32,
    pub modifiers: u64,
}

/// macOS's defaults, which apply until the user changes them: Mission Control,
/// application windows, and moving left or right a Space, on Control with the arrows; and
/// Accessibility's increase and decrease contrast on Control-Option-Command with period
/// and comma, which ship turned off.
const DEFAULTS: [SystemShortcut; 6] = [
    SystemShortcut {
        id: 32,
        enabled: true,
        key_code: 126,
        modifiers: 0x84_0000,
    },
    SystemShortcut {
        id: 33,
        enabled: true,
        key_code: 125,
        modifiers: 0x84_0000,
    },
    SystemShortcut {
        id: 79,
        enabled: true,
        key_code: 123,
        modifiers: 0x84_0000,
    },
    SystemShortcut {
        id: 81,
        enabled: true,
        key_code: 124,
        modifiers: 0x84_0000,
    },
    SystemShortcut {
        id: 25,
        enabled: false,
        key_code: 47,
        modifiers: 0x1C_0000,
    },
    SystemShortcut {
        id: 26,
        enabled: false,
        key_code: 43,
        modifiers: 0x1C_0000,
    },
];

/// True when an enabled macOS shortcut uses the same key and modifiers as `chord`.
pub fn is_taken(chord: CarbonChord, shortcuts: &[SystemShortcut]) -> bool {
    let wanted = symbolic_modifiers(chord.modifiers);
    shortcuts.iter().any(|shortcut| {
        shortcut.enabled
            && shortcut.key_code == chord.key_code
            && shortcut.modifiers & MODIFIER_MASK == wanted
    })
}

fn symbolic_modifiers(carbon: u32) -> u64 {
    CARBON
        .iter()
        .zip(SYMBOLIC)
        .filter(|(bit, _)| carbon & **bit != 0)
        .map(|(_, symbolic)| symbolic)
        .sum()
}

/// macOS's shortcuts as they stand: the user's settings over the defaults.
pub fn current() -> Vec<SystemShortcut> {
    let mut shortcuts = DEFAULTS.to_vec();
    for configured in configured() {
        shortcuts.retain(|shortcut| shortcut.id != configured.id);
        shortcuts.push(configured);
    }
    shortcuts
}

/// The shortcuts the user's settings mention, from `com.apple.symbolichotkeys`.
fn configured() -> Vec<SystemShortcut> {
    let domain = NSUserDefaults::standardUserDefaults()
        .persistentDomainForName(ns_string!("com.apple.symbolichotkeys"));
    let Some(table) = domain
        .and_then(|domain| domain.objectForKey(ns_string!("AppleSymbolicHotKeys")))
        .and_then(|table| table.downcast::<NSDictionary>().ok())
    else {
        return Vec::new();
    };
    table
        .allKeys()
        .iter()
        .filter_map(|key| {
            let id = key.downcast_ref::<NSString>()?.to_string().parse().ok()?;
            parse(id, &*table.objectForKey(&key)?)
        })
        .collect()
}

/// One entry: `{ enabled, value: { parameters: (character, key code, modifiers) } }`.
fn parse(id: u32, entry: &AnyObject) -> Option<SystemShortcut> {
    let entry = entry.downcast_ref::<NSDictionary>()?;
    let enabled = integer(&*entry.objectForKey(ns_string!("enabled"))?)? != 0;
    let value = entry.objectForKey(ns_string!("value"))?;
    let parameters = value
        .downcast_ref::<NSDictionary>()?
        .objectForKey(ns_string!("parameters"))?;
    let parameters = parameters.downcast_ref::<NSArray>()?.to_vec();
    let key_code = u32::try_from(integer(parameters.get(1)?)?).ok()?;
    let modifiers = u64::try_from(integer(parameters.get(2)?)?).ok()?;
    Some(SystemShortcut {
        id,
        enabled,
        key_code,
        modifiers,
    })
}

/// A number, also accepted when stored as text.
fn integer(value: &AnyObject) -> Option<i64> {
    if let Some(number) = value.downcast_ref::<NSNumber>() {
        return Some(number.longLongValue());
    }
    value.downcast_ref::<NSString>()?.to_string().trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{Combo, Modifier};

    use super::*;

    fn carbon(code: &str, mods: &[Modifier]) -> CarbonChord {
        CarbonChord::from_combo(&Combo::new(code, mods.iter().copied())).unwrap()
    }

    #[test]
    fn mission_control_keeps_control_arrows_by_default() {
        assert!(is_taken(carbon("ArrowRight", &[Modifier::Ctrl]), &DEFAULTS));
        assert!(is_taken(carbon("ArrowLeft", &[Modifier::Ctrl]), &DEFAULTS));
        assert!(!is_taken(
            carbon("ArrowRight", &[Modifier::Ctrl, Modifier::Alt, Modifier::Meta]),
            &DEFAULTS
        ));
        assert!(!is_taken(carbon("Digit1", &[Modifier::Ctrl]), &DEFAULTS));
    }

    #[test]
    fn mission_control_leaves_the_default_workspace_steps_free() {
        let settings = snappy_core::settings::Settings::defaults(snappy_core::keys::Platform::MacOs);
        for (_, chord) in settings.workspaces.steps() {
            let carbon = CarbonChord::from_combo(&chord.combo()).unwrap();
            assert!(!is_taken(carbon, &DEFAULTS), "{chord}");
        }
    }

    #[test]
    fn turning_on_the_contrast_shortcuts_takes_only_the_previous_move_window_step() {
        let settings = snappy_core::settings::Settings::defaults(snappy_core::keys::Platform::MacOs);
        let step = |chord: &Option<snappy_core::keys::KeyChord>| {
            CarbonChord::from_combo(&chord.as_ref().unwrap().combo()).unwrap()
        };
        let previous = step(&settings.workspaces.move_previous);
        let next = step(&settings.workspaces.move_next);
        let mut turned_on = DEFAULTS;
        turned_on[4].enabled = true;
        turned_on[5].enabled = true;
        assert!(!is_taken(previous, &DEFAULTS));
        assert!(is_taken(previous, &turned_on), "increase contrast is on period");
        assert!(!is_taken(next, &turned_on));
    }

    #[test]
    fn the_space_bar_shortcuts_macos_ships_leave_the_nearest_empty_zone_default_free() {
        let space = |id: u32, modifiers: u64| SystemShortcut {
            id,
            enabled: true,
            key_code: 49,
            modifiers,
        };
        // Spotlight, Finder search, and the input source switches as they ship, plus
        // Control-Command-Space, which opens the emoji picker. Each uses fewer modifiers
        // than the Snappy key.
        let shipped = [
            space(64, 0x10_0000),
            space(65, 0x18_0000),
            space(60, 0x4_0000),
            space(61, 0xC_0000),
            space(0, 0x14_0000),
        ];
        assert!(is_taken(carbon("Space", &[Modifier::Meta]), &shipped));
        assert!(is_taken(
            carbon("Space", &[Modifier::Ctrl, Modifier::Meta]),
            &shipped
        ));

        let settings = snappy_core::settings::Settings::defaults(snappy_core::keys::Platform::MacOs);
        let nearest_empty = settings.navigation.nearest_empty.unwrap().combo();
        let carbon = CarbonChord::from_combo(&nearest_empty).unwrap();
        assert!(!is_taken(carbon, &shipped));
        assert!(!is_taken(carbon, &DEFAULTS));
    }

    #[test]
    fn shortcuts_turned_off_in_system_settings_are_free() {
        let mut turned_off = DEFAULTS;
        turned_off[3].enabled = false;
        assert!(!is_taken(carbon("ArrowRight", &[Modifier::Ctrl]), &turned_off));
    }

    #[test]
    fn desktop_shortcuts_match_on_their_modifiers() {
        let desktop_two = [SystemShortcut {
            id: 119,
            enabled: true,
            key_code: 19,
            modifiers: 0x4_0000,
        }];
        assert!(is_taken(carbon("Digit2", &[Modifier::Ctrl]), &desktop_two));
        assert!(!is_taken(
            carbon("Digit2", &[Modifier::Ctrl, Modifier::Shift]),
            &desktop_two
        ));
    }
}
