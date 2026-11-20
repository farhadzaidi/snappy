//! Global shortcut registration.

use snappy_core::keys::Combo;

/// A shortcut to register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotKeyRequest {
    /// The ID its events carry.
    pub id: u32,
    pub combo: Combo,
    /// Report letting go of the shortcut too, with `HotKeyReleased`, for a shortcut whose
    /// hold does something more as it's let go. The keyboard is watched for it only while the
    /// shortcut is down.
    pub reports_release: bool,
}

impl HotKeyRequest {
    /// A shortcut that reports only its presses.
    pub fn new(id: u32, combo: Combo) -> Self {
        Self {
            id,
            combo,
            reports_release: false,
        }
    }
}

/// Global shortcuts. Each arrives as a `Combo`: the key and the concrete modifiers to
/// hold, with the Snappy key already expanded, so adapters never deal with it.
pub trait HotKeys {
    /// Register shortcuts, replacing any registered before. Returns the IDs refused.
    fn register(&mut self, shortcuts: &[HotKeyRequest]) -> Vec<u32>;
    fn unregister_all(&mut self);
    /// Whether the system and other apps leave `combo` free.
    fn is_available(&mut self, combo: &Combo) -> bool;
}
