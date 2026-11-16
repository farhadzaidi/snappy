//! Caps Lock as the Snappy key: the trait adapters implement and what they report.

use serde::{Deserialize, Serialize};
use snappy_core::keys::Mods;

/// Caps Lock as the Snappy key. Held, Caps Lock acts as the Snappy key's modifiers, so
/// every shortcut, and anything else that reads those modifiers, works with it as with the
/// keys themselves, and Caps Lock never turns on. The adapter reports each press as
/// `PlatformEvent::CapsLock`, and the engine decides whether it was a tap and asks for
/// `toggle_caps_lock`. While Caps Lock is down the adapter may notice that another key or a
/// mouse button is used, and says so, but never which.
pub trait CapsLock {
    /// Make Caps Lock, held, act as `modifiers`, replacing what an earlier call set up.
    /// Refuses, with a sentence for the user, when the system already remaps Caps Lock or
    /// the change can't be made, or when it can only take effect from the next login.
    /// Returns what a later run needs to undo the change should this one end without
    /// `restore_caps_lock`, as a crash does.
    fn hold_caps_lock_as(&mut self, modifiers: Mods) -> Result<CapsLockUndo, CapsLockRefusal>;
    /// Give Caps Lock back its usual behavior.
    fn restore_caps_lock(&mut self);
    /// Undo the change an earlier run made and left in place, from what its
    /// `hold_caps_lock_as` returned.
    fn undo_caps_lock(&mut self, undo: &CapsLockUndo);
    /// Turn Caps Lock on or off, its light included, as a tap of the key does. Errs with
    /// why, for the debug log, when the system didn't take it.
    fn toggle_caps_lock(&mut self) -> Result<(), String>;
}

/// Why Caps Lock isn't the Snappy key after `CapsLock::hold_caps_lock_as`. Either way
/// nothing of the change is left in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapsLockRefusal {
    /// It can't be, as when the system already remaps Caps Lock: a sentence saying why.
    Refused(String),
    /// It can from the user's next login, when the desktop reads the change: a sentence
    /// saying so. The setting stays on, and the next run's hold succeeds.
    AfterLogin(String),
}

impl CapsLockRefusal {
    /// The sentence for the user.
    pub fn message(&self) -> &str {
        match self {
            CapsLockRefusal::Refused(message) | CapsLockRefusal::AfterLogin(message) => message,
        }
    }
}

/// What an adapter needs to undo its Caps Lock change in a later run, in a form of its own:
/// any value of the adapter's that serializes. The engine keeps it in a state file while the
/// change is in place.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CapsLockUndo(pub serde_json::Value);

impl CapsLockUndo {
    /// The record of `undo`, a value of the adapter's own type.
    pub fn of(undo: &impl Serialize) -> Result<CapsLockUndo, serde_json::Error> {
        serde_json::to_value(undo).map(CapsLockUndo)
    }

    /// The adapter's own value back from the record.
    pub fn read<T: serde::de::DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        T::deserialize(&self.0)
    }
}

/// Caps Lock, while it is the Snappy key. Adapters whose helpers report it as JSON read it
/// as `"pressed"`, `"otherInput"`, or `"released"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CapsLockInput {
    Pressed,
    /// Another key or a mouse button was used while Caps Lock was down.
    OtherInput,
    Released,
}
