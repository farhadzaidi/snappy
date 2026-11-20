//! User-facing messages for refusals and errors, worded the same on every platform where the
//! situation is the same.

use snappy_core::error::WindowError;
pub use snappy_linux::messages::NOT_STANDARD;

pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const KWIN_UNREACHABLE: &str = "Snappy can't reach KWin right now. Try again in a moment.";
pub const ON_ALL_DESKTOPS: &str = "This window shows on every desktop, so Snappy can't hide it.";
pub const HIDE_REFUSED: &str = "KWin didn't let Snappy hide this window.";
pub const LOWER_NOT_ACTIVE: &str = "KWin lets Snappy lower only the active window. Click it and try again.";
pub const UNEXPECTED_REPLY: &str = "Snappy and its KWin script don't agree. Quit and reopen Snappy.";

pub const KWIN_NOT_RUNNING: &str = "Snappy can't find KWin. Snappy works with KDE Plasma's window manager.";
pub const SCRIPT_NOT_LOADED: &str = "KWin didn't start Snappy's script. Quit and reopen Snappy.";
pub const SCRIPT_STARTING: &str = "Snappy's KWin script is starting.";
pub const SCRIPT_OTHER_VERSION: &str =
    "A different version of Snappy's KWin script is running. Quit and reopen Snappy.";

pub const CAPS_LOCK_NOT_SET_UP: &str =
    "Snappy couldn't change KDE's keyboard settings to make Caps Lock the Snappy key.";
pub const CAPS_LOCK_AFTER_LOGIN: &str = "Caps Lock becomes the Snappy key after you log out and back in.";

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}

/// What `snappy --uninstall` says about KDE's settings.
pub const KEYBOARD_SETTINGS_RESTORED: &str = "Took Snappy's Caps Lock option out of KDE's keyboard settings.";
pub const KEYBOARD_SETTINGS_NOT_RESTORED: &str =
    "Snappy couldn't take its Caps Lock option out of KDE's keyboard settings.";
pub const SHORTCUTS_REMOVED: &str = "Removed Snappy's shortcuts from KDE's settings.";
pub const SHORTCUTS_NOT_REMOVED: &str = "Snappy couldn't remove its shortcuts from KDE's settings.";
