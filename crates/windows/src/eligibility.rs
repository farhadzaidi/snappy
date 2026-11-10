//! Which windows Snappy manages, decided on plain facts read from the
//! window's styles, DWM, and its process.

use snappy_core::error::WindowError;

use crate::messages::{ELEVATED, FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD, unavailable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowFacts {
    pub visible: bool,
    /// A top-level window with no owner.
    pub unowned_top_level: bool,
    /// DWM hides it: another virtual desktop, a suspended app, or a shell surface.
    pub cloaked: bool,
    pub caption: bool,
    /// Resizable: `WS_THICKFRAME`.
    pub sizing_border: bool,
    pub tool_window: bool,
    /// Its window rectangle covers its whole monitor.
    pub covers_monitor: bool,
    /// Its process runs at a higher integrity level than Snappy, or one Snappy can't read.
    pub elevated: bool,
    pub minimized: bool,
    pub maximized: bool,
    /// Disabled while one of its own popups is modal, as a sheet would be.
    pub has_modal_popup: bool,
}

impl WindowFacts {
    /// Full screen means covering the monitor with no caption, as games and video players
    /// and the F11 mode of browsers do.
    pub fn full_screen(&self) -> bool {
        self.covers_monitor && !self.caption
    }

    /// Whether the window may be managed.
    pub fn check(&self) -> Result<(), WindowError> {
        if self.full_screen() {
            return Err(unavailable(FULL_SCREEN));
        }
        if !self.visible || !self.unowned_top_level || self.cloaked || self.tool_window || !self.caption {
            return Err(unavailable(NOT_STANDARD));
        }
        // User Interface Privilege Isolation blocks moving windows of elevated apps.
        if self.elevated {
            return Err(unavailable(ELEVATED));
        }
        if !self.sizing_border {
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
            visible: true,
            unowned_top_level: true,
            caption: true,
            sizing_border: true,
            ..WindowFacts::default()
        }
    }

    fn message(facts: WindowFacts) -> String {
        facts
            .check()
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    }

    #[test]
    fn captioned_resizable_windows_are_managed() {
        assert!(standard().check().is_ok());
        let minimized = WindowFacts {
            minimized: true,
            ..standard()
        };
        assert!(minimized.check().is_ok());
        let maximized = WindowFacts {
            maximized: true,
            covers_monitor: true,
            ..standard()
        };
        assert!(maximized.check().is_ok());
    }

    #[test]
    fn full_screen_windows_are_refused_first() {
        let full_screen = WindowFacts {
            covers_monitor: true,
            caption: false,
            elevated: true,
            ..standard()
        };
        assert_eq!(
            message(full_screen),
            "Leave full screen before tiling this window."
        );
    }

    #[test]
    fn owned_tool_cloaked_and_captionless_windows_are_not_standard() {
        for facts in [
            WindowFacts {
                unowned_top_level: false,
                ..standard()
            },
            WindowFacts {
                tool_window: true,
                ..standard()
            },
            WindowFacts {
                cloaked: true,
                ..standard()
            },
            WindowFacts {
                caption: false,
                ..standard()
            },
            WindowFacts {
                visible: false,
                ..standard()
            },
        ] {
            assert_eq!(message(facts), "Choose a standard application window.");
        }
    }

    #[test]
    fn elevated_windows_are_refused() {
        let elevated = WindowFacts {
            elevated: true,
            ..standard()
        };
        assert_eq!(
            message(elevated),
            "Snappy can't move windows of apps running as administrator."
        );
    }

    #[test]
    fn fixed_size_windows_are_refused() {
        let fixed = WindowFacts {
            sizing_border: false,
            ..standard()
        };
        assert_eq!(message(fixed), "This window does not allow moving and resizing.");
    }
}
