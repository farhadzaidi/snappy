//! Hand-written declarations for the C APIs the `objc2` crates don't cover:
//! Accessibility, Carbon hotkeys, the display UUID lookup, and `kill`.

pub mod ax;
pub mod carbon;

use objc2_core_foundation::CFType;

/// Core Foundation's `Boolean`: an unsigned char holding 0 or 1.
pub type Boolean = u8;
pub type Pid = i32;

/// `ESRCH` from `errno.h`.
pub const NO_SUCH_PROCESS: i32 = 3;

#[link(name = "ColorSync", kind = "framework")]
unsafe extern "C" {
    /// Returns a +1 `CFUUIDRef`, or null for an unknown display.
    pub fn CGDisplayCreateUUIDFromDisplayID(display: u32) -> *const CFType;
}

unsafe extern "C" {
    pub fn kill(pid: Pid, signal: i32) -> i32;
}
