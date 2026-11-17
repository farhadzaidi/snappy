//! User-facing messages the Linux adapters share.

pub use snappy_core::eligibility::{FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD};

/// The refusal when the keyboard layout itself makes Caps Lock another key.
pub const LAYOUT_REMAPS_CAPS_LOCK: &str = "Caps Lock is already remapped by the keyboard layout. Choose a layout that \
     keeps Caps Lock to use Caps Lock as the Snappy key.";

/// What `snappy --uninstall` says it removed, or couldn't.
pub const LOGIN_ITEM_REMOVED: &str = "Turned off Open at login.";
pub const LOGIN_ITEM_NOT_REMOVED: &str = "Snappy couldn't turn off Open at login.";
pub const KEYBOARD_OPTION_REMOVED: &str = "Removed Snappy's Caps Lock keyboard option.";
pub const KEYBOARD_OPTION_NOT_REMOVED: &str = "Snappy couldn't remove its Caps Lock keyboard option.";
