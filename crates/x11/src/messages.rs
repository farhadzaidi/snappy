//! User-facing messages for refusals and errors, worded the same on every platform.

use snappy_core::error::WindowError;

pub use snappy_linux::messages::NOT_STANDARD;

pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const WAYLAND: &str =
    "Snappy can't control windows on this desktop yet. It works on GNOME, KDE Plasma, or in an X11 session.";
pub const NO_PARKING_DESKTOP: &str =
    "The window manager didn't add a desktop for hidden workspace windows, so workspaces can't hide windows.";

pub const CAPS_LOCK_OTHER_SETTING: &str = "Caps Lock is already remapped by another keyboard setting, such as xmodmap. Remove that to use Caps Lock as the Snappy key.";
pub const CAPS_LOCK_TAKEN: &str = "Another program uses Caps Lock as a shortcut, so it can't be the Snappy key. Quit that program to use Caps Lock as the Snappy key.";
pub const CAPS_LOCK_FAILED: &str = "The X server didn't let Snappy change Caps Lock.";

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}
