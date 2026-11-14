//! Caps Lock as the Snappy key: held, it stands for the Snappy key's modifiers; tapped
//! alone, it turns Caps Lock on or off as usual. What a press was, and what has to change
//! for Caps Lock to behave as the settings ask, are decided here.

use crate::keys::Mods;

/// The longest press that still counts as a tap. Caps Lock held longer and let go without
/// another key was a Snappy shortcut thought better of, not a tap.
pub const TAP_LIMIT: f64 = 0.5;

/// One press of Caps Lock while it is the Snappy key, from going down to coming up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CapsLockPress {
    pressed_at: f64,
    other_input: bool,
}

/// What a press of Caps Lock turned out to be once it came up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsLockRelease {
    /// Pressed and let go alone and quickly: Caps Lock turns on or off.
    Tap,
    /// Held as the Snappy key, or held too long to be a tap: nothing more happens.
    Hold,
}

impl CapsLockPress {
    /// Caps Lock went down at `at`, in seconds.
    pub fn new(at: f64) -> Self {
        Self {
            pressed_at: at,
            other_input: false,
        }
    }

    /// Another key or a mouse button was used while Caps Lock was down.
    pub fn note_other_input(&mut self) {
        self.other_input = true;
    }

    pub fn other_input(&self) -> bool {
        self.other_input
    }

    /// How long Caps Lock has been down at `at`.
    pub fn held_for(&self, at: f64) -> f64 {
        (at - self.pressed_at).max(0.0)
    }

    /// Caps Lock came up at `at`: a tap when nothing else was used and it wasn't held past
    /// `TAP_LIMIT`.
    pub fn release(self, at: f64) -> CapsLockRelease {
        if !self.other_input && self.held_for(at) <= TAP_LIMIT {
            CapsLockRelease::Tap
        } else {
            CapsLockRelease::Hold
        }
    }
}

/// What Snappy has done to Caps Lock, as far as this run knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsLockState {
    /// Caps Lock behaves as usual.
    Normal,
    /// An earlier run changed Caps Lock and ended without changing it back, as a crash does.
    LeftOver,
    /// Caps Lock, held, stands for these modifiers.
    Held(Mods),
}

/// One thing to do to bring Caps Lock to what the settings ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapsLockStep {
    /// Undo what an earlier run left behind.
    UndoLeftOver,
    /// Make Caps Lock, held, stand for these modifiers.
    Hold(Mods),
    /// Give Caps Lock back its usual behavior.
    Restore,
}

/// The steps from `state` to `wanted`: Caps Lock standing for those modifiers, or behaving
/// as usual with `None`. What an earlier run left behind is undone first, whatever is
/// wanted, so that checking whether Caps Lock is already remapped never mistakes Snappy's
/// own change for someone else's.
pub fn steps(state: CapsLockState, wanted: Option<Mods>) -> Vec<CapsLockStep> {
    match (state, wanted) {
        (CapsLockState::Normal, None) => Vec::new(),
        (CapsLockState::Normal, Some(modifiers)) => vec![CapsLockStep::Hold(modifiers)],
        (CapsLockState::LeftOver, None) => vec![CapsLockStep::UndoLeftOver],
        (CapsLockState::LeftOver, Some(modifiers)) => {
            vec![CapsLockStep::UndoLeftOver, CapsLockStep::Hold(modifiers)]
        }
        (CapsLockState::Held(_), None) => vec![CapsLockStep::Restore],
        (CapsLockState::Held(held), Some(modifiers)) if held == modifiers => Vec::new(),
        (CapsLockState::Held(_), Some(modifiers)) => vec![CapsLockStep::Hold(modifiers)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{Modifier, SnappyKey};

    fn control_option() -> Mods {
        Mods::from_iter([Modifier::Ctrl, Modifier::Alt])
    }

    #[test]
    fn a_quick_press_alone_is_a_tap() {
        let press = CapsLockPress::new(10.0);
        assert_eq!(press.release(10.12), CapsLockRelease::Tap);
    }

    #[test]
    fn a_press_held_up_to_the_limit_is_still_a_tap_and_past_it_is_not() {
        assert_eq!(
            CapsLockPress::new(10.0).release(10.0 + TAP_LIMIT),
            CapsLockRelease::Tap
        );
        assert_eq!(
            CapsLockPress::new(10.0).release(10.0 + TAP_LIMIT + 0.01),
            CapsLockRelease::Hold
        );
    }

    #[test]
    fn another_key_or_a_mouse_button_while_down_makes_it_a_hold_however_quick() {
        let mut press = CapsLockPress::new(10.0);
        press.note_other_input();
        assert!(press.other_input());
        assert_eq!(press.release(10.05), CapsLockRelease::Hold);
    }

    #[test]
    fn a_release_reported_before_its_press_counts_as_no_time_held() {
        let press = CapsLockPress::new(10.0);
        assert_eq!(press.held_for(9.9), 0.0);
        assert_eq!(press.release(9.9), CapsLockRelease::Tap);
    }

    #[test]
    fn nothing_happens_while_caps_lock_already_is_what_is_wanted() {
        let key = SnappyKey::DEFAULT.modifiers();
        assert!(steps(CapsLockState::Normal, None).is_empty());
        assert!(steps(CapsLockState::Held(key), Some(key)).is_empty());
    }

    #[test]
    fn turning_it_on_and_off_holds_and_restores() {
        let key = SnappyKey::DEFAULT.modifiers();
        assert_eq!(steps(CapsLockState::Normal, Some(key)), [CapsLockStep::Hold(key)]);
        assert_eq!(steps(CapsLockState::Held(key), None), [CapsLockStep::Restore]);
    }

    #[test]
    fn a_new_snappy_key_holds_again_with_its_modifiers() {
        let key = SnappyKey::DEFAULT.modifiers();
        assert_eq!(
            steps(CapsLockState::Held(key), Some(control_option())),
            [CapsLockStep::Hold(control_option())]
        );
    }

    #[test]
    fn what_a_crashed_run_left_is_undone_first_whatever_is_wanted() {
        let key = SnappyKey::DEFAULT.modifiers();
        assert_eq!(steps(CapsLockState::LeftOver, None), [CapsLockStep::UndoLeftOver]);
        assert_eq!(
            steps(CapsLockState::LeftOver, Some(key)),
            [CapsLockStep::UndoLeftOver, CapsLockStep::Hold(key)]
        );
    }
}
