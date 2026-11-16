//! What Caps Lock is doing for Snappy, kept in `Shared` for the event and engine threads.

use serde::{Deserialize, Serialize};

use super::key_definition::KeyDefinition;

/// What Caps Lock is doing for Snappy.
#[derive(Debug, Default)]
pub struct CapsLockState {
    pub(super) held: Option<Held>,
    /// Caps Lock is down: its press arrived and its release hasn't.
    pub(super) down: bool,
    /// Something else was used during this press, and the engine was told.
    pub(super) other_input: bool,
}

/// Caps Lock held as the Snappy key: its key and the definition to put back, which is
/// also what a later run needs to put it back.
#[derive(Debug, Serialize, Deserialize)]
pub struct Held {
    pub(super) keycode: u8,
    pub(super) original: KeyDefinition,
}
