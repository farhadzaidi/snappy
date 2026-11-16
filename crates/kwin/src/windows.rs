//! The facts the script reports about a window, for the rules every Wayland shell shares
//! (`snappy_linux::windows`).

use snappy_core::geometry::Rect;
pub use snappy_linux::windows::occupying;
use snappy_linux::windows::{ReportedWindow, WindowFacts};

use crate::protocol::WindowRecord;

impl ReportedWindow for WindowRecord {
    fn facts(&self) -> WindowFacts<'_> {
        WindowFacts {
            id: self.id,
            pid: self.pid,
            app_id: self.app_id.as_deref(),
            wm_class: self.wm_class.as_deref(),
            frame: Rect::from(self.frame),
            normal: self.normal,
            transient: self.transient,
            allows_move: self.allows_move,
            allows_resize: self.allows_resize,
            full_screen: self.full_screen,
            maximized: self.maximized,
            minimized: self.minimized,
            on_current_desktop: self.on_current_desktop,
            parked: self.parked,
        }
    }

    /// KWin's UUID for the window.
    fn lasting_id(&self) -> Option<String> {
        (!self.uuid.is_empty()).then(|| self.uuid.clone())
    }
}

#[cfg(test)]
mod tests {
    use snappy_platform::WindowId;

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

    #[test]
    fn the_app_is_the_desktop_file_then_the_window_class() {
        assert_eq!(standard().app(), "org.kde.konsole");
        let unpackaged = WindowRecord {
            app_id: Some(String::new()),
            ..standard()
        };
        assert_eq!(unpackaged.app(), "konsole");
    }

    #[test]
    fn windows_on_another_desktop_are_not_visible() {
        assert!(standard().is_visible());
        let elsewhere = WindowRecord {
            on_current_desktop: false,
            ..standard()
        };
        assert!(!elsewhere.is_visible());
    }

    #[test]
    fn windows_are_known_across_restarts_by_kwins_uuid() {
        assert_eq!(standard().info().id, WindowId(9));
        assert_eq!(
            standard().lasting_id().as_deref(),
            Some("{0a1b2c3d-0000-4000-8000-000000000009}")
        );
        let without = WindowRecord {
            uuid: String::new(),
            ..standard()
        };
        assert_eq!(without.lasting_id(), None);
    }

    #[test]
    fn windows_without_a_process_have_no_recovery_key() {
        assert_eq!(standard().info().recovery_key, None);
    }
}
