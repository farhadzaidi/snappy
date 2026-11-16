//! Caps Lock as the Snappy key on X11.
//!
//! Holding works through the server's own keyboard description (XKB): the Caps Lock key's
//! definition is replaced, for this session only, by one whose action sets the Snappy key's
//! modifiers while held, without repeating and without locking, so Snappy's grabs and every
//! other client see the modifiers themselves. Its symbol becomes `VoidSymbol`, which no
//! client acts on. The definition the key had is kept to put back exactly.
//!
//! Presses are learned without watching the keyboard at idle: a passive grab on the Caps
//! Lock key alone reports its press, and the keyboard grab that press starts is let go at
//! once. Only while Caps Lock is down are raw key and button presses selected, to notice
//! that something else was used and when Caps Lock comes up; which key it was is never
//! kept. A tap turns Caps Lock on or off through XKB's lock state, which its light follows.

use serde::{Deserialize, Serialize};
use snappy_core::keys::Mods;
use snappy_platform::{CapsLockInput, CapsLockUndo};
use x11rb::connection::Connection;
use x11rb::protocol::xkb::{
    self, ConnectionExt as _, Explicit, GetMapReply, KeyModMap, KeySymMap, MapPart, SASetMods, SAType,
    SetExplicit, SetMapAux, SetMapAuxKeyActions, SetMapFlags,
};
use x11rb::protocol::xproto::{
    AtomEnum, AutoRepeatMode, ChangeKeyboardControlAux, ConnectionExt as _, GrabMode, ModMask,
};
use x11rb::wrapper::ConnectionExt as _;
use x11rb::x11_utils::{Serialize as _, TryParse};

use crate::input;
use crate::keys::keymap::LOCK_MASK;
use crate::messages;
use crate::shared::Shared;
use crate::x::{self, lock};

const CAPS_LOCK: u32 = 0xffe5;
const VOID_SYMBOL: u32 = 0x00ff_ffff;
/// The first canonical key type, one level.
const ONE_LEVEL: u8 = 0;

/// What Caps Lock is doing for Snappy.
#[derive(Debug, Default)]
pub struct CapsLockState {
    held: Option<Held>,
    /// Caps Lock is down: its press arrived and its release hasn't.
    down: bool,
    /// Something else was used during this press, and the engine was told.
    other_input: bool,
}

/// Caps Lock held as the Snappy key: its key and the definition to put back, which is
/// also what a later run needs to put it back.
#[derive(Debug, Serialize, Deserialize)]
struct Held {
    keycode: u8,
    original: KeyDefinition,
}

/// One key's definition: what XKB keeps for it, as `GetMap` reports it, plus whether it
/// repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyDefinition {
    types: [u8; 4],
    group_info: u8,
    width: u8,
    syms: Vec<u32>,
    actions: Vec<[u8; 8]>,
    explicit: u8,
    modmap: u8,
    repeats: bool,
}

impl KeyDefinition {
    /// A plain Caps Lock: its first symbol is Caps_Lock and it is a Lock key.
    fn is_plain_caps_lock(&self) -> bool {
        self.syms.first() == Some(&CAPS_LOCK) && u16::from(self.modmap) & LOCK_MASK != 0
    }

    /// The definition Snappy gives Caps Lock, which nothing else uses.
    fn is_held(&self) -> bool {
        self.syms == [VOID_SYMBOL] && self.explicit & u8::from(Explicit::INTERPRET) != 0
    }

