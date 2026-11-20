//! Shortcut registration on the fake platform, with chords other apps can hold.

use snappy_core::keys::Combo;
use snappy_platform::{HotKeyRequest, HotKeys};

use crate::platform::FakePlatform;

impl HotKeys for FakePlatform {
    fn register(&mut self, shortcuts: &[HotKeyRequest]) -> Vec<u32> {
        self.registrations += 1;
        let (refused, accepted): (Vec<&HotKeyRequest>, Vec<&HotKeyRequest>) = shortcuts
            .iter()
            .partition(|shortcut| self.taken_elsewhere.contains(&shortcut.combo));
        self.registered = accepted
            .iter()
            .map(|shortcut| (shortcut.id, shortcut.combo.clone()))
            .collect();
        self.reporting_release = accepted
            .iter()
            .filter(|shortcut| shortcut.reports_release)
            .map(|shortcut| shortcut.id)
            .collect();
        refused.into_iter().map(|shortcut| shortcut.id).collect()
    }

    fn unregister_all(&mut self) {
        self.registered.clear();
        self.reporting_release.clear();
    }

    /// A chord Snappy holds itself reads as taken, as macOS's Carbon hotkeys report it.
    fn is_available(&mut self, combo: &Combo) -> bool {
        !self.taken_elsewhere.contains(combo) && !self.registered.iter().any(|(_, held)| held == combo)
    }
}
