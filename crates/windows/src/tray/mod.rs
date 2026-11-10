//! The notification-area icon and its menu, the Windows counterpart of the menu bar item.
//! The menu is built from the latest content each time it opens.

mod menu;

use snappy_core::geometry::Rect;
use snappy_platform::{PlatformEvent, TrayMenu};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_SELECT,
    NINF_KEY, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, HICON, HMENU, ICONINFO,
    MF_CHECKED, MF_GRAYED, MF_POPUP, MF_SEPARATOR, MF_STRING, PostMessageW, SM_CXSMICON, SetForegroundWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_CONTEXTMENU, WM_NULL,
};
use windows::core::{BOOL, PCWSTR};

use self::menu::Entry;
use crate::dispatch::WM_TRAY;
use crate::main_state::with_main;
use crate::overlay::{Canvas, Color, Dib};
use crate::wide::{copy_into, wide};

/// The only icon Snappy adds.
const ICON_ID: u32 = 1;
/// The icon was chosen with the keyboard.
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
const MINT: u32 = 0x00C7BE;

#[derive(Default)]
pub struct Tray {
    window: Option<HWND>,
    icon: Option<HICON>,
    menu: TrayMenu,
    added: bool,
}

/// Two zones side by side, in the overlay's highlight color, which reads on light and
/// dark taskbars alike.
fn draw_icon(size: usize) -> Option<HICON> {
    let mut dib = Dib::new(size, size)?;
    let mut canvas = Canvas::new(size, size, dib.pixels());
    let unit = size as f64 / 16.0;
    let color = Color::rgb(MINT, 1.0);
    canvas.fill_rounded(
        Rect::new(unit, 2.0 * unit, 6.5 * unit, 12.0 * unit),
        1.5 * unit,
        color,
    );
    canvas.fill_rounded(
        Rect::new(8.5 * unit, 2.0 * unit, 6.5 * unit, 12.0 * unit),
        1.5 * unit,
        color,
    );
    let side = i32::try_from(size).ok()?;
    // SAFETY: a monochrome mask of the icon's size; the color bitmap carries alpha.
    let mask = unsafe { CreateBitmap(side, side, 1, 1, None) };
    let info = ICONINFO {
        fIcon: BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: dib.bitmap,
    };
    // SAFETY: the icon copies both bitmaps; the mask is freed after.
    let icon = unsafe { CreateIconIndirect(&info) }.ok();
    // SAFETY: the mask was created above and isn't selected anywhere.
    let _ = unsafe { DeleteObject(mask.into()) };
    icon
}

impl Tray {
    fn data(&self, window: HWND) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: WM_TRAY,
            hIcon: self.icon.unwrap_or_default(),
            ..NOTIFYICONDATAW::default()
        };
        let tooltip = if self.menu.tooltip.is_empty() {
            "Snappy"
        } else {
            &self.menu.tooltip
        };
        copy_into(&mut data.szTip, tooltip);
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        data
    }

    /// Adds the icon; again after Explorer restarts, which drops every icon.
    pub fn add(&mut self, window: HWND) {
        self.window = Some(window);
        if self.icon.is_none() {
            // SAFETY: plain queries for a window this thread owns.
            let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForWindow(window)) };
            self.icon = draw_icon(usize::try_from(size).unwrap_or(16).max(16));
        }
        let data = self.data(window);
        // SAFETY: `data` is fully initialized for this window.
        self.added = unsafe {
            Shell_NotifyIconW(NIM_ADD, &data).as_bool() && Shell_NotifyIconW(NIM_SETVERSION, &data).as_bool()
        };
    }

    pub fn update(&mut self, menu: &TrayMenu) {
        let tooltip_changed = menu.tooltip != self.menu.tooltip;
        self.menu = menu.clone();
        if let (true, true, Some(window)) = (tooltip_changed, self.added, self.window) {
            let data = self.data(window);
            // SAFETY: as in `add`.
            let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        }
    }

    pub fn remove(&mut self) {
        if let (true, Some(window)) = (self.added, self.window) {
            let data = self.data(window);
            // SAFETY: as in `add`.
            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
            self.added = false;
        }
        if let Some(icon) = self.icon.take() {
            // SAFETY: an icon this module created, destroyed once.
            let _ = unsafe { DestroyIcon(icon) };
        }
    }
}

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
