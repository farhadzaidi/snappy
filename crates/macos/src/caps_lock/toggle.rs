//! Turning Caps Lock on or off for a tap.

use super::hid;

/// Turn Caps Lock on or off, as a tap of the key does.
pub fn toggle() -> Result<(), String> {
    hid::toggle_caps_lock()
        .then_some(())
        .ok_or_else(|| "the HID system didn't change the Caps Lock state".to_owned())
}
