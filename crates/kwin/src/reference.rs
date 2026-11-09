//! Read by the live-test harness, so its expectations use Snappy's own view of keys.

use snappy_core::keys::Combo;

/// The Qt key code a shortcut is registered with.
pub fn key_code(combo: &Combo) -> Option<i32> {
    crate::keys::key_code(combo)
}
