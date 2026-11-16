//! The display UUID lookup, from ColorSync.

use objc2_core_foundation::CFType;

#[link(name = "ColorSync", kind = "framework")]
unsafe extern "C" {
    /// Returns a +1 `CFUUIDRef`, or null for an unknown display.
    pub fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> *const CFType;
}
