//! User-facing messages for refusals and errors, worded the same on every platform where the
//! situation is the same.

use snappy_core::error::WindowError;
pub use snappy_linux::messages::NOT_STANDARD;

pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const SHELL_UNREACHABLE: &str = "Snappy can't reach GNOME right now. Try again in a moment.";
pub const ON_ALL_WORKSPACES: &str = "This window shows on every workspace, so Snappy can't hide it.";

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
pub const EXTENSION_NOT_INSTALLED: &str = "Snappy couldn't finish setting up. Quit and reopen Snappy.";
pub const EXTENSION_STARTING: &str = "Snappy's GNOME extension is starting.";

/// The extension failed to start. GNOME Shell's explanation goes to the daemon's output.
pub fn extension_error(desktop: &str) -> String {
    format!("Snappy's GNOME extension couldn't start. Log out of {desktop} and sign back in to try again.")
}

pub const ENHANCED_TILING: &str = "Ubuntu's Enhanced Tiling also snaps windows you drag to screen edges. Turn it \
     off in Settings → Ubuntu Desktop so Snappy's zones work alone.";
pub const TILING_ASSISTANT: &str = "The Tiling Assistant extension also snaps windows you drag to screen edges. \
     Turn it off in the Extensions app so Snappy's zones work alone.";
pub const LOG_OUT_FAILED: &str = "Snappy couldn't ask GNOME to log out. Log out from the menu at the top right of \
     the screen instead.";

pub const HIDE_REFUSED: &str = "GNOME didn't let Snappy hide this window.";
pub const UNEXPECTED_REPLY: &str = "Snappy and its GNOME extension don't agree. Quit and reopen Snappy.";

pub const CAPS_LOCK_UNSUPPORTED: &str = "This version of GNOME can't use Caps Lock as the Snappy key yet.";

/// Snappy's keyboard option couldn't be written to the user's XKB folder.
pub fn caps_lock_not_written(error: &std::io::Error) -> String {
    format!("Snappy couldn't save the keyboard setting that makes Caps Lock the Snappy key: {error}.")
}

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}

/// What `snappy --uninstall` says about the extension.
pub const EXTENSION_REMOVED: &str = "Removed Snappy's GNOME extension.";
pub const EXTENSION_NOT_REMOVED: &str =
    "Snappy couldn't remove its GNOME extension. Remove it in the Extensions app.";
