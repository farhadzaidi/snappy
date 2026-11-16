//! Caps Lock's definition in the server's keyboard description (XKB): reading it, the one
//! Snappy gives it, and writing either.

use serde::{Deserialize, Serialize};
use snappy_linux::key_codes::keysym::{CAPS_LOCK, VOID_SYMBOL};
use x11rb::connection::Connection;
use x11rb::protocol::xkb::{
    self, ConnectionExt as _, Explicit, GetMapReply, KeyModMap, KeySymMap, MapPart, SASetMods, SAType,
    SetExplicit, SetMapAux, SetMapAuxKeyActions, SetMapFlags,
};
use x11rb::protocol::xproto::{AutoRepeatMode, ChangeKeyboardControlAux, ConnectionExt as _, ModMask};
use x11rb::x11_utils::{Serialize as _, TryParse};

use crate::keymap::LOCK_MASK;
use crate::messages;
use crate::shared::Shared;

/// The first canonical key type, one level.
const ONE_LEVEL: u8 = 0;

/// One key's definition: what XKB keeps for it, as `GetMap` reports it, plus whether it
/// repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyDefinition {
    pub(super) types: [u8; 4],
    pub(super) group_info: u8,
    pub(super) width: u8,
    pub(super) syms: Vec<u32>,
    pub(super) actions: Vec<[u8; 8]>,
    pub(super) explicit: u8,
    pub(super) modmap: u8,
    pub(super) repeats: bool,
}

impl KeyDefinition {
    /// A plain Caps Lock: its first symbol is Caps_Lock and it is a Lock key.
    pub(super) fn is_plain_caps_lock(&self) -> bool {
        self.syms.first() == Some(&CAPS_LOCK) && u16::from(self.modmap) & LOCK_MASK != 0
    }

    /// The definition Snappy gives Caps Lock, which nothing else uses.
    pub(super) fn is_held(&self) -> bool {
        self.syms == [VOID_SYMBOL] && self.explicit & u8::from(Explicit::INTERPRET) != 0
    }

    /// Caps Lock held as the modifiers in `mask`, real modifier bits: one level, no symbol,
    /// setting them while down, kept from the server's own rules (`INTERPRET`), not
    /// repeating, and listed under those modifiers, so clients that follow modifier keys
    /// count it.
    pub(super) fn held_as(mask: u16) -> KeyDefinition {
        let real = ModMask::from(mask);
        let set_mods = SASetMods {
            type_: SAType::SET_MODS,
            flags: xkb::SA::from(0u8),
            mask: real,
            real_mods: real,
            vmods_high: xkb::VModsHigh::from(0u8),
            vmods_low: xkb::VModsLow::from(0u8),
        };
        KeyDefinition {
            types: [ONE_LEVEL; 4],
            group_info: 1,
            width: 1,
            syms: vec![VOID_SYMBOL],
            actions: vec![set_mods.serialize()],
            explicit: u8::from(Explicit::INTERPRET) | u8::from(Explicit::KEY_TYPE1),
            modmap: (mask & 0xff) as u8,
            repeats: false,
        }
    }
}

/// The definition `keycode` has now.
pub(super) fn read(shared: &Shared, keycode: u8) -> Option<KeyDefinition> {
    let parts =
        MapPart::KEY_SYMS | MapPart::KEY_ACTIONS | MapPart::EXPLICIT_COMPONENTS | MapPart::MODIFIER_MAP;
    let reply: GetMapReply = shared
        .conn
        .xkb_get_map(
            xkb::ID::USE_CORE_KBD.into(),
            MapPart::from(0u16),
            parts,
            0,
            0,
            keycode,
            1,
            keycode,
            1,
            0,
            0,
            xkb::VMod::from(0u16),
            keycode,
            1,
            keycode,
            1,
            0,
            0,
        )
        .ok()?
        .reply()
        .ok()?;
    let map = reply.map;
    let syms = map.syms_rtrn?.into_iter().next()?;
    let actions = map.key_actions?;
    let count = usize::from(actions.acts_rtrn_count.first().copied().unwrap_or(0));
    let explicit = map
        .explicit_rtrn
        .unwrap_or_default()
        .into_iter()
        .find(|entry| entry.keycode == keycode)
        .map_or(0, |entry| u8::from(entry.explicit));
    let modmap = map
        .modmap_rtrn
        .unwrap_or_default()
        .into_iter()
        .find(|entry| entry.keycode == keycode)
        .map_or(0, |entry| (u16::from(entry.mods) & 0xff) as u8);
    let control = shared.conn.get_keyboard_control().ok()?.reply().ok()?;
    let repeats = control
        .auto_repeats
        .get(usize::from(keycode / 8))
        .is_some_and(|byte| byte & (1 << (keycode % 8)) != 0);
    Some(KeyDefinition {
        types: syms.kt_index,
        group_info: syms.group_info,
        width: syms.width,
        syms: syms.syms,
        actions: actions
            .acts_rtrn_acts
            .iter()
            .take(count)
            .map(|action| action.serialize())
            .collect(),
        explicit,
        modmap,
        repeats,
    })
}

