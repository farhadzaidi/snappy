//! Global shortcuts through `RegisterHotKey` on the main window.
//!
//! Chords name physical keys; each is registered with the virtual key the foreground
//! thread's keyboard layout gives it, and re-registered when that layout changes.
//! Registering a chord and releasing it at once doubles as the availability probe.
//!
//! `RegisterHotKey` reports presses only. For a shortcut that reports being let go, the
//! keyboard hook runs from its press until its key comes up, and looks at that key alone.

use std::time::Instant;

use snappy_core::keys::{Combo, Modifier, Mods};
use snappy_platform::{HeldShortcuts, HotKeyRequest, KeyRepeat};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardLayout, HKL, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    RegisterHotKey, UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::events::is_down;
use crate::keys::virtual_key;
use crate::main_state::Main;

/// Application hotkey IDs must stay below 0xC000. The probe uses the last one.
const PROBE_ID: i32 = 0xBFFF;

/// A key as registered: modifier flags and virtual key.
type Binding = (u32, u16);

struct Registered {
    native: i32,
    engine: u32,
    binding: Binding,
}

/// The shortcuts the engine wants, those registered, the layout their virtual keys came
/// from, and which shortcuts that report being let go are down. A shortcut one layout
/// refuses stays wanted, so another layout can register it.
pub struct HotKeyTable {
    wanted: Vec<HotKeyRequest>,
    registered: Vec<Registered>,
    layout: isize,
    /// `MOD_NOREPEAT` keeps a held hotkey from repeating, and the hook ignores its key
    /// going down again.
    held: HeldShortcuts,
}

impl Default for HotKeyTable {
    fn default() -> Self {
        Self {
            wanted: Vec::new(),
            registered: Vec::new(),
            layout: 0,
            held: HeldShortcuts::new(KeyRepeat::Never),
        }
    }
}

impl HotKeyTable {
    /// The engine ID of the shortcut registered for virtual key `vk` with `mods` held.
    pub fn shortcut_for(&self, mods: Mods, vk: u16) -> Option<u32> {
        let binding = (modifier_flags(mods), vk);
        self.registered
            .iter()
            .find(|registered| registered.binding == binding)
            .map(|registered| registered.engine)
    }

    /// Whether a shortcut that reports being let go is down, so the keyboard hook watches
    /// for its key.
    pub fn is_holding(&self) -> bool {
        self.held.any_down()
    }
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
    /// Replaces every registration with `shortcuts`. Returns the IDs that couldn't be
    /// registered. A chord listed twice triggers the later ID. A shortcut still held is
    /// forgotten, its release unreported.
    pub fn register_hotkeys(&mut self, shortcuts: &[HotKeyRequest]) -> Vec<u32> {
        self.hotkeys.wanted = shortcuts.to_vec();
        self.hotkeys.held.register(shortcuts);
        self.update_input_hooks();
        self.register_wanted(foreground_layout())
    }

    /// Registers every wanted shortcut with the virtual keys `layout` gives them. Returns
    /// the IDs that couldn't be registered.
    fn register_wanted(&mut self, layout: HKL) -> Vec<u32> {
        self.release_hotkeys();
        self.hotkeys.layout = layout.0 as isize;
        let mut refused = Vec::new();
        for (index, wanted) in self.hotkeys.wanted.iter().enumerate() {
            let Some(key) = binding(&wanted.combo, layout) else {
                refused.push(wanted.id);
                continue;
            };
            if let Some(existing) = self
                .hotkeys
                .registered
                .iter_mut()
                .find(|item| item.binding == key)
            {
                existing.engine = wanted.id;
                continue;
            }
            let native = index as i32 + 1;
            if !register_one(self.window, native, key) {
                refused.push(wanted.id);
                continue;
            }
            self.hotkeys.registered.push(Registered {
                native,
                engine: wanted.id,
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
        self.hotkeys.wanted.clear();
        self.hotkeys.held.register(&[]);
        self.update_input_hooks();
        self.release_hotkeys();
    }

    fn release_hotkeys(&mut self) {
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

    /// `WM_HOTKEY` for one of Snappy's registrations. By now its key may be up again, too
    /// soon for the hook to see it.
    pub fn hotkey_pressed(&mut self, native: i32) {
        let pressed = self
            .hotkeys
            .registered
            .iter()
            .find(|item| item.native == native)
            .map(|item| (item.engine, item.binding.1));
        if let Some((engine, vk)) = pressed {
            self.shortcut_pressed(engine);
            if !is_down(vk) {
                self.release_shortcut(engine);
            }
        }
    }

    /// Report a shortcut's press. One that reports being let go is held from now until its
    /// key comes up, with the keyboard hook installed to see it; when the hook can't be,
    /// it's let go at once, so a hold never outlasts the key.
    pub fn shortcut_pressed(&mut self, engine: u32) {
        if let Some(event) = self.hotkeys.held.pressed(engine, Instant::now()) {
            self.emit(event);
        }
        if !self.hotkeys.held.is_down(engine) {
            return;
        }
        self.update_input_hooks();
        if self.keyboard.is_none() {
            self.release_shortcut(engine);
        }
    }

    /// A key reached the keyboard hook: a held shortcut's key coming up lets it go. Its key
    /// going down again is the key repeating.
    pub fn held_shortcut_key(&mut self, vk: u32, down: bool) {
        if down || !self.hotkeys.is_holding() {
            return;
        }
        let let_go: Vec<u32> = self
            .hotkeys
            .registered
            .iter()
            .filter(|registered| u32::from(registered.binding.1) == vk)
            .map(|registered| registered.engine)
            .collect();
        for engine in let_go {
            self.release_shortcut(engine);
        }
    }

    /// Report shortcut `engine` let go, if it's held, and remove the hook if nothing else
    /// needs it.
    fn release_shortcut(&mut self, engine: u32) {
        if let Some(event) = self.hotkeys.held.released(engine) {
            self.emit(event);
            self.update_input_hooks();
        }
    }

    /// Re-registers every wanted shortcut when the foreground window uses another layout.
    /// Returns whether any was refused, which the engine only learns by registering again.
    pub fn follow_keyboard_layout(&mut self, foreground: HWND) -> bool {
        if self.hotkeys.wanted.is_empty() {
            return false;
        }
        // SAFETY: plain queries.
        let layout = unsafe { GetKeyboardLayout(GetWindowThreadProcessId(foreground, None)) };
        if layout.0 as isize == self.hotkeys.layout {
            return false;
        }
        !self.register_wanted(layout).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_map_to_hotkey_flags() {
        let snappy_key = Mods::from_iter([Modifier::Ctrl, Modifier::Alt, Modifier::Meta]);
        assert_eq!(modifier_flags(snappy_key), MOD_CONTROL.0 | MOD_ALT.0 | MOD_WIN.0);
        assert_eq!(modifier_flags(Mods::NONE), 0);
        assert_eq!(modifier_flags(Mods::from_iter([Modifier::Shift])), MOD_SHIFT.0);
    }
}
