//! Whether Snappy may manage a window, decided on facts each adapter reads from its OS, and
//! the refusals every platform words the same.

use crate::error::WindowError;

pub const FULL_SCREEN: &str = "Leave full screen before tiling this window.";
pub const NOT_STANDARD: &str = "Choose a standard application window.";
pub const NOT_MOVABLE: &str = "This window doesn't allow moving and resizing.";

/// What an adapter knows about a window that decides whether Snappy manages it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EligibilityFacts {
    pub full_screen: bool,
    /// An ordinary app window, not a dialog, panel, tool window, or the like.
    pub standard: bool,
    /// A refusal only this platform has, given once the window is known to be standard.
    pub platform_refusal: Option<&'static str>,
    /// The OS lets Snappy move and resize it.
    pub movable: bool,
    /// Maximized or minimized where the window manager withdraws move and resize from such
    /// windows. They stay managed: writes restore maximized windows first, and minimized
    /// ones are never moved.
    pub move_withdrawn: bool,
}

/// Refuses a window for the first reason that applies: full screen, then not standard, then
/// the platform's own refusal, then not movable.
pub fn check(facts: &EligibilityFacts) -> Result<(), WindowError> {
    if facts.full_screen {
        return Err(WindowError::unavailable(FULL_SCREEN));
    }
    if !facts.standard {
        return Err(WindowError::unavailable(NOT_STANDARD));
    }
    if let Some(refusal) = facts.platform_refusal {
        return Err(WindowError::unavailable(refusal));
    }
    if !facts.movable && !facts.move_withdrawn {
        return Err(WindowError::unavailable(NOT_MOVABLE));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELEVATED: &str = "Snappy can't move windows of apps running as administrator.";

    fn manageable() -> EligibilityFacts {
        EligibilityFacts {
            standard: true,
            movable: true,
            ..EligibilityFacts::default()
        }
    }

    fn message(facts: EligibilityFacts) -> String {
        check(&facts)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    }

    #[test]
    fn standard_movable_windows_are_managed() {
        assert_eq!(check(&manageable()), Ok(()));
    }

    #[test]
    fn full_screen_is_refused_before_anything_else() {
        let facts = EligibilityFacts {
            full_screen: true,
            standard: false,
            platform_refusal: Some(ELEVATED),
            movable: false,
            ..manageable()
        };
        assert_eq!(message(facts), "Leave full screen before tiling this window.");
    }

    #[test]
    fn non_standard_windows_are_refused_before_the_platforms_own_refusal() {
        let facts = EligibilityFacts {
            standard: false,
            platform_refusal: Some(ELEVATED),
            movable: false,
            ..manageable()
        };
        assert_eq!(message(facts), "Choose a standard application window.");
    }

    #[test]
    fn the_platforms_own_refusal_comes_before_movability() {
        let facts = EligibilityFacts {
            platform_refusal: Some(ELEVATED),
            movable: false,
            ..manageable()
        };
        assert_eq!(message(facts), ELEVATED);
    }

    #[test]
    fn windows_that_cant_be_moved_are_refused_unless_the_move_was_withdrawn() {
        let fixed = EligibilityFacts {
            movable: false,
            ..manageable()
        };
        assert_eq!(message(fixed), "This window doesn't allow moving and resizing.");
        let withdrawn = EligibilityFacts {
            move_withdrawn: true,
            ..fixed
        };
        assert_eq!(check(&withdrawn), Ok(()));
    }
}
