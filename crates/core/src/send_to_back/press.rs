//! Telling a quick press of the shortcut from a hold, a peek, which puts the window the
//! press sent to the back where it was once let go.

/// A shortcut down this long, in seconds, was held rather than pressed and let go: well
/// past a quick press and release, which takes a hand under a tenth of a second.
pub const HOLD_AFTER: f64 = 0.2;

/// One press of the shortcut, from going down to being let go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShortcutPress {
    pressed_at: f64,
}

/// What a press turned out to be once let go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressKind {
    /// Let go within `HOLD_AFTER`.
    Quick,
    /// Down for `HOLD_AFTER` or longer.
    Held,
}

impl ShortcutPress {
    /// The shortcut went down at `at`, in seconds.
    pub fn new(at: f64) -> Self {
        Self { pressed_at: at }
    }

    /// The shortcut was let go at `at`.
    pub fn release(self, at: f64) -> PressKind {
        if at >= self.pressed_at + HOLD_AFTER {
            PressKind::Held
        } else {
            PressKind::Quick
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_let_go_at_once_is_quick() {
        assert_eq!(ShortcutPress::new(10.0).release(10.07), PressKind::Quick);
    }

    #[test]
    fn a_press_let_go_just_before_the_hold_is_due_is_still_quick() {
        let press = ShortcutPress::new(10.0);
        assert_eq!(press.release(10.0 + HOLD_AFTER - 0.01), PressKind::Quick);
    }

    #[test]
    fn a_press_let_go_once_the_hold_is_due_or_later_was_held() {
        let press = ShortcutPress::new(10.0);
        assert_eq!(press.release(10.0 + HOLD_AFTER), PressKind::Held);
        assert_eq!(press.release(12.0), PressKind::Held);
    }

    #[test]
    fn a_release_reported_before_its_press_counts_as_quick() {
        assert_eq!(ShortcutPress::new(10.0).release(9.9), PressKind::Quick);
    }
}
