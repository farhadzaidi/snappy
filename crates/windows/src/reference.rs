//! Read by the live-test harness and the integration tests, so their expectations use
//! Snappy's own view of displays, keys, and windows.

use snappy_core::geometry::Rect;
use snappy_core::keys::{KeyCode, Modifier};
use snappy_platform::Display;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext};
use windows::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowExW, GWL_EXSTYLE, GetWindowLongW, IsIconic, IsWindowVisible, PostMessageW,
    WM_CLOSE, WS_EX_TOPMOST,
};
use windows::core::{BOOL, w};

use crate::managed::record::{is_cloaked, pid, visible_frame as frame_of};
use crate::{displays as display_list, keys};

/// Use physical pixels everywhere, as the daemon does. Call before creating windows.
pub fn set_dpi_aware() {
    // SAFETY: a process-wide setting; fails harmlessly when already set.
    let _ = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
}

/// Every display, primary first, as the adapter reports them.
pub fn displays() -> Vec<Display> {
    display_list::read()
}

/// The set-1 scan code of `code`'s key and whether it is an extended key.
pub fn scan_code(code: &KeyCode) -> Option<(u16, bool)> {
    let key = keys::key(code)?;
    Some((key.scan & 0xFF, key.is_extended()))
}

/// The virtual key of the left-hand key for `modifier`.
pub fn modifier_key(modifier: Modifier) -> u16 {
    keys::modifier_key(modifier).0
}

/// A window's visible frame, as Snappy measures it.
pub fn visible_frame(window: isize) -> Option<Rect> {
    frame_of(HWND(window as *mut _))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenWindow {
    pub pid: u32,
    /// Always above normal windows.
    pub topmost: bool,
    pub frame: Rect,
}

unsafe extern "system" fn collect(window: HWND, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the vector `on_screen_windows` passed, alive for the enumeration.
    let found = unsafe { &mut *(data.0 as *mut Vec<isize>) };
    found.push(window.0 as isize);
    BOOL(1)
}

/// Visible, uncloaked, unminimized top-level windows, front to back.
pub fn on_screen_windows() -> Vec<ScreenWindow> {
    let mut handles: Vec<isize> = Vec::new();
    // SAFETY: the callback only pushes into `handles`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&raw mut handles as isize)) };
    handles
        .into_iter()
        .map(|handle| HWND(handle as *mut _))
        .filter(|window| {
            // SAFETY: plain queries.
            let shown = unsafe { IsWindowVisible(*window).as_bool() && !IsIconic(*window).as_bool() };
            shown && !is_cloaked(*window)
        })
        .filter_map(|window| {
            // SAFETY: a plain style read.
            let extended = unsafe { GetWindowLongW(window, GWL_EXSTYLE) } as u32;
            Some(ScreenWindow {
                pid: pid(window)?,
                topmost: extended & WS_EX_TOPMOST.0 != 0,
                frame: frame_of(window)?,
            })
        })
        .collect()
}

/// The hidden main window of the daemon `daemon_pid`.
fn daemon_window(daemon_pid: u32) -> Option<HWND> {
    let mut after: Option<HWND> = None;
    loop {
        // SAFETY: finds top-level windows of Snappy's class one after another.
        let window = unsafe { FindWindowExW(None, after, w!("Snappy.Daemon"), None) }.ok()?;
        if pid(window) == Some(daemon_pid) {
            return Some(window);
        }
        after = Some(window);
    }
}

/// Whether the daemon `daemon_pid` has its notification-area icon in place.
pub fn has_tray_icon(daemon_pid: u32) -> bool {
    let Some(window) = daemon_window(daemon_pid) else {
        return false;
    };
    let icon = NOTIFYICONIDENTIFIER {
        cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: window,
        uID: 1,
        ..NOTIFYICONIDENTIFIER::default()
    };
    // SAFETY: the identifier names that window's icon.
    unsafe { Shell_NotifyIconGetRect(&icon) }.is_ok()
}

/// Asks the daemon to quit the way Windows asks an app to close: a close request to its
/// window. `taskkill` without `/F` can't, because it only closes visible windows.
pub fn request_quit(daemon_pid: u32) -> bool {
    daemon_window(daemon_pid).is_some_and(|window| {
        // SAFETY: posts a plain message to another process's window.
        unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok()
    })
}
