//! Caps Lock as the Snappy key, through the low-level keyboard hook: nothing is installed
//! or written, and nothing outlives the process.
//!
//! The hook swallows Caps Lock, so Windows never turns it on, and reports its presses.
//! While it is down, a key matching a Snappy shortcut with the Snappy key's modifiers
//! fires that shortcut directly, without injecting modifier keys that Windows would act
//! on, such as the Windows key opening Start. A tap turns Caps Lock on or off by injecting
//! a Caps Lock press marked as Snappy's, which the hook lets through. The mouse hook runs
//! only while Caps Lock is down, to notice a button.
//!
//! Every key reaches the hook while this is on; only Caps Lock is looked at until it is
//! held, and then only to tell another key was used, never which, and to find its
//! shortcut.

mod holding;
mod keys;
mod scancode_map;
mod toggle;

pub use keys::CapsLockKeys;
pub use scancode_map::system_remaps_caps_lock;
pub use toggle::toggle;
