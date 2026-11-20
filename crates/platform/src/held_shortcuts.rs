//! Turning the presses and releases an OS reports for a shortcut into what the engine hears:
//! one `HotKey` for each press, and for a shortcut that reports being let go, one
//! `HotKeyReleased` after it, however the OS repeats the key or misses its release.

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::events::PlatformEvent;
use crate::hotkeys::HotKeyRequest;

/// Longest a held key goes between the presses the OS reports as it repeats, the first
/// repeat after the press included: KDE waits 600 ms by default before a key repeats.
const REPEAT_WITHIN: Duration = Duration::from_secs(1);

/// Whether the OS reports a held shortcut pressed again as its key repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRepeat {
    /// It does, so a press while the shortcut is down, within `REPEAT_WITHIN` of the last,
    /// is its key repeating.
    Reported,
    /// It never does, or the adapter tells a repeat apart itself and never passes it on.
    Never,
}

/// Which registered shortcuts report being let go, and which of those are down, each with
/// when it was last reported pressed.
#[derive(Debug)]
pub struct HeldShortcuts {
    repeat: KeyRepeat,
    reporting: BTreeSet<u32>,
    down: BTreeMap<u32, Instant>,
}

impl HeldShortcuts {
    pub fn new(repeat: KeyRepeat) -> Self {
        Self {
            repeat,
            reporting: BTreeSet::new(),
            down: BTreeMap::new(),
        }
    }

    /// Follow `shortcuts` from now on, forgetting any still down: once its shortcut is
    /// registered again, a release in progress may never come.
    pub fn register(&mut self, shortcuts: &[HotKeyRequest]) {
        self.reporting = shortcuts
            .iter()
            .filter(|shortcut| shortcut.reports_release)
            .map(|shortcut| shortcut.id)
            .collect();
        self.down.clear();
    }

    /// What a press of shortcut `id` at `at` reports: nothing when it's its key repeating,
    /// and else the press. A press while it's down that isn't a repeat means its last
    /// release went unseen; that one stays unreported, so the engine takes the press for a
    /// new one rather than for the end of a hold.
    pub fn pressed(&mut self, id: u32, at: Instant) -> Option<PlatformEvent> {
        if !self.reporting.contains(&id) {
            return Some(PlatformEvent::HotKey(id));
        }
        let repeating = self.repeat == KeyRepeat::Reported
            && self
                .down
                .get(&id)
                .is_some_and(|last| at.saturating_duration_since(*last) < REPEAT_WITHIN);
        self.down.insert(id, at);
        (!repeating).then_some(PlatformEvent::HotKey(id))
    }

    /// What a release of shortcut `id` reports: its release once, if it reports one.
    pub fn released(&mut self, id: u32) -> Option<PlatformEvent> {
        self.down.remove(&id).map(|_| PlatformEvent::HotKeyReleased(id))
    }

    /// Whether shortcut `id` reports being let go and is down.
    pub fn is_down(&self, id: u32) -> bool {
        self.down.contains_key(&id)
    }

