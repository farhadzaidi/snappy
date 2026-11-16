//! Hand-written declarations for the C APIs the `objc2` crates don't cover:
//! Accessibility, Carbon hotkeys, the HID system, and the display UUID lookup.

pub mod ax;
pub mod carbon;
mod colorsync;
#[cfg(not(feature = "mac-app-store"))]
pub mod hid;
mod types;

pub use colorsync::CGDisplayCreateUUIDFromDisplayID;
pub use types::{Boolean, Pid};
