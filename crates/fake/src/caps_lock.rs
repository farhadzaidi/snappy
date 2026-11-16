//! Caps Lock as the Snappy key on the fake platform, with refusals and failures to script.

use snappy_core::keys::Mods;
use snappy_platform::{CapsLock, CapsLockRefusal, CapsLockUndo};

use crate::platform::FakePlatform;

impl CapsLock for FakePlatform {
    fn hold_caps_lock_as(&mut self, modifiers: Mods) -> Result<CapsLockUndo, CapsLockRefusal> {
        if let Some(refusal) = &self.caps_lock_refusal {
            self.caps_lock_held_as = None;
            return Err(refusal.clone());
        }
        self.caps_lock_held_as = Some(modifiers);
        Ok(self.caps_lock_undo.clone())
    }

    fn restore_caps_lock(&mut self) {
        self.caps_lock_held_as = None;
    }

    fn undo_caps_lock(&mut self, undo: &CapsLockUndo) {
        self.caps_lock_undone.push(undo.clone());
    }

    fn toggle_caps_lock(&mut self) -> Result<(), String> {
        if let Some(failure) = &self.caps_lock_toggle_failure {
            return Err(failure.clone());
        }
        self.caps_lock_toggles += 1;
        Ok(())
    }
}
