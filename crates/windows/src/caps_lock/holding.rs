//! Holding Caps Lock as the Snappy key: installing the hook for it, and what each key and
//! button that reaches the hook means.

use snappy_core::keys::Mods;
use snappy_platform::{CapsLockInput, PlatformEvent};

use super::keys::{CapsLockKeys, Effect, HookKey};
use super::toggle::SNAPPY_CAPS_LOCK;
use crate::events::modifiers;
use crate::main_state::Main;

impl Main {
    /// Hold Caps Lock as `modifiers` from now on. False when the hook can't be installed.
    pub fn hold_caps_lock(&mut self, modifiers: Mods) -> bool {
        match &mut self.caps_lock {
            Some(keys) => keys.set_modifiers(modifiers),
            None => self.caps_lock = Some(CapsLockKeys::new(modifiers)),
        }
        self.update_input_hooks();
        if self.keyboard.is_none() {
            self.restore_caps_lock();
            return false;
        }
        true
    }

    pub fn restore_caps_lock(&mut self) {
        self.caps_lock = None;
        self.update_input_hooks();
    }

    /// A key reached the hook: whether to swallow it.
    pub fn caps_lock_key(&mut self, vk: u32, down: bool, extra: usize) -> bool {
        let Some(keys) = self.caps_lock.as_mut() else {
            return false;
        };
        let Ok(vk) = u16::try_from(vk) else {
            return false;
        };
        let was_held = keys.is_held();
        let key = HookKey {
            vk,
            down,
            ours: extra == SNAPPY_CAPS_LOCK,
        };
        let hotkeys = &self.hotkeys;
        let verdict = keys.key(key, modifiers(), |mods, vk| hotkeys.shortcut_for(mods, vk));
        let held = keys.is_held();
        for effect in verdict.effects {
            self.report(effect);
        }
        if held != was_held {
            self.update_input_hooks();
        }
        verdict.swallow
    }

    /// A mouse button went down.
    pub fn caps_lock_button(&mut self) {
        if let Some(effect) = self.caps_lock.as_mut().and_then(CapsLockKeys::button) {
            self.report(effect);
        }
    }

    pub fn caps_lock_held(&self) -> bool {
        self.caps_lock.as_ref().is_some_and(CapsLockKeys::is_held)
    }

    fn report(&self, effect: Effect) {
        self.emit(match effect {
            Effect::Pressed => PlatformEvent::CapsLock(CapsLockInput::Pressed),
            Effect::OtherInput => PlatformEvent::CapsLock(CapsLockInput::OtherInput),
            Effect::Released => PlatformEvent::CapsLock(CapsLockInput::Released),
            Effect::Shortcut(id) => PlatformEvent::HotKey(id),
        });
    }
}
