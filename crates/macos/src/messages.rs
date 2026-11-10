//! User-facing messages for refusals and errors, worded the same on every platform.

use snappy_core::error::WindowError;

pub const FULL_SCREEN: &str = "Leave full screen before tiling this window.";
pub const NOT_STANDARD: &str = "Choose a standard application window.";
pub const NOT_MOVABLE: &str = "This window does not allow moving and resizing.";
pub const NO_PERMISSION: &str = "Enable Accessibility for Snappy in System Settings first.";
pub const NO_FOCUSED_WINDOW: &str = "No movable window is focused.";
pub const WINDOW_GONE: &str = "The window is no longer available.";
pub const NOT_RESPONDING: &str = "The app is not responding. Try again when it is.";
pub const CLOSED_OR_REVOKED: &str = "The window is closed or Accessibility access was revoked.";
pub const UNAVAILABLE_OR_FULL_SCREEN: &str = "The window is unavailable or in full screen.";

pub const SWAP_NEEDS_PERMISSION: &str = "Accessibility access is required to swap windows.";
pub const SWAP_LIST_UNREADABLE: &str = "Could not check which window occupies the zone.";
pub const SWAP_UNSUPPORTED_OCCUPANT: &str = "An unsupported app occupies that zone. The swap was cancelled.";
pub const SWAP_NOT_RESPONDING: &str = "An app in that zone is not responding. The swap was cancelled.";
pub const SWAP_NOT_SWAPPABLE: &str = "A window in that zone does not allow swapping. The drag was cancelled.";
pub const SWAP_UNIDENTIFIED: &str = "The window in that zone could not be identified safely. Try again.";

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
            "The window in that zone could not be identified safely. Try again."
        );
    }
}
