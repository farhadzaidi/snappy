//! User-facing messages for refusals and errors, worded the same on every platform.

use snappy_core::error::WindowError;

pub use snappy_core::eligibility::NOT_STANDARD;

pub const NO_PERMISSION: &str = "Enable Accessibility for Snappy in System Settings first.";
pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const NOT_RESPONDING: &str = "The app isn't responding. Try again when it is.";
pub const CLOSED_OR_REVOKED: &str = "The window is closed or Accessibility access was revoked.";
pub const UNAVAILABLE_OR_FULL_SCREEN: &str = "The window is unavailable or in full screen.";

#[cfg(not(feature = "mac-app-store"))]
pub const CAPS_LOCK_IN_KEY_MAPPING: &str = "Caps Lock is already remapped by a key mapping (hidutil or a keyboard app). Remove that to use Caps Lock as the Snappy key.";
#[cfg(not(feature = "mac-app-store"))]
pub const CAPS_LOCK_IN_MODIFIER_KEYS: &str = "Caps Lock is already remapped by System Settings (Keyboard Shortcuts, Modifier Keys). Remove that to use Caps Lock as the Snappy key.";
#[cfg(not(feature = "mac-app-store"))]
pub const CAPS_LOCK_UNMAPPABLE: &str = "macOS didn't let Snappy remap Caps Lock. Try again in a moment.";
#[cfg(not(feature = "mac-app-store"))]
pub const CAPS_LOCK_UNWATCHED: &str =
    "Snappy couldn't watch Caps Lock. Check that Accessibility is on for Snappy in System Settings.";
#[cfg(feature = "mac-app-store")]
pub const CAPS_LOCK_SANDBOXED: &str =
    "Apps from the App Store can't remap Caps Lock, so it can't be the Snappy key.";

pub const SWAP_NEEDS_PERMISSION: &str = "Accessibility access is required to swap windows.";
pub const SWAP_LIST_UNREADABLE: &str = "Couldn't check which window occupies the zone.";
pub const SWAP_UNSUPPORTED_OCCUPANT: &str = "An unsupported app occupies that zone. The swap was cancelled.";
pub const SWAP_NOT_RESPONDING: &str = "An app in that zone isn't responding. The swap was cancelled.";
pub const SWAP_NOT_SWAPPABLE: &str = "A window in that zone doesn't allow swapping. The drag was cancelled.";
pub const SWAP_UNIDENTIFIED: &str = "The window in that zone couldn't be identified safely. Try again.";

/// What `snappy --uninstall` says about Open at login.
pub const LOGIN_ITEM_REMOVED: &str = "Turned off Open at login.";
pub const LOGIN_ITEM_NOT_REMOVED: &str =
    "Snappy couldn't turn off Open at login. Turn it off in System Settings > General > Login Items.";

/// An app rejected a position or size write.
pub fn declined(code: i32) -> WindowError {
    WindowError::refused(format!(
        "The app declined the window move (Accessibility error {code})."
    ))
}

pub fn unavailable(message: &str) -> WindowError {
    WindowError::unavailable(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declined_names_the_accessibility_error() {
        assert_eq!(
            declined(-25204).to_string(),
            "The app declined the window move (Accessibility error -25204)."
        );
        assert!(matches!(declined(-25200), WindowError::Refused(_)));
    }

    #[test]
    fn unavailable_messages_are_shown_as_written() {
        assert_eq!(
            unavailable(NO_FOCUSED_WINDOW).to_string(),
            "No movable window is focused."
        );
        assert_eq!(
            unavailable(SWAP_UNIDENTIFIED).to_string(),
            "The window in that zone couldn't be identified safely. Try again."
        );
    }
}