    /// Caps Lock held as the modifiers in `mask`, real modifier bits: one level, no symbol,
    /// setting them while down, kept from the server's own rules (`INTERPRET`), not
    /// repeating, and listed under those modifiers, so clients that follow modifier keys
    /// count it.
    fn held_as(mask: u16) -> KeyDefinition {
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

/// Why Caps Lock can't be the Snappy key: one of `options` remaps it, or `caps` isn't a
/// plain Caps Lock, as after `xmodmap`.
fn refusal(options: &str, caps: Option<&KeyDefinition>) -> Option<String> {
    if let Some(reason) = snappy_linux::xkb::options_refusal(options) {
        return Some(reason);
    }
    (!caps.is_some_and(KeyDefinition::is_plain_caps_lock))
        .then(|| messages::CAPS_LOCK_OTHER_SETTING.to_owned())
}

/// The XKB options the session's keymap was built with.
fn options(shared: &Shared) -> String {
    x::bytes(
        &shared.conn,
        shared.screen.root,
        shared.atoms._XKB_RULES_NAMES,
        AtomEnum::STRING,
    )
    .map(|names| {
        // Rules, model, layouts, variants, options, each ended by a zero byte.
        names
            .split(|byte| *byte == 0)
            .nth(4)
            .map(|options| String::from_utf8_lossy(options).into_owned())
            .unwrap_or_default()
    })
    .unwrap_or_default()
}

/// The key that types Caps_Lock.
fn caps_lock_keycode(shared: &Shared) -> Option<u8> {
    lock(&shared.keyboard).keymap.find(CAPS_LOCK)
}

fn read(shared: &Shared, keycode: u8) -> Option<KeyDefinition> {
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
fn write(shared: &Shared, keycode: u8, key: &KeyDefinition) -> Result<(), String> {
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

/// The real modifier bits for `modifiers` in this session's modifier mapping.
fn real_modifiers(shared: &Shared, modifiers: Mods) -> u16 {
    lock(&shared.keyboard).masks.mask(modifiers)
}

fn undo_record(held: &Held) -> CapsLockUndo {
    CapsLockUndo::of(held).unwrap_or_default()
}

/// Hold Caps Lock as `modifiers`: refuse when the session already remaps it, else change
/// its definition and listen for its presses.
pub fn hold(shared: &Shared, modifiers: Mods) -> Result<CapsLockUndo, String> {
    // The keymap may have changed a moment ago, as when a crashed run's change was just
    // undone or a keymap was just loaded, before the server's notice reached the event
    // thread, so the keyboard is read afresh rather than taken from the cache.
    shared.refresh_keyboard();
    let mask = real_modifiers(shared, modifiers);
    let mut state = lock(&shared.caps_lock);
    if let Some(held) = state.held.as_ref() {
        if let Err(error) = write(shared, held.keycode, &KeyDefinition::held_as(mask)) {
            drop(state);
            restore(shared);
            return Err(error);
        }
        return Ok(undo_record(held));
    }

    let keycode = caps_lock_keycode(shared);
    let current = keycode.and_then(|keycode| read(shared, keycode));
    if let Some(reason) = refusal(&options(shared), current.as_ref()) {
        return Err(reason);
    }
    let (Some(keycode), Some(original)) = (keycode, current) else {
        return Err(messages::CAPS_LOCK_OTHER_SETTING.to_owned());
    };
    if let Err(error) = write(shared, keycode, &KeyDefinition::held_as(mask)) {
        let _ = write(shared, keycode, &original);
        return Err(error);
    }
    if !grab(shared, keycode) {
        let _ = write(shared, keycode, &original);
        return Err(messages::CAPS_LOCK_TAKEN.to_owned());
    }
    let held = Held { keycode, original };
    let undo = undo_record(&held);
    *state = CapsLockState {
        held: Some(held),
        ..CapsLockState::default()
    };
    Ok(undo)
}

/// Give Caps Lock its own definition back. A keymap loaded since, which already has it,
/// is left alone.
pub fn restore(shared: &Shared) {
    let held = std::mem::take(&mut *lock(&shared.caps_lock)).held;
    let Some(held) = held else {
        return;
    };
    stop_listening(shared, held.keycode);
    put_back(shared, held.keycode, &held.original);
}

/// Let go of Caps Lock's grab and stop following its press.
fn stop_listening(shared: &Shared, keycode: u8) {
    if let Ok(cookie) = shared.conn.ungrab_key(keycode, shared.screen.root, ModMask::ANY) {
        cookie.ignore_error();
    }
    input::set(shared, |monitoring| monitoring.caps_lock_down = false);
}

/// Put back what a run that ended without restoring left, from its `hold`'s record.
pub fn undo(shared: &Shared, undo: &CapsLockUndo) {
    if let Ok(left) = undo.read::<Held>() {
        put_back(shared, left.keycode, &left.original);
    }
}

/// Gives `keycode` its `original` definition while it still has Snappy's.
fn put_back(shared: &Shared, keycode: u8, original: &KeyDefinition) {
    if read(shared, keycode).is_some_and(|current| current.is_held()) {
        let _ = write(shared, keycode, original);
    }
    let _ = shared.conn.sync();
}

/// A passive grab on Caps Lock alone, whatever modifiers or locks are on; another client
/// holding it refuses the grab.
fn grab(shared: &Shared, keycode: u8) -> bool {
    shared
        .conn
        .grab_key(
            false,
            shared.screen.root,
            ModMask::ANY,
            keycode,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        )
        .ok()
        .and_then(|cookie| cookie.check().ok())
        .is_some()
}

/// Turn Caps Lock on or off by flipping XKB's Lock lock, as the key itself does.
pub fn toggle(shared: &Shared) -> Result<(), String> {
    let state = shared
        .conn
        .xkb_get_state(xkb::ID::USE_CORE_KBD.into())
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| error.to_string())?;
    let locks = if u16::from(state.locked_mods) & LOCK_MASK != 0 {
        0
    } else {
        LOCK_MASK
    };
    shared
        .conn
        .xkb_latch_lock_state(
            xkb::ID::USE_CORE_KBD.into(),
            ModMask::LOCK,
            ModMask::from(locks),
            false,
            xkb::Group::M1,
            ModMask::from(0u16),
            false,
            0,
        )
        .map_err(|error| error.to_string())?
        .check()
        .map_err(|error| error.to_string())
}

/// A key press the event thread received. Caps Lock's own, through its grab, starts a
/// press: raw key and button presses are selected before the keyboard grab the press
/// started is let go, so its release is caught either way. Other presses arrive here only
/// while that grab lasts.
pub fn key_pressed(shared: &Shared, keycode: u8) -> Option<CapsLockInput> {
    let mut state = lock(&shared.caps_lock);
    let caps_lock = state.held.as_ref()?.keycode;
    if keycode != caps_lock {
        return other_input(&mut state);
    }
    if std::mem::replace(&mut state.down, true) {
        return None;
    }
    state.other_input = false;
    drop(state);
    // Without XInput 2 the grab stays until Caps Lock comes up, which ends it.
    if shared.xinput {
        input::set(shared, |monitoring| monitoring.caps_lock_down = true);
        if let Ok(cookie) = shared.conn.ungrab_keyboard(x11rb::CURRENT_TIME) {
            cookie.ignore_error();
        }
        let _ = shared.conn.flush();
    }
    Some(CapsLockInput::Pressed)
}

/// A key release, core or raw: Caps Lock's own ends the press.
pub fn key_released(shared: &Shared, keycode: u8) -> Option<CapsLockInput> {
    let mut state = lock(&shared.caps_lock);
    if state.held.as_ref()?.keycode != keycode || !std::mem::take(&mut state.down) {
        return None;
    }
    drop(state);
    input::set(shared, |monitoring| monitoring.caps_lock_down = false);
    Some(CapsLockInput::Released)
}

/// A raw key press while Caps Lock is down, or a raw button press.
pub fn raw_pressed(shared: &Shared, keycode: Option<u8>) -> Option<CapsLockInput> {
    let mut state = lock(&shared.caps_lock);
    let caps_lock = state.held.as_ref()?.keycode;
    if keycode == Some(caps_lock) {
        return None;
    }
    other_input(&mut state)
}

/// Something else was used while Caps Lock is down; the engine hears it once per press.
fn other_input(state: &mut CapsLockState) -> Option<CapsLockInput> {
    (state.down && !std::mem::replace(&mut state.other_input, true)).then_some(CapsLockInput::OtherInput)
}

/// The keyboard mapping changed. A keymap loaded over Snappy's change, as `setxkbmap` or a
/// layout switch does, gives Caps Lock a definition of its own, which stays: Snappy lets go
/// of Caps Lock and returns true, for the engine to hear it was lost and decide whether to
/// hold it again, which reads the new definition as the one to put back.
pub fn keymap_changed(shared: &Shared) -> bool {
    let mut state = lock(&shared.caps_lock);
    let Some(keycode) = state.held.as_ref().map(|held| held.keycode) else {
        return false;
    };
    if read(shared, keycode).is_none_or(|current| current.is_held()) {
        return false;
    }
    *state = CapsLockState::default();
    drop(state);
    stop_listening(shared, keycode);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROL_ALT_SUPER: u16 = 0x4 | 0x8 | 0x40;

    fn plain_caps_lock() -> KeyDefinition {
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

    #[test]
    fn a_caps_lock_option_or_another_remapping_is_refused_naming_it() {
        let caps = plain_caps_lock();
        assert_eq!(refusal("grp:alt_shift_toggle", Some(&caps)), None);
        assert_eq!(
            refusal("ctrl:nocaps,compose:caps", Some(&caps)),
            snappy_linux::xkb::options_refusal("ctrl:nocaps,compose:caps"),
            "the options are named first, even for a key that is still plain"
        );
        let mut xmodmapped = caps.clone();
        xmodmapped.syms = vec![0xffe3];
        assert_eq!(
            refusal("", Some(&xmodmapped)).as_deref(),
            Some(messages::CAPS_LOCK_OTHER_SETTING)
        );
        let mut not_a_lock = caps;
        not_a_lock.modmap = 0;
        assert!(refusal("", Some(&not_a_lock)).is_some());
        assert!(refusal("", None).is_some(), "no Caps Lock key at all");
    }

    #[test]
    fn the_record_for_a_later_run_reads_back_as_written() {
        let held = Held {
            keycode: 66,
            original: plain_caps_lock(),
        };
        let record = undo_record(&held);
        let read: Held = record.read().unwrap();
        assert_eq!((read.keycode, read.original), (66, plain_caps_lock()));
    }

    #[test]
    fn something_else_used_is_reported_once_per_press_and_only_while_down() {
        let mut state = CapsLockState::default();
        assert_eq!(other_input(&mut state), None);
        state.down = true;
        assert_eq!(other_input(&mut state), Some(CapsLockInput::OtherInput));
        assert_eq!(other_input(&mut state), None);
    }
}
