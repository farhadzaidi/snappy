//! Caps Lock as the Snappy key. macOS remaps Caps Lock to F18 for every keyboard through
//! the HID event system's user key mapping, so Caps Lock never turns on by itself and its
//! light stays off; `tap` makes F18 held stand for the Snappy key; and a tap turns Caps Lock
//! on or off through the HID system. macOS forgets the mapping at restart, and the engine
//! sets it again at launch.
//!
//! The mapping is changed off the main thread, since each change is a round trip to the HID
//! service; the hotkey and the tap live on the main thread.

mod hid;
mod holding;
mod key_mapping;
mod modifier_keys;
mod tap;
mod toggle;

pub use holding::{hold_as, restore, undo, unmap};
pub use tap::{key_changed, stop};
pub use toggle::toggle;
