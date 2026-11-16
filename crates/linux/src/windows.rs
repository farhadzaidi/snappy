//! Which windows Snappy manages, decided on the facts a desktop shell reports through
//! Snappy's extension or script in it, and how they appear to the engine.

use snappy_core::eligibility::{self, EligibilityFacts};
use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_platform::{RecoveryKey, WindowId, WindowInfo, WindowState};

use crate::process;

/// What the shell reports about a window that decides how Snappy treats it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WindowFacts<'a> {
    pub id: u64,
    pub pid: u32,
    /// The app's desktop file, when the shell names a real one.
    pub app_id: Option<&'a str>,
    pub wm_class: Option<&'a str>,
    pub frame: Rect,
    pub normal: bool,
    pub transient: bool,
    pub allows_move: bool,
    pub allows_resize: bool,
    pub full_screen: bool,
    pub maximized: bool,
    pub minimized: bool,
    /// On the workspace or desktop in use.
    pub on_current_desktop: bool,
    pub parked: bool,
}

/// A window as the shell reports it.
pub trait ReportedWindow {
    fn facts(&self) -> WindowFacts<'_>;

    /// The shell's own name for the window, which lasts across a daemon restart while the
    /// shell keeps running.
    fn lasting_id(&self) -> Option<String>;

    /// Whether the window may be managed.
    fn check(&self) -> Result<(), WindowError> {
        let facts = self.facts();
        eligibility::check(&EligibilityFacts {
            full_screen: facts.full_screen,
            standard: facts.normal && !facts.transient,
            movable: facts.allows_move && facts.allows_resize,
            // Shells withdraw resizing from maximized windows, and minimized ones are never moved.
            move_withdrawn: facts.maximized || facts.minimized,
            ..EligibilityFacts::default()
        })
    }

    /// On the workspace in use and not minimized or parked.
    fn is_visible(&self) -> bool {
        let facts = self.facts();
        facts.on_current_desktop && !facts.minimized && !facts.parked
    }

    /// The desktop file, else the window class, else the executable name.
    fn app(&self) -> String {
        let facts = self.facts();
        facts
            .app_id
            .filter(|id| !id.is_empty())
            .or(facts.wm_class.filter(|class| !class.is_empty()))
            .map(str::to_owned)
            .or_else(|| (facts.pid > 0).then(|| process::executable(facts.pid)).flatten())
            .unwrap_or_else(|| format!("pid-{}", facts.pid))
    }

    fn info(&self) -> WindowInfo {
        let facts = self.facts();
        WindowInfo {
            id: WindowId(facts.id),
            app: self.app(),
            pid: facts.pid,
            frame: facts.frame,
            recovery_key: self.recovery_key(),
        }
    }

    fn state(&self) -> WindowState {
        let facts = self.facts();
        WindowState {
            full_screen: facts.full_screen,
            has_sheet: false,
            minimized: facts.minimized,
            on_parking_desktop: facts.parked,
        }
    }

    /// The process launch plus the shell's lasting name identify the window across a daemon
    /// restart.
    fn recovery_key(&self) -> Option<RecoveryKey> {
        let pid = self.facts().pid;
        (pid > 0).then_some(())?;
        let window = self.lasting_id()?;
        Some(RecoveryKey {
            pid,
            launch_time: process::start_time(pid)?,
            window,
        })
    }
}

/// Visible eligible windows filling any of `frames`, by the rule snapping uses.
pub fn occupying<W: ReportedWindow>(windows: &[W], frames: &[Rect]) -> Vec<WindowInfo> {
    windows
        .iter()
        .filter(|window| window.check().is_ok() && window.is_visible())
        .filter(|window| {
            frames
                .iter()
                .any(|zone| snappy_core::placement::swap::fills_zone(window.facts().frame, *zone))
        })
        .map(ReportedWindow::info)
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::messages::{FULL_SCREEN, NOT_MOVABLE, NOT_STANDARD};

    use super::*;

    #[derive(Clone)]
    struct Reported {
        facts: WindowFacts<'static>,
        lasting_id: Option<String>,
    }

    impl ReportedWindow for Reported {
        fn facts(&self) -> WindowFacts<'_> {
            self.facts
        }

        fn lasting_id(&self) -> Option<String> {
            self.lasting_id.clone()
        }
    }

    fn standard() -> Reported {
        Reported {
            facts: WindowFacts {
                id: 9,
                pid: 0,
                app_id: Some("org.gnome.TextEditor.desktop"),
                wm_class: Some("gnome-text-editor"),
                frame: Rect::new(14.0, 10.0, 936.0, 1048.0),
                normal: true,
                allows_move: true,
                allows_resize: true,
                on_current_desktop: true,
                ..WindowFacts::default()
            },
            lasting_id: Some("9".into()),
        }
    }

    /// The standard window with `change` made to its facts.
    fn standard_with(change: impl FnOnce(&mut WindowFacts<'static>)) -> Reported {
        let mut window = standard();
        change(&mut window.facts);
        window
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
        let window = standard_with(|facts| {
            facts.full_screen = true;
            facts.normal = false;
        });
        assert_eq!(message(window.check()), FULL_SCREEN);
    }

    #[test]
    fn dialogs_panels_and_other_types_are_refused() {
        let dialog = standard_with(|facts| facts.transient = true);
        assert_eq!(message(dialog.check()), NOT_STANDARD);
        let panel = standard_with(|facts| facts.normal = false);
        assert_eq!(message(panel.check()), NOT_STANDARD);
    }

    #[test]
    fn fixed_windows_are_refused_unless_maximized_or_minimized() {
        let fixed = standard_with(|facts| facts.allows_resize = false);
        assert_eq!(message(fixed.check()), NOT_MOVABLE);
        let mut maximized = fixed.clone();
        maximized.facts.maximized = true;
        assert!(maximized.check().is_ok());
        let mut minimized = fixed;
        minimized.facts.minimized = true;
        assert!(minimized.check().is_ok());
    }

    #[test]
    fn the_app_is_the_desktop_file_then_the_window_class() {
        assert_eq!(standard().app(), "org.gnome.TextEditor.desktop");
        let unpackaged = standard_with(|facts| facts.app_id = Some(""));
        assert_eq!(unpackaged.app(), "gnome-text-editor");
        let nameless = standard_with(|facts| {
            facts.app_id = None;
            facts.wm_class = None;
        });
        assert_eq!(nameless.app(), "pid-0");
    }

    #[test]
    fn hidden_windows_are_not_visible() {
        assert!(standard().is_visible());
        for hidden in [
            standard_with(|facts| facts.minimized = true),
            standard_with(|facts| facts.parked = true),
            standard_with(|facts| facts.on_current_desktop = false),
        ] {
            assert!(!hidden.is_visible());
        }
    }

    #[test]
    fn occupants_fill_the_zone_and_are_visible() {
        let zone = Rect::new(12.0, 12.0, 940.0, 1044.0);
        let parked = standard_with(|facts| {
            facts.id = 10;
            facts.parked = true;
        });
        let elsewhere = standard_with(|facts| {
            facts.id = 11;
            facts.frame.x = 100.0;
        });
        let found = occupying(&[standard(), parked, elsewhere], &[zone]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, WindowId(9));
    }

    #[test]
    fn windows_without_a_process_have_no_recovery_key() {
        assert_eq!(standard().info().id, WindowId(9));
        assert_eq!(standard().info().recovery_key, None);
    }

    #[test]
    fn parked_windows_are_on_the_parking_desktop() {
        assert!(
            standard_with(|facts| facts.parked = true)
                .state()
                .on_parking_desktop
        );
    }
}
