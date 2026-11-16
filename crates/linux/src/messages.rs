//! User-facing messages the Linux adapters share.

pub use snappy_core::eligibility::{FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD};

/// The refusal when the keyboard layout itself makes Caps Lock another key.
pub const LAYOUT_REMAPS_CAPS_LOCK: &str = "Caps Lock is already remapped by the keyboard layout. Choose a layout that \
     keeps Caps Lock to use Caps Lock as the Snappy key.";
