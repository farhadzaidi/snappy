//! User-facing messages for refusals and errors, worded the same on every platform where the
//! situation is the same.

use snappy_core::error::WindowError;

pub const FULL_SCREEN: &str = "Leave full screen before tiling this window.";
pub const NOT_STANDARD: &str = "Choose a standard application window.";
pub const NOT_MOVABLE: &str = "This window does not allow moving and resizing.";
pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const SHELL_UNREACHABLE: &str =
    "Snappy lost contact with GNOME Shell. It will try again when Shell is back.";
pub const ON_ALL_WORKSPACES: &str = "GNOME shows this window on every workspace, so Snappy can't hide it. Windows on \
     displays other than the primary one do this when \"Workspaces on primary display only\" is on in Settings, \
     Multitasking.";

pub const EXTENSION_NEEDS_LOGIN: &str = "Log out and back in to finish setting up Snappy's GNOME Shell extension. Snappy will start snapping then.";
pub const EXTENSION_OTHER_VERSION: &str = "Snappy's GNOME Shell extension is from another version of Snappy. Log \
     out and back in to load the one that matches.";
pub const EXTENSION_DISABLED: &str = "Snappy's GNOME Shell extension is turned off. Turn on Snappy in the \
     Extensions app so Snappy can control windows.";
pub const EXTENSIONS_OFF: &str = "GNOME has extensions turned off. Turn on \"Use Extensions\" in the Extensions \
     app so Snappy can control windows.";
pub const EXTENSION_UNSUPPORTED: &str =
    "This version of GNOME Shell isn't supported by Snappy's extension. Snappy needs GNOME 46 or newer.";
pub const EXTENSION_NOT_INSTALLED: &str = "Snappy couldn't install its GNOME Shell extension, so it can't \
     control windows. Check that your home folder is writable, then quit and reopen Snappy.";

/// The extension failed to start, with GNOME Shell's own explanation.
pub fn extension_error(detail: &str) -> String {
    let detail = detail.trim().trim_end_matches('.');
    if detail.is_empty() {
        "Snappy's GNOME Shell extension couldn't start. Log out and back in to try again.".to_owned()
    } else {
        format!("Snappy's GNOME Shell extension couldn't start: {detail}. Log out and back in to try again.")
    }
}

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}
