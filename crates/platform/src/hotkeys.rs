//! Global shortcut registration.

use snappy_core::keys::Combo;

/// Global shortcuts. Each arrives as a `Combo`: the key and the concrete modifiers to
/// hold, with the Snappy key already expanded, so adapters never deal with it.
pub trait HotKeys {
    /// Register shortcuts, replacing any registered before. Returns the ones refused.
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32>;
    fn unregister_all(&mut self);
    /// Whether the system and other apps leave `combo` free.
    fn is_available(&mut self, combo: &Combo) -> bool;
}
