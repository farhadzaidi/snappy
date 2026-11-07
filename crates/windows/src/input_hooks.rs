//! Low-level keyboard and mouse hooks, installed only while drag monitoring is on. They
//! report Escape, modifier changes, and the primary button's release, and always pass
//! every event on.
//!
//! The release is taken from the mouse hook rather than from the end of the move loop:
//! hooks see input in the order it happened, while the move loop's end arrives after the
//! app has handled it, by which time the user may have let go of the drag modifier.

use snappy_core::geometry::Point;
use snappy_core::keys::{Modifier, Mods};
use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT,
    VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, MSLLHOOKSTRUCT, SetWindowsHookExW,
    UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOWS_HOOK_ID, WM_KEYDOWN, WM_LBUTTONUP,
    WM_SYSKEYDOWN,
};

use crate::events::{cursor, modifiers};
use crate::keys::is_escape;
use crate::main_state::{Main, with_main};

/// The modifier a virtual key belongs to.
pub fn modifier_of(vk: u32) -> Option<Modifier> {
    let vk = u16::try_from(vk).ok()?;
    let is = |keys: &[u16]| keys.contains(&vk);
    if is(&[VK_SHIFT.0, VK_LSHIFT.0, VK_RSHIFT.0]) {
        Some(Modifier::Shift)
    } else if is(&[VK_CONTROL.0, VK_LCONTROL.0, VK_RCONTROL.0]) {
        Some(Modifier::Ctrl)
    } else if is(&[VK_MENU.0, VK_LMENU.0, VK_RMENU.0]) {
        Some(Modifier::Alt)
    } else if is(&[VK_LWIN.0, VK_RWIN.0]) {
        Some(Modifier::Meta)
    } else {
        None
    }
}

/// `held` with one modifier pressed or released. The hook runs before the system's key
/// state is updated, so the key being reported overrides what's held.
pub fn with_change(held: Mods, modifier: Modifier, down: bool) -> Mods {
    if down {
        held.with(modifier)
    } else {
        held.iter().filter(|other| *other != modifier).collect()
    }
}

unsafe extern "system" fn on_key(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for HC_ACTION, lparam points to the key's KBDLLHOOKSTRUCT.
        let key = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        with_main(|main| main.key(key.vkCode, down));
    }
    // SAFETY: passes the event to the next hook unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn on_mouse(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wparam.0 as u32 == WM_LBUTTONUP {
        // SAFETY: for HC_ACTION, lparam points to the event's MSLLHOOKSTRUCT.
        let event = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        let at = Point::new(f64::from(event.pt.x), f64::from(event.pt.y));
        with_main(|main| main.released(at));
    }
    // SAFETY: passes the event to the next hook unchanged.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn install(kind: WINDOWS_HOOK_ID, procedure: HOOKPROC) -> Option<HHOOK> {
    // SAFETY: a low-level hook with a static callback, called on this thread.
    unsafe {
        let module = GetModuleHandleW(None).ok();
        SetWindowsHookExW(kind, procedure, module.map(Into::into), 0)
    }
    .ok()
}

fn remove(hook: Option<HHOOK>) {
    if let Some(hook) = hook {
        // SAFETY: a hook this thread installed, removed once.
        let _ = unsafe { UnhookWindowsHookEx(hook) };
    }
}

impl Main {
    fn key(&mut self, vk: u32, down: bool) {
        if !self.shared.monitoring().drag {
            return;
        }
        if is_escape(vk) {
            if down {
                self.emit(PlatformEvent::Escape);
            }
            return;
        }
        let Some(modifier) = modifier_of(vk) else {
            return;
        };
        let mods = with_change(self.held, modifier, down);
        if mods != self.held {
            self.held = mods;
            self.emit(PlatformEvent::ModifiersChanged { at: cursor(), mods });
        }
    }

    /// The primary button went up during a move or size loop: the drop.
    fn released(&mut self, at: Point) {
        if self.drag.take().is_some() {
            self.emit(PlatformEvent::PointerUp { at, mods: self.held });
        }
    }

    /// Installs or removes the hooks to match drag monitoring.
    pub fn set_input_hooks(&mut self, on: bool) {
        if on == self.keyboard.is_some() {
            return;
        }
        if on {
            self.held = modifiers();
            self.keyboard = install(WH_KEYBOARD_LL, Some(on_key));
            self.mouse = install(WH_MOUSE_LL, Some(on_mouse));
        } else {
            remove(self.keyboard.take());
            remove(self.mouse.take());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifier_keys_of_either_side_count() {
        assert_eq!(modifier_of(u32::from(VK_RSHIFT.0)), Some(Modifier::Shift));
        assert_eq!(modifier_of(u32::from(VK_LCONTROL.0)), Some(Modifier::Ctrl));
        assert_eq!(modifier_of(u32::from(VK_RMENU.0)), Some(Modifier::Alt));
        assert_eq!(modifier_of(u32::from(VK_LWIN.0)), Some(Modifier::Meta));
        assert_eq!(modifier_of(0x41), None);
    }

    #[test]
    fn the_reported_key_overrides_the_held_state() {
        let shift = Mods::from_iter([Modifier::Shift]);
        let both = with_change(shift, Modifier::Ctrl, true);
        assert!(both.contains(Modifier::Ctrl) && both.contains(Modifier::Shift));
        assert_eq!(with_change(both, Modifier::Ctrl, false), shift);
    }
}