/// Give `keycode` the definition `key`, waiting until the server has applied it.
pub(super) fn write(shared: &Shared, keycode: u8, key: &KeyDefinition) -> Result<(), String> {
    let failed = |error: &dyn std::fmt::Display| format!("{} ({error})", messages::CAPS_LOCK_FAILED);
    let actions = key
        .actions
        .iter()
        .map(|bytes| xkb::Action::try_parse(bytes).map(|(action, _)| action))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| failed(&error))?;
    // The server reads these two-byte lists in four-byte steps, which x11rb doesn't pad
    // them to, so the key's entry goes twice; applying it twice changes nothing.
    let explicit: Vec<SetExplicit> = (key.explicit != 0)
        .then(|| SetExplicit {
            keycode,
            explicit: Explicit::from(key.explicit),
        })
        .into_iter()
        .flat_map(|entry| [entry, entry])
        .collect();
    let modmap: Vec<KeyModMap> = (key.modmap != 0)
        .then(|| KeyModMap {
            keycode,
            mods: ModMask::from(u16::from(key.modmap)),
        })
        .into_iter()
        .flat_map(|entry| [entry, entry])
        .collect();
    let to_u8 = |count: usize| u8::try_from(count).map_err(|error| failed(&error));
    let (total_actions, total_explicit, total_modmap) = (
        to_u8(actions.len())?,
        to_u8(explicit.len())?,
        to_u8(modmap.len())?,
    );
    let total_syms = u16::try_from(key.syms.len()).map_err(|error| failed(&error))?;
    let values = SetMapAux {
        syms: Some(vec![KeySymMap {
            kt_index: key.types,
            group_info: key.group_info,
            width: key.width,
            syms: key.syms.clone(),
        }]),
        key_actions: Some(SetMapAuxKeyActions {
            actions_count: vec![total_actions],
            actions,
        }),
        explicit: Some(explicit),
        modmap: Some(modmap),
        ..SetMapAux::default()
    };
    let setup = shared.conn.setup();
    shared
        .conn
        .xkb_set_map(
            xkb::ID::USE_CORE_KBD.into(),
            SetMapFlags::from(0u16),
            setup.min_keycode,
            setup.max_keycode,
            0,
            0,
            keycode,
            1,
            total_syms,
            keycode,
            1,
            u16::from(total_actions),
            0,
            0,
            0,
            keycode,
            1,
            total_explicit,
            keycode,
            1,
            total_modmap,
            0,
            0,
            0,
            xkb::VMod::from(0u16),
            &values,
        )
        .map_err(|error| failed(&error))?
        .check()
        .map_err(|error| failed(&error))?;
    let repeat = if key.repeats {
        AutoRepeatMode::ON
    } else {
        AutoRepeatMode::OFF
    };
    let control = ChangeKeyboardControlAux::new()
        .key(u32::from(keycode))
        .auto_repeat_mode(repeat);
    shared
        .conn
        .change_keyboard_control(&control)
        .map_err(|error| failed(&error))?
        .check()
        .map_err(|error| failed(&error))
}

/// A plain Caps Lock, as a default keymap defines it.
#[cfg(test)]
pub(super) fn plain_caps_lock() -> KeyDefinition {
    let lock_mods = [3u8, 0, 2, 2, 0, 0, 0, 0];
    KeyDefinition {
        types: [ONE_LEVEL; 4],
        group_info: 1,
        width: 1,
        syms: vec![CAPS_LOCK],
        actions: vec![lock_mods],
        explicit: 0,
        modmap: LOCK_MASK as u8,
        repeats: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROL_ALT_SUPER: u16 = 0x4 | 0x8 | 0x40;

    #[test]
    fn held_caps_lock_sets_the_modifiers_without_a_symbol_a_lock_or_repeating() {
        let held = KeyDefinition::held_as(CONTROL_ALT_SUPER);
        assert_eq!(held.syms, [VOID_SYMBOL]);
        assert_eq!(held.actions, [[1, 0, 0x4c, 0x4c, 0, 0, 0, 0]]);
        assert_eq!(held.modmap, 0x4c, "listed under its modifiers, not Lock");
        assert!(!held.repeats);
        assert!(held.is_held() && !held.is_plain_caps_lock());
        assert!(!plain_caps_lock().is_held());
    }
}
