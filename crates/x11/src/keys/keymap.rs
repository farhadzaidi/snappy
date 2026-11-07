//! The server's keyboard mapping: which keycode a shortcut key is, and which modifier
//! bits mean Alt, Super, and Num Lock.

use std::collections::BTreeMap;

use snappy_core::keys::{KeyCode, Modifier, Mods};

use super::codes::{self, keysym};

pub const SHIFT_MASK: u16 = 1 << 0;
pub const LOCK_MASK: u16 = 1 << 1;
pub const CONTROL_MASK: u16 = 1 << 2;
const MOD1_MASK: u16 = 1 << 3;
const MOD2_MASK: u16 = 1 << 4;
const MOD4_MASK: u16 = 1 << 6;
/// Modifier rows in `GetModifierMapping` order: Shift, Lock, Control, Mod1 to Mod5.
const MODIFIER_ROWS: usize = 8;

/// The keysyms of every keycode, as `GetKeyboardMapping` returns them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Keymap {
    min_keycode: u8,
    per_keycode: usize,
    keysyms: Vec<u32>,
}

impl Keymap {
    pub fn new(min_keycode: u8, per_keycode: u8, keysyms: Vec<u32>) -> Keymap {
        Keymap {
            min_keycode,
            per_keycode: usize::from(per_keycode),
            keysyms,
        }
    }

    pub fn keysyms(&self, keycode: u8) -> &[u32] {
        let Some(index) = keycode.checked_sub(self.min_keycode) else {
            return &[];
        };
        let start = usize::from(index) * self.per_keycode;
        self.keysyms.get(start..start + self.per_keycode).unwrap_or(&[])
    }

    fn keycodes(&self) -> impl Iterator<Item = u8> + '_ {
        let count = self.keysyms.len().checked_div(self.per_keycode).unwrap_or(0);
        (0..count).filter_map(|offset| u8::try_from(offset).ok()?.checked_add(self.min_keycode))
    }

    /// The keycode producing `keysym`, preferring unshifted positions.
    pub fn find(&self, keysym: u32) -> Option<u8> {
        (0..self.per_keycode).find_map(|level| {
            self.keycodes()
                .find(|&keycode| self.keysyms(keycode).get(level) == Some(&keysym))
        })
    }

    /// Evdev keycodes hold when the server places Escape and Left where evdev does, as
    /// every libinput- or evdev-driven server does.
    pub fn is_evdev(&self) -> bool {
        let has = |keycode: u8, keysym: u32| self.keysyms(keycode).first() == Some(&keysym);
        has(9, keysym::ESCAPE) && has(113, keysym::LEFT)
    }

    /// The keycode of a physical key: its evdev keycode on evdev servers, otherwise the
    /// key producing its US keysym.
    pub fn keycode_for(&self, code: &KeyCode) -> Option<u8> {
        if self.is_evdev() {
            return codes::evdev_keycode(code.as_str());
        }
        self.find(codes::us_keysym(code.as_str())?)
    }
}

/// The state bits each modifier key sets, from the modifier mapping.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModifierKeys(BTreeMap<u8, u16>);

impl ModifierKeys {
    /// Reads `keycodes`, eight rows of `per_modifier` keycodes each, zero for unused slots.
    pub fn from_mapping(keycodes: &[u8], per_modifier: usize) -> ModifierKeys {
        let mut bits = BTreeMap::new();
        for row in 0..MODIFIER_ROWS {
            let start = row * per_modifier;
            let Some(keys) = keycodes.get(start..start + per_modifier) else {
                break;
            };
            for keycode in keys.iter().filter(|keycode| **keycode != 0) {
                *bits.entry(*keycode).or_insert(0) |= 1u16 << row;
            }
        }
        ModifierKeys(bits)
    }

    pub fn is_modifier(&self, keycode: u8) -> bool {
        self.0.contains_key(&keycode)
    }

    /// The state mask while `held` keys are down.
    pub fn mask<'a>(&self, held: impl IntoIterator<Item = &'a u8>) -> u16 {
        held.into_iter()
            .filter_map(|keycode| self.0.get(keycode))
            .fold(0, |mask, bits| mask | bits)
    }
}

/// Which modifier bits Alt, Super, and Num Lock occupy on this server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModMasks {
    pub alt: u16,
    pub super_: u16,
    pub num_lock: u16,
}

