//! macOS timings and limits: Accessibility timeouts, permission checks, window and tab
//! matching, and the Chrome workaround.

use std::time::Duration;

/// Accessibility messaging timeout, in seconds, for single elements and writes.
pub const AX_MESSAGING_TIMEOUT: f32 = 0.25;
/// Accessibility messaging timeout, in seconds, for listing an app's windows.
pub const AX_ENUMERATION_TIMEOUT: f32 = 0.15;
/// Accessibility messaging timeout, in seconds, for listing every window, which journal
/// replay relies on.
pub const AX_RECOVERY_TIMEOUT: f32 = 0.5;
/// How many parents to walk up from the element under the pointer to find its window.
pub const POINTER_HIT_WALK: usize = 12;
/// How often to check for an Accessibility grant while one is awaited.
pub const PERMISSION_POLL: Duration = Duration::from_secs(1);
/// Trust settles shortly after its notification arrives.
pub const PERMISSION_RECHECK_DELAY: Duration = Duration::from_millis(500);
/// Largest difference between an Accessibility frame and a window server frame, on each
/// of x, y, width, and height, for both to count as the same window.
pub const WINDOW_LIST_MATCH: f64 = 3.0;
/// Most a native tab group's frame differs from where one of its tabs went out of sight, at
/// the top or the bottom: the group's first new tab adds a tab bar, about 28 points tall,
/// and a new tab can push the group up to keep it on screen.
pub const TAB_BAR_HEIGHT_LIMIT: f64 = 40.0;
/// How long Chrome needs after enhanced accessibility is turned back on before its
/// window's frame is final.
pub const CHROME_RESTORE_DELAY: Duration = Duration::from_millis(25);
