//! What the window server shows: owner, stacking layer, and bounds of on-screen windows,
//! and screenshots, each from the desktop in use (`desktop`). Layers follow macOS: 0 for
//! normal windows, higher for floating ones.

use snappy_core::geometry::Rect;

#[cfg(target_os = "linux")]
pub use crate::desktop::x11::session_bus_names;

/// Normal application windows.
pub const NORMAL_LAYER: i64 = 0;
/// Floating windows, such as overlays, above every normal window.
pub const FLOATING_LAYER: i64 = 3;
/// The menu bar's status items.
pub const STATUS_BAR_LAYER: i64 = 25;
/// Open menus.
pub const POPUP_MENU_LAYER: i64 = 101;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenWindow {
    pub pid: i32,
    pub layer: i64,
    pub frame: Rect,
}

/// On-screen windows, front to back.
#[cfg(target_os = "macos")]
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    crate::desktop::macos::on_screen_windows()
}

/// On Wayland the desktop shell lists windows and Snappy's overlays; on X11 the X server
/// does.
#[cfg(target_os = "linux")]
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    match crate::desktop::wayland::Shell::current() {
        Some(shell) => shell.on_screen_windows(crate::session::daemon_pid()),
        None => crate::desktop::x11::on_screen_windows(),
    }
}

#[cfg(windows)]
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    crate::desktop::windows::on_screen_windows()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn on_screen_windows() -> Result<Vec<ScreenWindow>, String> {
    Err("reading the window list isn't implemented on this platform yet".to_owned())
}

/// Saves the whole screen as a PNG at `path`.
#[cfg(target_os = "macos")]
pub fn screenshot(path: &std::path::Path) {
    crate::desktop::macos::screenshot(path);
}

/// Saves the whole screen, every monitor included, as a PNG at `path`: through the desktop
/// shell on Wayland, else from the X server.
#[cfg(target_os = "linux")]
pub fn screenshot(path: &std::path::Path) {
    match crate::desktop::wayland::Shell::current() {
        Some(shell) => shell.screenshot(path),
        None => crate::desktop::x11::screenshot(path),
    }
}

#[cfg(windows)]
pub fn screenshot(path: &std::path::Path) {
    crate::desktop::windows::screenshot(path);
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn screenshot(_path: &std::path::Path) {}
