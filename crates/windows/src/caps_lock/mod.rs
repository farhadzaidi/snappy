//! Caps Lock as the Snappy key, through the low-level keyboard hook: nothing is installed
//! or written, and nothing outlives the process.
//!
//! The hook swallows Caps Lock, so Windows never turns it on, and reports its presses.
//! While it is down, a key matching a Snappy shortcut with the Snappy key's modifiers
//! fires that shortcut directly, without injecting modifier keys that Windows would act
//! on, such as the Windows key opening Start. A tap turns Caps Lock on or off by injecting
//! a Caps Lock press marked as Snappy's, which the hook lets through. The mouse hook runs
//! only while Caps Lock is down, to notice a button.
//!
//! Every key reaches the hook while this is on; only Caps Lock is looked at until it is
//! held, and then only to tell another key was used, never which, and to find its
//! shortcut.

mod keys;
mod scancode_map;

use snappy_core::keys::Mods;
use snappy_platform::{CapsLockInput, PlatformEvent};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_CAPITAL,
};

pub use keys::CapsLockKeys;
use keys::{Effect, HookKey};
pub use scancode_map::system_remaps_caps_lock;

use crate::events::modifiers;
use crate::main_state::Main;

/// Marks Snappy's own Caps Lock presses in their extra information, so the hook passes
/// them on.
pub const SNAPPY_CAPS_LOCK: usize = 0x534E_4150;

/// Turn Caps Lock on or off as its key would, for every keyboard and its light.
pub fn toggle() -> bool {
    let press = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VK_CAPITAL,
                dwFlags: flags,
                dwExtraInfo: SNAPPY_CAPS_LOCK,
                ..Default::default()
            },
        },
    };
    let inputs = [press(KEYBD_EVENT_FLAGS(0)), press(KEYEVENTF_KEYUP)];
    // SAFETY: two fully initialized input records.
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    sent as usize == inputs.len()
}

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