impl Default for ModMasks {
    /// The conventional assignment, used when the mapping names none.
    fn default() -> Self {
        ModMasks {
            alt: MOD1_MASK,
            super_: MOD4_MASK,
            num_lock: MOD2_MASK,
        }
    }
}

impl ModMasks {
    /// Reads masks from the modifier mapping: `keycodes` holds eight rows of
    /// `per_modifier` keycodes each, zero for unused slots.
    pub fn from_mapping(keycodes: &[u8], per_modifier: usize, keymap: &Keymap) -> ModMasks {
        let defaults = ModMasks::default();
        let mask_holding = |wanted: &[u32]| {
            (3..MODIFIER_ROWS).find_map(|row| {
                let start = row * per_modifier;
                let keys = keycodes.get(start..start + per_modifier)?;
                keys.iter()
                    .filter(|keycode| **keycode != 0)
                    .any(|keycode| keymap.keysyms(*keycode).iter().any(|sym| wanted.contains(sym)))
                    .then_some(1u16 << row)
            })
        };
        ModMasks {
            alt: mask_holding(&[keysym::ALT_L, keysym::ALT_R, keysym::META_L, keysym::META_R])
                .unwrap_or(defaults.alt),
            super_: mask_holding(&[keysym::SUPER_L, keysym::SUPER_R]).unwrap_or(defaults.super_),
            num_lock: mask_holding(&[keysym::NUM_LOCK]).unwrap_or(defaults.num_lock),
        }
    }

    /// The modifiers held in an X state mask.
    pub fn mods(&self, state: u16) -> Mods {
        [
            (SHIFT_MASK, Modifier::Shift),
            (CONTROL_MASK, Modifier::Ctrl),
            (self.alt, Modifier::Alt),
            (self.super_, Modifier::Meta),
        ]
        .into_iter()
        .filter(|(mask, _)| state & mask != 0)
        .map(|(_, modifier)| modifier)
        .collect()
    }

    /// The X state mask for `mods`.
    pub fn mask(&self, mods: Mods) -> u16 {
        mods.iter()
            .map(|modifier| match modifier {
                Modifier::Shift => SHIFT_MASK,
                Modifier::Ctrl => CONTROL_MASK,
                Modifier::Alt => self.alt,
                Modifier::Meta => self.super_,
            })
            .fold(0, |mask, bit| mask | bit)
    }

    /// The bits a shortcut matches on; Caps Lock, Num Lock, and the rest are ignored.
    pub fn relevant(&self, state: u16) -> u16 {
        state & (SHIFT_MASK | CONTROL_MASK | self.alt | self.super_)
    }

