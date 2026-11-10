//! Window classes for Snappy's own windows.

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, WINDOW_EX_STYLE, WM_CLOSE, WNDCLASSEXW, WNDPROC,
    WS_POPUP,
};
use windows::core::PCWSTR;

/// For overlays. They ignore close requests, which `taskkill` sends to every
/// top-level window of a process: only Snappy closes them.
pub unsafe extern "system" fn default_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_CLOSE {
        return LRESULT(0);
    }
    // SAFETY: forwards the message unchanged.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// Registers `name` with `procedure`. Registering a name twice fails harmlessly.
pub fn register(name: PCWSTR, procedure: WNDPROC) {
    // SAFETY: the class structure is complete and the name is a static string.
    unsafe {
        let Ok(module) = GetModuleHandleW(None) else {
            return;
        };
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: procedure,
            hInstance: module.into(),
            lpszClassName: name,
            ..WNDCLASSEXW::default()
        };
        RegisterClassExW(&class);
    }
}

/// A popup window of class `name`, hidden unless the caller shows it.
pub fn popup(
    name: PCWSTR,
    extended: WINDOW_EX_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) -> Option<HWND> {
    // SAFETY: creates a window of a registered class on this thread.
    unsafe {
        let module = GetModuleHandleW(None).ok()?;
        CreateWindowExW(
            extended,
            name,
            None,
            WS_POPUP,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(module.into()),
            None,
        )
        .ok()
    }
}
