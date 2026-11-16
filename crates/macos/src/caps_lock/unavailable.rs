//! Caps Lock in the Mac App Store build. The App Sandbox doesn't let an app change the HID
//! event system's key mapping or watch the keyboard with an active event tap. The build says
//! so up front (`UNAVAILABLE`), every request is still refused, and nothing is ever changed
//! that would need undoing.

use snappy_core::keys::Mods;
use snappy_platform::CapsLockUndo;

use crate::messages;

/// Why Caps Lock can't be the Snappy key in this build, declared at start.
pub const UNAVAILABLE: Option<&str> = Some(messages::CAPS_LOCK_SANDBOXED);

/// Refuses: this build can't make Caps Lock the Snappy key.
pub fn hold_as(_modifiers: Mods) -> Result<CapsLockUndo, String> {
    Err(messages::CAPS_LOCK_SANDBOXED.to_owned())
}

pub fn restore() {}

/// This build never changes Caps Lock, and its sandbox has no other build's state file.
pub fn undo(_undo: &CapsLockUndo) {}

pub fn unmap() {}

pub fn stop() {}

/// Never asked for, since Caps Lock is never held.
pub fn toggle() -> Result<(), String> {
    Err("Caps Lock is never the Snappy key in the Mac App Store build".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holding_caps_lock_is_refused_with_the_message_the_build_declares() {
        assert_eq!(UNAVAILABLE, Some(messages::CAPS_LOCK_SANDBOXED));
        assert_eq!(hold_as(Mods::NONE).unwrap_err(), messages::CAPS_LOCK_SANDBOXED);
    }
}
