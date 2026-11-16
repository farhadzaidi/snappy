//! The icon's menu: opening it where the icon was clicked or chosen, and reporting the
//! chosen item.

use snappy_platform::PlatformEvent;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{NIN_SELECT, NINF_KEY};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, HMENU, MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR,
    MF_STRING, PostMessageW, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx,
    WM_CONTEXTMENU, WM_NULL,
};
use windows::core::PCWSTR;

use super::menu::{self, Entry};
use crate::main_state::with_main;
use crate::wide::wide;

/// The icon was chosen with the keyboard.
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;

/// Builds a Win32 menu from `entries`; item IDs are 1-based indexes into `events`.
fn materialize(entries: &[Entry], events: &mut Vec<PlatformEvent>) -> Option<HMENU> {
    // SAFETY: a new, empty popup menu.
    let menu = unsafe { CreatePopupMenu() }.ok()?;
    for entry in entries {
        let (flags, id, title) = match entry {
            Entry::Label(title) => (MF_STRING | MF_GRAYED, 0, title.as_str()),
            Entry::Separator => (MF_SEPARATOR, 0, ""),
            Entry::Item {
                title,
                checked,
                enabled,
                event,
            } => {
                events.push(event.clone());
                let mut flags = MF_STRING;
                if *checked == Some(true) {
                    flags |= MF_CHECKED;
                }
                if !enabled {
                    flags |= MF_GRAYED;
                }
                (flags, events.len(), title.as_str())
            }
            Entry::Submenu { title, entries } => {
                let Some(submenu) = materialize(entries, events) else {
                    continue;
                };
                (MF_POPUP, submenu.0 as usize, title.as_str())
            }
        };
        let text = wide(title);
        let text = if flags == MF_SEPARATOR {
            PCWSTR::null()
        } else {
            PCWSTR(text.as_ptr())
        };
        // SAFETY: the text outlives the call; the menu copies it.
        let _ = unsafe { AppendMenuW(menu, flags, id, text) };
    }
    Some(menu)
}

/// The icon's callback. With version 4, the event is the low word of `lparam` and the
/// anchor point is packed into `wparam`.
pub fn callback(window: HWND, wparam: WPARAM, lparam: LPARAM) {
    let event = (lparam.0 as u32) & 0xFFFF;
    if !matches!(event, WM_CONTEXTMENU | NIN_SELECT | NIN_KEYSELECT) {
        return;
    }
    let anchor = POINT {
        x: i32::from(wparam.0 as u16 as i16),
        y: i32::from((wparam.0 >> 16) as u16 as i16),
    };
    open_menu(window, anchor);
}

/// Shows the menu and reports the chosen item. The menu runs its own modal loop, so the
/// main thread's state is released while it's open.
fn open_menu(window: HWND, anchor: POINT) {
    let Some(entries) = with_main(|main| menu::build(&main.tray.menu)) else {
        return;
    };
    let mut events = Vec::new();
    let Some(menu) = materialize(&entries, &mut events) else {
        return;
    };
    // SAFETY: the menu belongs to this thread; bringing the owner forward lets a click
    // elsewhere dismiss it, and the posted null message ends the menu's loop cleanly.
    let chosen = unsafe {
        let _ = SetForegroundWindow(window);
        let chosen = TrackPopupMenuEx(
            menu,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
            anchor.x,
            anchor.y,
            window,
            None,
        );
        let _ = PostMessageW(Some(window), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        chosen.0
    };
    let chosen = usize::try_from(chosen).ok().and_then(|id| id.checked_sub(1));
    if let Some(event) = chosen.and_then(|index| events.get(index)).cloned() {
        with_main(|main| main.emit(event));
    }
}
