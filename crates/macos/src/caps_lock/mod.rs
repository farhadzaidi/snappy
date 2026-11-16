//! Caps Lock as the Snappy key. macOS remaps Caps Lock to F18 for every keyboard through
//! the HID event system's user key mapping, so Caps Lock never turns on by itself and its
//! light stays off; `tap` makes F18 held stand for the Snappy key; and a tap turns Caps Lock
//! on or off through the HID system. macOS forgets the mapping at restart, and the engine
//! sets it again at launch.
//!
//! The mapping is changed off the main thread, since each change is a round trip to the HID
//! service; the hotkey and the tap live on the main thread.
//!
//! The App Sandbox keeps the Mac App Store build away from the HID system, so that build
//! has `unavailable` in place of all of this.

#[cfg(not(feature = "mac-app-store"))]
mod hid;
#[cfg(not(feature = "mac-app-store"))]
mod holding;
#[cfg(not(feature = "mac-app-store"))]
mod key_mapping;
#[cfg(not(feature = "mac-app-store"))]
mod modifier_keys;
#[cfg(not(feature = "mac-app-store"))]
mod tap;
#[cfg(not(feature = "mac-app-store"))]
mod toggle;
#[cfg(feature = "mac-app-store")]
mod unavailable;

#[cfg(not(feature = "mac-app-store"))]
pub use holding::{UNAVAILABLE, hold_as, restore, undo, unmap};
#[cfg(not(feature = "mac-app-store"))]
pub use tap::{key_changed, stop};
#[cfg(not(feature = "mac-app-store"))]
pub use toggle::toggle;
#[cfg(feature = "mac-app-store")]
pub use unavailable::{UNAVAILABLE, hold_as, restore, stop, toggle, undo, unmap};
