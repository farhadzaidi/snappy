//! User-facing messages for refusals and errors, worded the same on every platform.

use snappy_core::error::WindowError;

pub const FULL_SCREEN: &str = "Leave full screen before tiling this window.";
pub const NOT_STANDARD: &str = "Choose a standard application window.";
pub const NOT_MOVABLE: &str = "This window does not allow moving and resizing.";
pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const WAYLAND: &str =
    "Snappy can't control windows in a Wayland session yet. Log in with an X11 session to use Snappy.";
pub const NO_PARKING_DESKTOP: &str =
    "The window manager didn't add a desktop for hidden workspace windows, so workspaces can't hide windows.";

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}
