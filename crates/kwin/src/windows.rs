//! Which windows Snappy manages, decided on the facts the script reports, and how they appear
//! to the engine.

use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_linux::process;
use snappy_platform::{RecoveryKey, WindowId, WindowInfo, WindowState};

use crate::messages::{FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD, unavailable};
use crate::protocol::WindowRecord;

impl WindowRecord {
    pub fn window_id(&self) -> WindowId {
        WindowId(self.id)
    }

    /// Whether the window may be managed.
    pub fn check(&self) -> Result<(), WindowError> {
        if self.full_screen {
            return Err(unavailable(FULL_SCREEN));
        }
        if !self.normal || self.transient {
            return Err(unavailable(NOT_STANDARD));
        }
        // Minimized windows are never moved; maximized ones are restored before a write.
        if !(self.allows_move && self.allows_resize) && !self.maximized && !self.minimized {
            return Err(unavailable(NOT_MOVABLE));
        }
        Ok(())
    }

    /// On the desktop in use and not minimized or parked.
    pub fn is_visible(&self) -> bool {
        self.on_current_desktop && !self.minimized && !self.parked
    }

    /// The desktop file name, else the window class, else the executable name.
    pub fn app(&self) -> String {
        self.app_id
            .clone()
            .filter(|id| !id.is_empty())
            .or_else(|| self.wm_class.clone().filter(|class| !class.is_empty()))
            .or_else(|| (self.pid > 0).then(|| process::executable(self.pid)).flatten())
            .unwrap_or_else(|| format!("pid-{}", self.pid))
    }

    pub fn info(&self) -> WindowInfo {
        WindowInfo {
            id: self.window_id(),
            app: self.app(),
            pid: self.pid,
            frame: Rect::from(self.frame),
            recovery_key: self.recovery_key(),
        }
    }

    pub fn state(&self) -> WindowState {
        WindowState {
            full_screen: self.full_screen,
            has_sheet: false,
            minimized: self.minimized,
            on_parking_desktop: self.parked,
        }
    }

    /// The process launch plus KWin's UUID for the window identify it across a daemon
    /// restart while KWin keeps running.
    fn recovery_key(&self) -> Option<RecoveryKey> {
        (self.pid > 0 && !self.uuid.is_empty()).then_some(())?;
        Some(RecoveryKey {
            pid: self.pid,
            launch_time: process::start_time(self.pid)?,
            window: self.uuid.clone(),
        })
    }
}

/// Visible eligible windows filling any of `frames`, by the rule snapping uses.
pub fn occupying(records: &[WindowRecord], frames: &[Rect]) -> Vec<WindowInfo> {
    records
        .iter()
        .filter(|record| record.check().is_ok() && record.is_visible())
        .filter(|record| {
            frames
                .iter()
                .any(|zone| snappy_core::swap::fills_zone(Rect::from(record.frame), *zone))
        })
        .map(WindowRecord::info)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Frame;

    fn standard() -> WindowRecord {
        WindowRecord {
            id: 9,
            uuid: "{0a1b2c3d-0000-4000-8000-000000000009}".into(),
            pid: 0,
            app_id: Some("org.kde.konsole".into()),
            wm_class: Some("konsole".into()),
            frame: Frame {
                x: 14.0,
                y: 10.0,
                width: 936.0,
                height: 1048.0,
            },
            normal: true,
            allows_move: true,
            allows_resize: true,
            on_current_desktop: true,
            ..WindowRecord::default()
        }
    }

    fn message(result: Result<(), WindowError>) -> String {
        result.err().map(|error| error.to_string()).unwrap_or_default()
    }

    #[test]
    fn normal_windows_are_managed() {
        assert!(standard().check().is_ok());
    }

    #[test]
    fn full_screen_is_refused_first() {
        let window = WindowRecord {
            full_screen: true,
            normal: false,
            ..standard()
        };
        assert_eq!(message(window.check()), FULL_SCREEN);
    }

    #[test]
    fn dialogs_panels_and_other_types_are_refused() {
        let dialog = WindowRecord {
            transient: true,
            ..standard()
        };
        assert_eq!(message(dialog.check()), NOT_STANDARD);
        let panel = WindowRecord {
            normal: false,
            ..standard()
        };
        assert_eq!(message(panel.check()), NOT_STANDARD);
    }

    #[test]
    fn fixed_windows_are_refused_unless_maximized_or_minimized() {
        let fixed = WindowRecord {
            allows_resize: false,
            ..standard()
        };
        assert_eq!(message(fixed.check()), NOT_MOVABLE);
        assert!(
            WindowRecord {
                maximized: true,
                ..fixed.clone()
            }
            .check()
            .is_ok()
        );
    }

    #[test]
    fn the_app_is_the_desktop_file_then_the_window_class() {
        assert_eq!(standard().app(), "org.kde.konsole");
        let unpackaged = WindowRecord {
            app_id: Some(String::new()),
            ..standard()
        };
        assert_eq!(unpackaged.app(), "konsole");
        let nameless = WindowRecord {
            app_id: None,
            wm_class: None,
            ..standard()
        };
        assert_eq!(nameless.app(), "pid-0");
    }

    #[test]
    fn hidden_windows_are_not_visible() {
        assert!(standard().is_visible());
        for hidden in [
            WindowRecord {
                minimized: true,
                ..standard()
            },
            WindowRecord {
                parked: true,
                ..standard()
            },
            WindowRecord {
                on_current_desktop: false,
                ..standard()
            },
        ] {
            assert!(!hidden.is_visible());
        }
    }

    #[test]
    fn occupants_fill_the_zone_and_are_visible() {
        let zone = Rect::new(12.0, 12.0, 940.0, 1044.0);
        let parked = WindowRecord {
            id: 10,
            parked: true,
            ..standard()
        };
        let elsewhere = WindowRecord {
            id: 11,
            frame: Frame {
                x: 100.0,
                ..standard().frame
            },
            ..standard()
        };
        let found = occupying(&[standard(), parked, elsewhere], &[zone]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, WindowId(9));
    }

    #[test]
    fn windows_without_a_process_have_no_recovery_key() {
        assert_eq!(standard().info().id, WindowId(9));
        assert_eq!(standard().info().recovery_key, None);
        assert!(
            WindowRecord {
                parked: true,
                ..standard()
            }
            .state()
            .on_parking_desktop
        );
    }
}
