//! Holding Caps Lock as the Snappy key: adding Snappy's pair to the user key mapping and
//! watching F18, and taking both away again.

use snappy_core::keys::Mods;
use snappy_platform::CapsLockUndo;

use super::hid::EventSystem;
use super::{key_mapping, modifier_keys, tap};
use crate::{main_thread, messages};

/// This build can hold Caps Lock, unless the user's keyboard settings already remap it.
pub const UNAVAILABLE: Option<&str> = None;

/// Make Caps Lock, held, stand for `modifiers`. Refuses, leaving Caps Lock as usual, when
/// the user's own key mapping or Modifier Keys setting already remaps it.
pub fn hold_as(modifiers: Mods) -> Result<CapsLockUndo, String> {
    let held = remap().and_then(|()| {
        main_thread::sync(move |_| tap::hold_as(modifiers))
            .then_some(())
            .ok_or_else(|| messages::CAPS_LOCK_UNWATCHED.to_owned())
    });
    match held {
        Ok(()) => Ok(CapsLockUndo(key_mapping::UNDO_NAME.into())),
        Err(reason) => {
            restore();
            Err(reason)
        }
    }
}

fn remap() -> Result<(), String> {
    let system = EventSystem::connect().ok_or(messages::CAPS_LOCK_UNMAPPABLE)?;
    if key_mapping::modifier_keys_remap_caps_lock(&modifier_keys::pairs()) {
        return Err(messages::CAPS_LOCK_IN_MODIFIER_KEYS.to_owned());
    }
    let mapping =
        key_mapping::with_snappy(&system.user_key_mapping()).ok_or(messages::CAPS_LOCK_IN_KEY_MAPPING)?;
    if !system.set_user_key_mapping(&mapping) {
        return Err(messages::CAPS_LOCK_UNMAPPABLE.to_owned());
    }
    Ok(())
}

/// Give Caps Lock back its usual behavior.
pub fn restore() {
    main_thread::sync(|_| tap::stop());
    unmap();
}

/// Undo the mapping a run that ended without restoring Caps Lock left in place.
pub fn undo(undo: &CapsLockUndo) {
    if undo.0.as_str().and_then(key_mapping::named).is_some() {
        unmap();
    }
}

/// Take Snappy's pair out of the user key mapping, if it is there.
pub fn unmap() {
    if let Some(system) = EventSystem::connect() {
        let mapping = system.user_key_mapping();
        let without = key_mapping::without_snappy(&mapping);
        if without != mapping {
            system.set_user_key_mapping(&without);
        }
    }
}