    /// Lock combinations a grab repeats for, so shortcuts work with Num Lock or Caps
    /// Lock on.
    pub fn lock_variants(&self) -> Vec<u16> {
        let mut variants = vec![0, LOCK_MASK, self.num_lock, LOCK_MASK | self.num_lock];
        variants.dedup();
        variants
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small keymap with two keysyms per keycode, starting at keycode 8.
    fn keymap(entries: &[(u8, u32, u32)]) -> Keymap {
        let mut keysyms = vec![0u32; 2 * 248];
        for &(keycode, base, shifted) in entries {
            let index = usize::from(keycode - 8) * 2;
            keysyms[index] = base;
            keysyms[index + 1] = shifted;
        }
        Keymap::new(8, 2, keysyms)
    }

    fn evdev() -> Keymap {
        keymap(&[
            (9, keysym::ESCAPE, 0),
            (113, keysym::LEFT, 0),
            (38, 0x61, 0x41),
            (64, keysym::ALT_L, keysym::META_L),
            (133, keysym::SUPER_L, 0),
            (77, keysym::NUM_LOCK, 0),
        ])
    }

    #[test]
    fn evdev_servers_use_physical_keycodes() {
        let map = evdev();
        assert!(map.is_evdev());
        assert_eq!(map.keycode_for(&KeyCode::new("KeyQ")), Some(24));
        assert_eq!(map.keycode_for(&KeyCode::new("ArrowLeft")), Some(113));
    }

    #[test]
    fn other_servers_look_up_the_us_keysym() {
        // The legacy xfree86 keycodes put Left at 100.
        let map = keymap(&[(9, keysym::ESCAPE, 0), (100, keysym::LEFT, 0), (10, 0x31, 0x21)]);
        assert!(!map.is_evdev());
        assert_eq!(map.keycode_for(&KeyCode::new("ArrowLeft")), Some(100));
        assert_eq!(map.keycode_for(&KeyCode::new("Digit1")), Some(10));
        assert_eq!(map.keycode_for(&KeyCode::new("KeyZ")), None);
    }

    #[test]
    fn keysym_lookup_prefers_unshifted_positions() {
        let map = keymap(&[(20, 0x26, 0x31), (30, 0x31, 0x21)]);
        assert_eq!(map.find(0x31), Some(30));
        assert_eq!(map.find(0x26), Some(20));
        assert_eq!(map.find(0x99), None);
    }

    #[test]
    fn keysyms_outside_the_map_are_empty() {
        let map = Keymap::new(8, 2, vec![0xff1b, 0, 0x31, 0x21]);
        assert!(map.keysyms(7).is_empty());
        assert!(map.keysyms(10).is_empty());
        assert_eq!(map.keysyms(9), &[0x31, 0x21]);
    }

    fn mapping(rows: [[u8; 2]; 8]) -> Vec<u8> {
        rows.concat()
    }

    #[test]
    fn modifier_masks_come_from_the_modifier_mapping() {
        // Alt on Mod1, Num Lock on Mod2, Super on Mod4: the usual layout.
        let usual = mapping([
            [50, 0],
            [66, 0],
            [37, 0],
            [64, 0],
            [77, 0],
            [0, 0],
            [133, 0],
            [0, 0],
        ]);
        assert_eq!(ModMasks::from_mapping(&usual, 2, &evdev()), ModMasks::default());

        // Super moved to Mod3 and Num Lock to Mod5.
        let moved = mapping([
            [50, 0],
            [66, 0],
            [37, 0],
            [64, 0],
            [0, 0],
            [133, 0],
            [0, 0],
            [77, 0],
        ]);
        let masks = ModMasks::from_mapping(&moved, 2, &evdev());
        assert_eq!(masks.super_, 1 << 5);
        assert_eq!(masks.num_lock, 1 << 7);
        assert_eq!(masks.alt, 1 << 3);
    }

    #[test]
    fn held_modifier_keys_give_the_state_mask_their_rows_name() {
        // Both Shift keys on Shift, Control on Control, Alt on Mod1, Super on Mod4.
        let keys = ModifierKeys::from_mapping(
            &mapping([
                [50, 62],
                [66, 0],
                [37, 0],
                [64, 0],
                [77, 0],
                [0, 0],
                [133, 0],
                [0, 0],
            ]),
            2,
        );
        assert!(keys.is_modifier(62) && !keys.is_modifier(38));
        assert_eq!(keys.mask(&[50, 62]), SHIFT_MASK);
        assert_eq!(keys.mask(&[37, 64, 38]), CONTROL_MASK | MOD1_MASK);
        let masks = ModMasks::default();
        let mods: Mods = [Modifier::Shift, Modifier::Meta].into_iter().collect();
        assert_eq!(masks.mods(keys.mask(&[62, 133])), mods);
        assert_eq!(keys.mask(&[]), 0);
    }

    #[test]
    fn missing_modifiers_fall_back_to_convention() {
        let empty = vec![0u8; 16];
        assert_eq!(ModMasks::from_mapping(&empty, 2, &evdev()), ModMasks::default());
    }

    #[test]
    fn state_masks_translate_both_ways() {
        let masks = ModMasks::default();
        let hyper: Mods = [Modifier::Ctrl, Modifier::Alt, Modifier::Meta]
            .into_iter()
            .collect();
        assert_eq!(masks.mask(hyper), 0x4 | 0x8 | 0x40);
        assert_eq!(masks.mods(0x4 | 0x8 | 0x40 | LOCK_MASK | MOD2_MASK), hyper);
        assert_eq!(masks.relevant(0x4 | LOCK_MASK | MOD2_MASK | 0x1), 0x5);
    }

    #[test]
    fn grabs_repeat_for_caps_lock_and_num_lock() {
        assert_eq!(ModMasks::default().lock_variants(), vec![0, 0x2, 0x10, 0x12]);
    }
}
