//! Global shortcuts through `RegisterHotKey` on the main window.
//!
//! Chords name physical keys; each is registered with the virtual key the foreground
//! thread's keyboard layout gives it, and re-registered when that layout changes.
//! Registering a chord and releasing it at once doubles as the availability probe.

use snappy_core::keys::{Combo, Modifier, Mods};
use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, HKL, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::keys::virtual_key;
use crate::main_state::Main;

/// Application hotkey IDs must stay below 0xC000. The probe uses the last one.
const PROBE_ID: i32 = 0xBFFF;

/// A key as registered: modifier flags and virtual key.
type Binding = (u32, u16);

struct Registered {
    native: i32,
    engine: u32,
    combo: Combo,
    binding: Binding,
}

/// Registered shortcuts and the layout their virtual keys came from.
#[derive(Default)]
pub struct HotKeyTable {
    registered: Vec<Registered>,
    layout: isize,
}

pub fn modifier_flags(mods: Mods) -> u32 {
    [
        (Modifier::Alt, MOD_ALT),
        (Modifier::Ctrl, MOD_CONTROL),
        (Modifier::Shift, MOD_SHIFT),
        (Modifier::Meta, MOD_WIN),
    ]
    .into_iter()
    .filter(|(modifier, _)| mods.contains(*modifier))
    .fold(0, |flags, (_, flag)| flags | flag.0)
}

/// The keyboard layout of the foreground window's thread, which decides what virtual key
/// a physical key produces.
fn foreground_layout() -> HKL {
    // SAFETY: plain queries; no foreground window yields thread 0, the system default.
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        GetKeyboardLayout(thread)
    }
}

fn binding(combo: &Combo, layout: HKL) -> Option<Binding> {
    Some((modifier_flags(combo.mods), virtual_key(&combo.code, layout)?))
}

fn register_one(window: HWND, id: i32, (mods, vk): Binding) -> bool {
    // SAFETY: registers on a window this thread owns.
    unsafe {
        RegisterHotKey(
            Some(window),
            id,
            HOT_KEY_MODIFIERS(mods | MOD_NOREPEAT.0),
            u32::from(vk),
        )
    }
    .is_ok()
}

fn unregister_one(window: HWND, id: i32) {
    // SAFETY: an ID this thread registered on its own window.
    let _ = unsafe { UnregisterHotKey(Some(window), id) };
}

impl Main {
    /// Replaces every registration with `keys`. Returns the IDs that couldn't be
    /// registered. A chord listed twice triggers the later ID.
    pub fn register_hotkeys(&mut self, keys: &[(u32, Combo)]) -> Vec<u32> {
        self.unregister_hotkeys();
        let layout = foreground_layout();
        self.hotkeys.layout = layout.0 as isize;
        let mut refused = Vec::new();
        for (index, (engine, combo)) in keys.iter().enumerate() {
            let Some(key) = binding(combo, layout) else {
                refused.push(*engine);
                continue;
            };
            if let Some(existing) = self
                .hotkeys
                .registered
                .iter_mut()
                .find(|item| item.binding == key)
            {
                existing.engine = *engine;
                continue;
            }
            let native = index as i32 + 1;
            if !register_one(self.window, native, key) {
                refused.push(*engine);
                continue;
            }
            self.hotkeys.registered.push(Registered {
                native,
                engine: *engine,
                combo: combo.clone(),
                binding: key,
            });
        }
        refused
    }

    fn registered_binding(&self, key: Binding) -> bool {
        self.hotkeys
            .registered
            .iter()
            .any(|registered| registered.binding == key)
    }

    pub fn unregister_hotkeys(&mut self) {
        for registered in std::mem::take(&mut self.hotkeys.registered) {
            unregister_one(self.window, registered.native);
        }
    }

    /// Snappy's own registrations count as free; anything else is probed.
    pub fn hotkey_available(&mut self, combo: &Combo) -> bool {
        let Some(key) = binding(combo, foreground_layout()) else {
            return false;
        };
        if self.registered_binding(key) {
            return true;
        }
        let free = register_one(self.window, PROBE_ID, key);
        if free {
            unregister_one(self.window, PROBE_ID);
        }
        free
    }

    /// `WM_HOTKEY` for one of Snappy's registrations.
    pub fn hotkey_pressed(&mut self, native: i32) {
        let engine = self
            .hotkeys
            .registered
            .iter()
            .find(|item| item.native == native)
            .map(|item| item.engine);
        if let Some(engine) = engine {
            self.emit(PlatformEvent::HotKey(engine));
        }
    }

    /// Re-registers every shortcut when the foreground window uses another layout.
    pub fn follow_keyboard_layout(&mut self, foreground: HWND) {
        if self.hotkeys.registered.is_empty() {
            return;
        }
        // SAFETY: plain queries.
        let layout = unsafe { GetKeyboardLayout(GetWindowThreadProcessId(foreground, None)) };
        if layout.0 as isize == self.hotkeys.layout {
            return;
        }
        let keys: Vec<(u32, Combo)> = self
            .hotkeys
            .registered
            .iter()
            .map(|registered| (registered.engine, registered.combo.clone()))
            .collect();
        self.register_hotkeys(&keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_map_to_hotkey_flags() {
        let hyper = Mods::from_iter([Modifier::Ctrl, Modifier::Alt, Modifier::Meta]);
        assert_eq!(modifier_flags(hyper), MOD_CONTROL.0 | MOD_ALT.0 | MOD_WIN.0);
        assert_eq!(modifier_flags(Mods::NONE), 0);
        assert_eq!(modifier_flags(Mods::from_iter([Modifier::Shift])), MOD_SHIFT.0);
    }
}
