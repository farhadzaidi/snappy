//! Which apps and windows Snappy manages, decided on plain facts read from AppKit and
//! Accessibility.

use snappy_core::error::WindowError;

use crate::messages::{FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD, unavailable};

/// The settings app runs with the accessory policy, yet its windows are managed.
pub const SETTINGS_APP_BUNDLE_ID: &str = "local.snappy.settings";

const STANDARD_WINDOW_SUBROLE: &str = "AXStandardWindow";

/// What the adapter knows about a running app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppFacts {
    pub pid: i32,
    /// Bundle identifier, else executable name.
    pub identifier: String,
    pub bundle_id: Option<String>,
    pub regular: bool,
    pub hidden: bool,
    /// Snappy's own daemon.
    pub is_self: bool,
}

impl AppFacts {
    /// Apps whose windows Snappy can manage.
    pub fn is_supported(&self) -> bool {
        !self.is_self && (self.regular || self.bundle_id.as_deref() == Some(SETTINGS_APP_BUNDLE_ID))
    }

    /// Whether the app's windows may be managed.
    pub fn check(&self) -> Result<(), WindowError> {
        if !self.is_supported() {
            return Err(unavailable(NOT_STANDARD));
        }
        Ok(())
    }
}

/// The identifier an app is known by: bundle identifier, else executable name.
pub fn app_identifier(bundle_id: Option<&str>, executable: Option<&str>, pid: i32) -> String {
    bundle_id
        .or(executable)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("pid-{pid}"))
}

/// What Accessibility reports about a window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowFacts {
    pub subrole: Option<String>,
    pub full_screen: bool,
    pub position_settable: bool,
    pub size_settable: bool,
}

impl WindowFacts {
    /// Whether the window may be managed.
    pub fn check(&self) -> Result<(), WindowError> {
        if self.full_screen {
            return Err(unavailable(FULL_SCREEN));
        }
        if self.subrole.as_deref() != Some(STANDARD_WINDOW_SUBROLE) {
            return Err(unavailable(NOT_STANDARD));
        }
        if !(self.position_settable && self.size_settable) {
            return Err(unavailable(NOT_MOVABLE));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(bundle: &str, regular: bool) -> AppFacts {
        AppFacts {
            pid: 42,
            identifier: bundle.to_owned(),
            bundle_id: Some(bundle.to_owned()),
            regular,
            hidden: false,
            is_self: false,
        }
    }

    fn standard_window() -> WindowFacts {
        WindowFacts {
            subrole: Some("AXStandardWindow".into()),
            full_screen: false,
            position_settable: true,
            size_settable: true,
        }
    }

    fn message(result: Result<(), WindowError>) -> String {
        result.err().map(|error| error.to_string()).unwrap_or_default()
    }

    #[test]
    fn regular_apps_with_standard_settable_windows_are_managed() {
        assert!(app("com.apple.TextEdit", true).check().is_ok());
        assert!(standard_window().check().is_ok());
    }

    #[test]
    fn accessory_apps_and_snappy_itself_are_refused() {
        assert_eq!(
            message(app("com.example.MenuBarTool", false).check()),
            "Choose a standard application window."
        );
        let daemon = AppFacts {
            is_self: true,
            ..app("local.snappy", true)
        };
        assert_eq!(message(daemon.check()), "Choose a standard application window.");
    }

    #[test]
    fn settings_app_is_managed_despite_its_accessory_policy() {
        assert!(app(SETTINGS_APP_BUNDLE_ID, false).check().is_ok());
    }

    #[test]
    fn full_screen_windows_are_refused_first() {
        let window = WindowFacts {
            full_screen: true,
            subrole: None,
            ..standard_window()
        };
        assert_eq!(
            message(window.check()),
            "Leave full screen before tiling this window."
        );
    }

    #[test]
    fn non_standard_windows_are_refused() {
        for subrole in [
            Some("AXDialog"),
            Some("AXFloatingWindow"),
            Some("AXSystemDialog"),
            None,
        ] {
            let window = WindowFacts {
                subrole: subrole.map(str::to_owned),
                ..standard_window()
            };
            assert_eq!(message(window.check()), "Choose a standard application window.");
        }
    }

    #[test]
    fn windows_without_settable_position_or_size_are_refused() {
        for (position_settable, size_settable) in [(false, true), (true, false), (false, false)] {
            let window = WindowFacts {
                position_settable,
                size_settable,
                ..standard_window()
            };
            assert_eq!(
                message(window.check()),
                "This window does not allow moving and resizing."
            );
        }
    }

    #[test]
    fn identifiers_prefer_the_bundle_identifier() {
        assert_eq!(app_identifier(Some("com.a"), Some("a"), 1), "com.a");
        assert_eq!(app_identifier(None, Some("a"), 1), "a");
        assert_eq!(app_identifier(None, None, 7), "pid-7");
    }
}
