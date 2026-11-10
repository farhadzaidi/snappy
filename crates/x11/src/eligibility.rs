//! Which X clients Snappy manages, decided on plain facts read from
//! EWMH and ICCCM properties.

use snappy_core::error::WindowError;

use crate::messages::{FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD, unavailable};

/// A `_NET_WM_WINDOW_TYPE` entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    Normal,
    /// Any other type EWMH defines: dialog, dock, utility, splash, menu, and so on.
    Special,
    /// A type this window manager specification doesn't define, such as a vendor type.
    Unknown,
}

/// The first type EWMH defines wins; vendor types listed before it are skipped.
pub fn is_normal(types: &[WindowType]) -> bool {
    types
        .iter()
        .find(|kind| **kind != WindowType::Unknown)
        .is_none_or(|kind| *kind == WindowType::Normal)
}

/// What the properties say about a client.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowFacts {
    pub in_client_list: bool,
    pub types: Vec<WindowType>,
    pub full_screen: bool,
    pub maximized: bool,
    pub minimized: bool,
    /// `WM_TRANSIENT_FOR` names another window.
    pub transient: bool,
    /// `_NET_WM_ALLOWED_ACTIONS` lacks move or resize. Absent actions allow both.
    pub fixed: bool,
}

impl WindowFacts {
    /// Whether the window may be managed.
    pub fn check(&self) -> Result<(), WindowError> {
        if self.full_screen {
            return Err(unavailable(FULL_SCREEN));
        }
        if !self.in_client_list || self.transient || !is_normal(&self.types) {
            return Err(unavailable(NOT_STANDARD));
        }
        // Window managers withdraw move and resize from maximized and minimized windows.
        // Writes restore maximized windows first; minimized ones are never moved.
        if self.fixed && !self.maximized && !self.minimized {
            return Err(unavailable(NOT_MOVABLE));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn standard() -> WindowFacts {
        WindowFacts {
            in_client_list: true,
            ..WindowFacts::default()
        }
    }

    fn message(result: Result<(), WindowError>) -> String {
        result.err().map(|error| error.to_string()).unwrap_or_default()
    }

    #[test]
    fn normal_and_untyped_clients_are_managed() {
        assert!(standard().check().is_ok());
        let typed = WindowFacts {
            types: vec![WindowType::Normal],
            ..standard()
        };
        assert!(typed.check().is_ok());
    }

    #[test]
    fn vendor_types_before_normal_are_skipped() {
        let kde = WindowFacts {
            types: vec![WindowType::Unknown, WindowType::Normal],
            ..standard()
        };
        assert!(kde.check().is_ok());
    }

    #[test]
    fn special_types_and_dialogs_are_refused() {
        for types in [
            vec![WindowType::Special],
            vec![WindowType::Special, WindowType::Normal],
            vec![WindowType::Unknown, WindowType::Special],
        ] {
            let window = WindowFacts { types, ..standard() };
            assert_eq!(message(window.check()), "Choose a standard application window.");
        }
        let transient = WindowFacts {
            transient: true,
            ..standard()
        };
        assert_eq!(
            message(transient.check()),
            "Choose a standard application window."
        );
    }

    #[test]
    fn unlisted_windows_are_refused() {
        let unlisted = WindowFacts {
            in_client_list: false,
            ..standard()
        };
        assert_eq!(message(unlisted.check()), "Choose a standard application window.");
    }

    #[test]
    fn full_screen_is_refused_first() {
        let window = WindowFacts {
            full_screen: true,
            transient: true,
            ..standard()
        };
        assert_eq!(
            message(window.check()),
            "Leave full screen before tiling this window."
        );
    }

    #[test]
    fn minimized_windows_stay_managed_without_move_actions() {
        let minimized = WindowFacts {
            fixed: true,
            minimized: true,
            ..standard()
        };
        assert!(minimized.check().is_ok());
    }

    #[test]
    fn fixed_windows_are_refused_unless_maximized() {
        let fixed = WindowFacts {
            fixed: true,
            ..standard()
        };
        assert_eq!(
            message(fixed.check()),
            "This window does not allow moving and resizing."
        );
        let maximized = WindowFacts {
            maximized: true,
            ..fixed
        };
        assert!(maximized.check().is_ok());
    }
}
