//! Which windows Snappy manages, decided on the facts the extension reports, and how they
//! appear to the engine.

use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_linux::process;
use snappy_platform::{RecoveryKey, WindowId, WindowInfo, WindowState};

use crate::messages::{FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD, unavailable};
use crate::protocol::WindowRecord;

/// Placeholder apps GNOME makes for windows without a desktop file are named like this.
const WINDOW_BACKED_PREFIX: &str = "window:";

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
        // GNOME withdraws resizing from maximized windows; writes restore them first, and
        // minimized windows are never moved.
        if !(self.allows_move && self.allows_resize) && !self.maximized && !self.minimized {
            return Err(unavailable(NOT_MOVABLE));
        }
        Ok(())
    }

    /// On the workspace in use and not minimized or parked.
    pub fn is_visible(&self) -> bool {
        self.on_active_workspace && !self.minimized && !self.parked
    }

    /// The desktop file ID, else the window class, else the executable name.
    pub fn app(&self) -> String {
        self.app_id
            .clone()
            .filter(|id| !id.is_empty() && !id.starts_with(WINDOW_BACKED_PREFIX))
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

    /// The process launch plus Mutter's stable sequence identify the window across a
    /// daemon restart while GNOME Shell keeps running.
    fn recovery_key(&self) -> Option<RecoveryKey> {
        (self.pid > 0).then_some(())?;
        Some(RecoveryKey {
            pid: self.pid,
            launch_time: process::start_time(self.pid)?,
            window: self.id.to_string(),
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
            pid: 0,
            app_id: Some("org.gnome.TextEditor.desktop".into()),
            wm_class: Some("gnome-text-editor".into()),
            frame: Frame {
                x: 14.0,
                y: 10.0,
                width: 936.0,
                height: 1048.0,
            },
            normal: true,
            allows_move: true,
            allows_resize: true,
            on_active_workspace: true,
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
    fn dialogs_and_other_types_are_refused() {
        let dialog = WindowRecord {
            transient: true,
            ..standard()
        };
        assert_eq!(message(dialog.check()), NOT_STANDARD);
        let utility = WindowRecord {
            normal: false,
            ..standard()
        };
        assert_eq!(message(utility.check()), NOT_STANDARD);
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
        assert!(
            WindowRecord {
                minimized: true,
                ..fixed
            }
            .check()
            .is_ok()
        );
    }

    #[test]
    fn the_app_is_the_desktop_file_then_the_window_class() {
        assert_eq!(standard().app(), "org.gnome.TextEditor.desktop");
        let unpackaged = WindowRecord {
            app_id: Some("window:3".into()),
            ..standard()
        };
        assert_eq!(unpackaged.app(), "gnome-text-editor");
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
                on_active_workspace: false,
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
    fn window_ids_are_mutters_stable_sequences() {
        assert_eq!(standard().info().id, WindowId(9));
        assert_eq!(standard().info().recovery_key, None, "no process to identify");
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
