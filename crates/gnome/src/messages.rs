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

/// GNOME loads a new extension only when the desktop session starts.
pub fn needs_login(desktop: &str) -> String {
    format!("One last step: log out of {desktop} and sign back in to finish setting up Snappy.")
}

/// A different copy of the extension is loaded; the new one loads with the next session.
pub fn other_version(desktop: &str) -> String {
    format!("Snappy was updated. Log out of {desktop} and sign back in to finish the update.")
}

pub const EXTENSION_DISABLED: &str =
    "Snappy's GNOME extension is turned off. Turn it on in the Extensions app.";
pub const EXTENSIONS_OFF: &str =
    "GNOME extensions are turned off. Turn on \"Use Extensions\" in the Extensions app.";
pub const EXTENSION_UNSUPPORTED: &str = "Snappy needs GNOME 46 or newer.";
pub const EXTENSION_NOT_INSTALLED: &str = "Snappy couldn't install its GNOME extension. Check that your home folder \
     is writable, then quit and reopen Snappy.";
pub const EXTENSION_STARTING: &str = "Snappy's GNOME extension is starting.";

/// The extension failed to start, with GNOME Shell's own explanation.
pub fn extension_error(detail: &str, desktop: &str) -> String {
    let detail = detail.trim().trim_end_matches('.');
    if detail.is_empty() {
        format!(
            "Snappy's GNOME extension couldn't start. Log out of {desktop} and sign back in to try again."
        )
    } else {
        format!(
            "Snappy's GNOME extension couldn't start: {detail}. Log out of {desktop} and sign back in to try again."
        )
    }
}

pub const ENHANCED_TILING: &str = "Ubuntu's Enhanced Tiling also snaps windows you drag to screen edges. Turn it \
     off in Settings → Ubuntu Desktop so Snappy's zones work alone.";
pub const TILING_ASSISTANT: &str = "The Tiling Assistant extension also snaps windows you drag to screen edges. \
     Turn it off in the Extensions app so Snappy's zones work alone.";
pub const LOG_OUT_FAILED: &str = "Snappy couldn't ask GNOME to log out. Log out from the menu at the top right of \
     the screen instead.";

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}