    /// Whether any shortcut that reports being let go is down.
    pub fn any_down(&self) -> bool {
        !self.down.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{KeyChord, SnappyKey};

    use super::*;

    const FOCUS: u32 = 4;
    const SEND_TO_BACK: u32 = 9;

    fn held(repeat: KeyRepeat) -> HeldShortcuts {
        let combo = |code: &str| KeyChord::snappy(code, []).combo(SnappyKey::DEFAULT);
        let mut held = HeldShortcuts::new(repeat);
        held.register(&[
            HotKeyRequest::new(FOCUS, combo("KeyJ")),
            HotKeyRequest {
                reports_release: true,
                ..HotKeyRequest::new(SEND_TO_BACK, combo("KeyB"))
            },
        ]);
        held
    }

    fn after(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn a_shortcut_reports_one_press_and_one_release_only_when_it_asks() {
        let now = Instant::now();
        for repeat in [KeyRepeat::Reported, KeyRepeat::Never] {
            let mut held = held(repeat);
            assert_eq!(
                held.pressed(SEND_TO_BACK, now),
                Some(PlatformEvent::HotKey(SEND_TO_BACK))
            );
            assert_eq!(
                held.released(SEND_TO_BACK),
                Some(PlatformEvent::HotKeyReleased(SEND_TO_BACK))
            );
            assert_eq!(held.released(SEND_TO_BACK), None, "{repeat:?}");
            assert_eq!(held.pressed(FOCUS, now), Some(PlatformEvent::HotKey(FOCUS)));
            assert_eq!(held.pressed(FOCUS, now), Some(PlatformEvent::HotKey(FOCUS)));
            assert_eq!(held.released(FOCUS), None, "{repeat:?}: it doesn't ask");
        }
    }

    #[test]
    fn where_the_os_repeats_a_held_key_its_presses_report_nothing_more() {
        let start = Instant::now();
        let mut held = held(KeyRepeat::Reported);
        held.pressed(SEND_TO_BACK, start);
        assert_eq!(
            held.pressed(SEND_TO_BACK, after(start, 600)),
            None,
            "the first repeat"
        );
        for repeat in 1..=40 {
            assert_eq!(held.pressed(SEND_TO_BACK, after(start, 600 + repeat * 40)), None);
        }
        assert_eq!(
            held.released(SEND_TO_BACK),
            Some(PlatformEvent::HotKeyReleased(SEND_TO_BACK))
        );
    }

    #[test]
    fn where_the_os_repeats_a_held_key_a_press_long_after_the_last_is_a_new_press() {
        let start = Instant::now();
        let mut held = held(KeyRepeat::Reported);
        held.pressed(SEND_TO_BACK, start);
        let later = after(start, 3000);
        assert_eq!(
            held.pressed(SEND_TO_BACK, later),
            Some(PlatformEvent::HotKey(SEND_TO_BACK)),
            "its release went unseen, and it's no repeat this long after"
        );
        assert_eq!(
            held.pressed(SEND_TO_BACK, after(start, 3600)),
            None,
            "its key repeating"
        );
        assert_eq!(
            held.released(SEND_TO_BACK),
            Some(PlatformEvent::HotKeyReleased(SEND_TO_BACK))
        );
    }

    #[test]
    fn where_the_os_never_repeats_a_press_while_down_is_a_new_press_reported_alone() {
        let start = Instant::now();
        let mut held = held(KeyRepeat::Never);
        held.pressed(SEND_TO_BACK, start);
        assert_eq!(
            held.pressed(SEND_TO_BACK, after(start, 300)),
            Some(PlatformEvent::HotKey(SEND_TO_BACK)),
            "the Snappy key, or Caps Lock, let go first, and the release went unseen: \
             reported let go, the press would end a hold"
        );
        assert_eq!(
            held.released(SEND_TO_BACK),
            Some(PlatformEvent::HotKeyReleased(SEND_TO_BACK))
        );
    }

    #[test]
    fn a_release_reported_as_the_snappy_key_comes_up_leaves_the_next_press_a_press() {
        let now = Instant::now();
        let mut held = held(KeyRepeat::Reported);
        held.pressed(SEND_TO_BACK, now);
        assert_eq!(
            held.released(SEND_TO_BACK),
            Some(PlatformEvent::HotKeyReleased(SEND_TO_BACK))
        );
        assert_eq!(
            held.released(SEND_TO_BACK),
            None,
            "its key coming up after reports nothing"
        );
        assert_eq!(
            held.pressed(SEND_TO_BACK, now),
            Some(PlatformEvent::HotKey(SEND_TO_BACK))
        );
    }

    #[test]
    fn registering_again_forgets_a_shortcut_still_down() {
        let now = Instant::now();
        for repeat in [KeyRepeat::Reported, KeyRepeat::Never] {
            let mut held = held(repeat);
            held.pressed(SEND_TO_BACK, now);
            held.register(&[HotKeyRequest {
                reports_release: true,
                ..HotKeyRequest::new(
                    SEND_TO_BACK,
                    KeyChord::snappy("KeyB", []).combo(SnappyKey::DEFAULT),
                )
            }]);
            assert_eq!(held.released(SEND_TO_BACK), None, "{repeat:?}");
            assert_eq!(
                held.pressed(SEND_TO_BACK, now),
                Some(PlatformEvent::HotKey(SEND_TO_BACK)),
                "{repeat:?}"
            );
        }
    }
}
