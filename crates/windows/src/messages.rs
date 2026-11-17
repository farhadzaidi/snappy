//! User-facing messages for refusals and errors, worded the same on every platform.

use snappy_core::error::WindowError;

pub use snappy_core::eligibility::NOT_STANDARD;

pub const ELEVATED: &str = "Snappy can't move windows of apps running as administrator.";
pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const CAPS_LOCK_REMAPPED: &str = "Caps Lock is already remapped by the Scancode Map in the registry. Remove that to use Caps Lock as the Snappy key.";
pub const CAPS_LOCK_UNAVAILABLE: &str = "Snappy couldn't watch the keyboard for Caps Lock. Try again.";

pub const LOGIN_ITEM_NOT_CHANGED: &str = "Snappy couldn't change Open at login.";
pub const LOGIN_ITEM_OFF_IN_SETTINGS: &str =
    "Snappy is turned off in Settings > Apps > Startup. Turn it on there to open Snappy at login.";
pub const LOGIN_ITEM_BLOCKED_BY_POLICY: &str = "Your organization doesn't let Snappy open at login.";
pub const LOGIN_ITEM_REQUIRED_BY_POLICY: &str =
    "Your organization has Snappy open at login, so it can't be turned off here.";

/// What `snappy --uninstall` says about Open at login.
pub const LOGIN_ITEM_REMOVED: &str = "Turned off Open at login.";
pub const LOGIN_ITEM_NOT_REMOVED: &str =
    "Snappy couldn't turn off Open at login. Turn it off in Settings > Apps > Startup.";

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}
