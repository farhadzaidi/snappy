//! Shortcut registration on the fake platform, with chords other apps can hold.

use snappy_core::keys::Combo;
use snappy_platform::HotKeys;

use crate::platform::FakePlatform;

impl HotKeys for FakePlatform {
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32> {
        self.registrations += 1;
        let (refused, accepted): (Vec<_>, Vec<_>) = keys
            .iter()
            .cloned()
            .partition(|(_, combo)| self.taken_elsewhere.contains(combo));
        self.registered = accepted;
        refused.into_iter().map(|(id, _)| id).collect()
    }

    fn unregister_all(&mut self) {
        self.registered.clear();
    }

    /// A chord Snappy holds itself reads as taken, as macOS's Carbon hotkeys report it.
    fn is_available(&mut self, combo: &Combo) -> bool {
        !self.taken_elsewhere.contains(combo) && !self.registered.iter().any(|(_, held)| held == combo)
    }
}
