//! A fake app window and the app behaviors tests script on it.

use snappy_core::geometry::{Rect, Size};
use snappy_platform::{WindowId, WindowState};

/// A window with scriptable app behavior.
#[derive(Debug, Clone, PartialEq)]
pub struct FakeWindow {
    pub app: String,
    pub pid: u32,
    pub frame: Rect,
    pub state: WindowState,
    pub min_size: Size,
    /// Size writes apply only while the window is fully on one display.
    pub strict_resize: bool,
    /// Every write is ignored.
    pub frozen: bool,
    /// Its app doesn't answer, so every write fails once Snappy stops waiting.
    pub hung: bool,
    pub hidden: bool,
    /// A native tab hidden behind this shown tab, as on macOS: off screen, left out of
    /// the visible windows, with a frame that goes stale while the group moves.
    pub tab_of: Option<WindowId>,
    /// When its process started; another value with the same process ID is a later process
    /// that reused the ID.
    pub launch_time: f64,
}

impl FakeWindow {
    pub fn new(app: &str, pid: u32, frame: Rect) -> Self {
        Self {
            app: app.to_owned(),
            pid,
            launch_time: 1.0,
            frame,
            state: WindowState::default(),
            min_size: Size::new(1.0, 1.0),
            strict_resize: false,
            frozen: false,
            hung: false,
            hidden: false,
            tab_of: None,
        }
    }

    /// Not hidden by its app, as a hidden tab, or on the parking desktop.
    pub fn is_shown(&self) -> bool {
        !self.hidden && self.tab_of.is_none() && !self.state.on_parking_desktop
    }
}
