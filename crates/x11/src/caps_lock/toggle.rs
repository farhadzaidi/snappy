//! Turning Caps Lock on or off for a tap.

use x11rb::protocol::xkb::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ModMask;

use crate::keymap::LOCK_MASK;
use crate::shared::Shared;

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
