//! Keyboard shortcuts as physical keys plus modifiers, independent of any platform.
//! A chord names the key's physical position, so it stays on the same key on every
//! keyboard layout.

pub mod caps_lock;
mod chord;
mod chord_mods;
mod key_code;
mod modifiers;
mod os;
mod reserved;
mod snappy_key;
#[cfg(test)]
mod test_support;

pub use chord::{Combo, InvalidChord, KeyChord, LabelParts, MAX_LABEL_LENGTH};
pub use chord_mods::ChordMods;
pub use key_code::KeyCode;
pub use modifiers::{Modifier, Mods};
pub use os::Os;
pub use reserved::{KeptChord, chords_the_system_keeps, reserved_app_commands};
pub use snappy_key::{SNAPPY_KEY_NAME, SnappyKey, SnappyKeyError};
