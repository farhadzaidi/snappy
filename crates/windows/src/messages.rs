//! User-facing messages for refusals and errors, worded the same on every platform.

use snappy_core::error::WindowError;

pub use snappy_core::eligibility::NOT_STANDARD;

pub const ELEVATED: &str = "Snappy can't move windows of apps running as administrator.";
pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const CAPS_LOCK_REMAPPED: &str = "Caps Lock is already remapped by the Scancode Map in the registry. Remove that to use Caps Lock as the Snappy key.";
pub const CAPS_LOCK_UNAVAILABLE: &str = "Snappy couldn't watch the keyboard for Caps Lock. Try again.";

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}
