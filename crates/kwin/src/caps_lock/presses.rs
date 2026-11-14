//! Telling a press of Caps Lock from the Snappy key's own modifier keys on KDE Plasma.
//!
//! Held, Caps Lock sets every modifier of the Snappy key with one key's action, so KWin's
//! keystate reports them all becoming held in one report. The modifier keys themselves are
//! separate keys, and KWin handles one key event at a time and reports after each, so they
//! become held one report after another. Letting go of either alone fires the same
//! modifier-only shortcut of KDE's shortcut service; only one that follows a Caps Lock
//! press is a release of Caps Lock.

use snappy_core::keys::Mods;

/// What the reports and the shortcut have said since Caps Lock last went down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presses {
    snappy_key: Mods,
    /// The modifiers held as of the last report; `None` until the first, which only says
    /// what was already held when the watch began.
    held: Option<Mods>,
    /// Caps Lock went down and its modifier-only shortcut hasn't fired since.
    open: bool,
}

impl Presses {
    pub fn new(snappy_key: Mods) -> Presses {
        Presses {
            snappy_key,
            held: None,
            open: false,
        }
    }

    /// A complete report of the held modifiers. True when it is Caps Lock going down: at
    /// least two of the Snappy key's modifiers became held at once. Any of them becoming held
    /// on its own is a modifier key, which ends a press of Caps Lock still open, as one whose
    /// release came with another key or a button.
    pub fn report(&mut self, held: Mods) -> bool {
        let Some(before) = self.held.replace(held) else {
            return false;
        };
        let added = held.without(before);
        let added_of_the_key = Mods::from_iter(
            added
                .iter()
                .filter(|modifier| self.snappy_key.contains(*modifier)),
        );
        match added_of_the_key.count() {
            0 => false,
            1 => {
                self.open = false;
                false
            }
            _ => {
                self.open = true;
                true
            }
        }
    }

    /// The modifier-only shortcut fired: the Snappy key's modifiers came up with nothing else
    /// used meanwhile. True when that was Caps Lock coming up. The shortcut's signal and the
    /// report of the modifiers coming up travel on different connections, so either may come
    /// first; only the open press counts.
    pub fn released_alone(&mut self) -> bool {
        std::mem::replace(&mut self.open, false)
    }
}

#[cfg(test)]
mod tests {
    use snappy_core::keys::{Modifier, SnappyKey};

    use super::*;

    fn mods(list: &[Modifier]) -> Mods {
        list.iter().copied().collect()
    }

    fn watching() -> Presses {
        let mut presses = Presses::new(SnappyKey::DEFAULT.modifiers());
        assert!(
            !presses.report(Mods::NONE),
            "the first report only sets the start"
        );
        presses
    }

    #[test]
    fn caps_lock_sets_every_modifier_at_once_and_its_release_alone_is_a_release() {
        let mut presses = watching();
        assert!(presses.report(SnappyKey::DEFAULT.modifiers()));
        assert!(presses.released_alone());
        assert!(!presses.report(Mods::NONE));
        assert!(!presses.released_alone(), "once only");
    }

    #[test]
    fn the_release_counts_whether_the_shortcut_or_the_report_comes_first() {
        let mut presses = watching();
        presses.report(SnappyKey::DEFAULT.modifiers());
        presses.report(Mods::NONE);
        assert!(presses.released_alone());
    }

    #[test]
    fn the_modifier_keys_pressed_one_by_one_are_never_caps_lock() {
        let mut presses = watching();
        assert!(!presses.report(mods(&[Modifier::Ctrl])));
        assert!(!presses.report(mods(&[Modifier::Ctrl, Modifier::Alt])));
        assert!(!presses.report(SnappyKey::DEFAULT.modifiers()));
        assert!(
            !presses.released_alone(),
            "a tap of Ctrl+Alt+Super isn't a tap of Caps Lock"
        );
    }

    #[test]
    fn a_modifier_key_after_a_caps_lock_press_that_never_came_up_alone_ends_it() {
        let mut presses = watching();
        presses.report(SnappyKey::DEFAULT.modifiers());
        presses.report(Mods::NONE);
        presses.report(mods(&[Modifier::Ctrl]));
        presses.report(mods(&[Modifier::Ctrl, Modifier::Alt]));
        presses.report(mods(&[Modifier::Ctrl, Modifier::Alt, Modifier::Meta]));
        assert!(!presses.released_alone());
    }

    #[test]
    fn modifiers_held_when_watching_begins_are_not_a_press() {
        let mut presses = Presses::new(SnappyKey::DEFAULT.modifiers());
        assert!(!presses.report(SnappyKey::DEFAULT.modifiers()));
        assert!(!presses.released_alone());
    }

    #[test]
    fn caps_lock_with_another_modifier_already_held_still_counts() {
        let mut presses = watching();
        presses.report(mods(&[Modifier::Shift]));
        assert!(presses.report(mods(&[
            Modifier::Shift,
            Modifier::Ctrl,
            Modifier::Alt,
            Modifier::Meta
        ])));
    }
}
