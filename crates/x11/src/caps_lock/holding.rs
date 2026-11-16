//! Holding Caps Lock as the Snappy key: changing its definition and grabbing it, and
//! putting both back.

use snappy_core::keys::Mods;
use snappy_linux::key_codes::keysym::CAPS_LOCK;
use snappy_platform::CapsLockUndo;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode, ModMask};
use x11rb::wrapper::ConnectionExt as _;

use super::key_definition::{KeyDefinition, read, write};
use super::refusal::{options, refusal};
use super::state::{CapsLockState, Held};
use crate::input;
use crate::messages;
use crate::shared::{Shared, lock};

/// The key that types Caps_Lock.
fn caps_lock_keycode(shared: &Shared) -> Option<u8> {
    lock(&shared.keyboard).keymap.find(CAPS_LOCK)
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
    use crate::caps_lock::key_definition::plain_caps_lock;

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
}
