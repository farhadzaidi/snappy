//! The facts the extension reports about a window, for the rules every Wayland shell
//! shares (`snappy_linux::windows`).

use snappy_core::geometry::Rect;
pub use snappy_linux::windows::occupying;
use snappy_linux::windows::{ReportedWindow, WindowFacts};

use crate::protocol::WindowRecord;

/// Placeholder apps GNOME makes for windows without a desktop file are named like this.
const WINDOW_BACKED_PREFIX: &str = "window:";

impl ReportedWindow for WindowRecord {
    fn facts(&self) -> WindowFacts<'_> {
        WindowFacts {
            id: self.id,
            pid: self.pid,
            app_id: self
                .app_id
                .as_deref()
                .filter(|id| !id.starts_with(WINDOW_BACKED_PREFIX)),
            wm_class: self.wm_class.as_deref(),
            frame: Rect::from(self.frame),
            normal: self.normal,
            transient: self.transient,
            allows_move: self.allows_move,
            allows_resize: self.allows_resize,
            full_screen: self.full_screen,
            maximized: self.maximized,
            minimized: self.minimized,
            on_current_desktop: self.on_active_workspace,
            parked: self.parked,
        }
    }

    /// Mutter's stable sequence.
    fn lasting_id(&self) -> Option<String> {
        Some(self.id.to_string())
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

    #[test]
    fn the_app_is_the_desktop_file_then_the_window_class() {
        assert_eq!(standard().app(), "org.gnome.TextEditor.desktop");
        let unpackaged = WindowRecord {
            app_id: Some("window:3".into()),
            ..standard()
        };
        assert_eq!(unpackaged.app(), "gnome-text-editor");
    }

    #[test]
    fn windows_on_another_workspace_are_not_visible() {
        assert!(standard().is_visible());
        let elsewhere = WindowRecord {
            on_active_workspace: false,
            ..standard()
        };
        assert!(!elsewhere.is_visible());
    }

    #[test]
    fn window_ids_are_mutters_stable_sequences() {
        assert_eq!(standard().info().id, WindowId(9));
        assert_eq!(standard().lasting_id().as_deref(), Some("9"));
        assert_eq!(standard().info().recovery_key, None, "no process to identify");
    }
}
