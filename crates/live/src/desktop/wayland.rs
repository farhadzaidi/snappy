//! The Wayland desktop shell that places and reads the fixture's windows, lists what's on
//! screen, and injects input, since Wayland clients can do none of that themselves: GNOME
//! Shell's test helper, or KWin's probe scripts and fake input device.

use std::path::Path;
use std::time::Duration;

use snappy_core::geometry::{Point, Rect};
use snappy_platform::Display;

use super::{gnome, kwin};
use crate::input::Input;
use crate::screen::ScreenWindow;

/// How far a drag's first movement goes, in pixels: past the shell's drag threshold (GTK's
/// is 8 pixels), and short enough to stay on the title bar it pressed.
pub const FIRST_STEP: f64 = 12.0;
/// How long the pointer rests before a press, as a hand does, so the window under it knows
/// it's there.
pub const HOVER_MS: u64 = 100;
/// How long a click holds the button.
pub const CLICK_MS: u64 = 50;
/// How often the shell is asked again while waiting for it.
pub const POLL_MS: u64 = 10;
/// How long a drag waits for the shell to start moving the window.
pub const MOVE_START_TIMEOUT: Duration = Duration::from_secs(1);
/// How long focusing a window may take.
pub const FOCUS_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Gnome,
    KWin,
}

impl Shell {
    /// The shell of the session the suite runs in; `None` on X11.
    pub fn current() -> Option<Shell> {
        if gnome::is_session() {
            Some(Shell::Gnome)
        } else if kwin::is_session() {
            Some(Shell::KWin)
        } else {
            None
        }
    }

    /// The displays, as the daemon's adapter reads them.
    pub fn displays(self) -> Result<Vec<Display>, String> {
        match self {
            Shell::Gnome => snappy_gnome::reference::displays(),
            Shell::KWin => kwin::displays(),
        }
    }

    /// The frame of the window with this title.
    pub fn window_frame(self, title: &str) -> Result<Option<Rect>, String> {
        match self {
            Shell::Gnome => gnome::window_frame(title),
            Shell::KWin => kwin::window_frame(title),
        }
    }

    /// Moves the window with this title so its frame starts at `origin`.
    pub fn place_window(self, title: &str, origin: Point) -> Result<Rect, String> {
        match self {
            Shell::Gnome => gnome::place_window(title, origin),
            Shell::KWin => kwin::place_window(title, origin),
        }
    }

    pub fn focus_window(self, title: &str) -> Result<(), String> {
        match self {
            Shell::Gnome => gnome::focus_window(title),
            Shell::KWin => kwin::focus_window(title),
        }
    }

    /// Normal windows and Snappy's overlays on screen, front to back.
    pub fn on_screen_windows(self, daemon: Option<i32>) -> Result<Vec<ScreenWindow>, String> {
        match self {
            Shell::Gnome => gnome::on_screen_windows(daemon),
            Shell::KWin => kwin::on_screen_windows(daemon),
        }
    }

    pub fn screenshot(self, path: &Path) {
        match self {
            Shell::Gnome => gnome::screenshot(path),
            Shell::KWin => kwin::screenshot(path),
        }
    }

    /// Lets go of everything a failed scenario may have left held.
    pub fn release_all(self) {
        match self {
            Shell::Gnome => gnome::release_all(),
            Shell::KWin => kwin::release_all(),
        }
    }

    pub fn input(self) -> Result<Box<dyn Input>, String> {
        Ok(match self {
            Shell::Gnome => Box::new(gnome::GnomeInput::new()?),
            Shell::KWin => Box::new(kwin::KWinInput::new()?),
        })
    }
}
