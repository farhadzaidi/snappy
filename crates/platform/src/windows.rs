//! Windows and displays as adapters see them, and the trait that reads and moves windows.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::window::WindowId;

/// A connected display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    /// Stable across reconnects and restarts.
    pub id: String,
    pub name: String,
    pub frame: Rect,
    /// The frame minus the menu bar, Dock, taskbar, or panels.
    pub work_area: Rect,
    pub scale: f64,
}

/// Identifies a window across a daemon restart while its app keeps running: the process ID
/// and the process's start time, which together never name another process, and a token
/// for the window that stays the same while the window exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKey {
    pub pid: u32,
    pub launch_time: f64,
    pub window: String,
}

impl RecoveryKey {
    /// Start times this close, in seconds, are one start, whatever a save and a read round.
    const SAME_LAUNCH: f64 = 0.001;

    /// The same process; a reused process ID with another start time is not.
    pub fn same_process(&self, other: &RecoveryKey) -> bool {
        self.pid == other.pid && (self.launch_time - other.launch_time).abs() < Self::SAME_LAUNCH
    }

    /// The same window of the same process.
    pub fn same_window(&self, other: &RecoveryKey) -> bool {
        self.same_process(other) && self.window == other.window
    }
}

/// A managed window as the adapter sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub id: WindowId,
    /// Bundle identifier, executable name, or WM_CLASS.
    pub app: String,
    pub pid: u32,
    pub frame: Rect,
    pub recovery_key: Option<RecoveryKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowState {
    pub full_screen: bool,
    pub has_sheet: bool,
    pub minimized: bool,
    /// On the native desktop Snappy parks workspace windows on. Always false where
    /// parking moves windows instead.
    pub on_parking_desktop: bool,
}

impl WindowState {
    /// Workspaces can move the window.
    pub fn movable(self) -> bool {
        !self.full_screen && !self.has_sheet
    }
}

/// One window's native tabs, where the platform makes each tab a window of its own and
/// shows only the selected one, as macOS does. Moving the shown tab moves the group.
#[derive(Debug, Clone, PartialEq)]
pub struct TabGroup {
    /// The selected tab, the only one on screen.
    pub shown: WindowInfo,
    /// Other tabs of the group that the platform has seen. Their frames are stale until
    /// they show, and moving one changes nothing on screen.
    pub hidden: Vec<WindowId>,
}

pub trait WindowSystem {
    fn displays(&mut self) -> Vec<Display>;
    /// The managed window under `point`; errors explain why there is none.
    fn window_at(&mut self, point: Point) -> Result<WindowInfo, WindowError>;
    /// The frontmost app's focused window.
    fn focused_window(&mut self) -> Result<WindowInfo, WindowError>;
    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError>;
    fn set_size(&mut self, window: WindowId, size: Size) -> Result<(), WindowError>;
    fn set_position(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError>;
    fn state(&mut self, window: WindowId) -> Result<WindowState, WindowError>;
    fn is_closed(&mut self, window: WindowId) -> bool;
    /// Visible managed windows on the current native desktop, optionally of one app.
    fn visible_windows(&mut self, app: Option<&str>) -> Vec<WindowInfo>;
    /// Every managed window, including minimized and parked ones.
    fn all_windows(&mut self) -> Vec<WindowInfo>;
    /// Windows filling any of `frames`, for a swap drop.
    fn windows_occupying(&mut self, frames: &[Rect]) -> Result<Vec<WindowInfo>, WindowError>;
    /// The tab groups, with at least one hidden tab, of the apps that own `windows`. Hidden
    /// tabs are never among the visible windows or the ones filling zones. Platforms whose
    /// tabs live inside one window report none.
    fn tab_groups(&mut self, _windows: &[WindowId]) -> Vec<TabGroup> {
        Vec::new()
    }
    fn focus(&mut self, window: WindowId);
    /// Prepare an app for a bounds change and undo it afterwards, for apps that block
    /// bounds changes in some mode, such as Chrome with enhanced accessibility on macOS.
    /// Ending returns how long the app needs before the window's frame is final.
    fn begin_bounds_change(&mut self, window: WindowId);
    fn end_bounds_change(&mut self, window: WindowId) -> Duration;
    /// Hide a window for a workspace switch: park it at `parked` when the platform parks
    /// by moving, or move it to the parking desktop without touching its frame.
    fn hide_for_workspace(&mut self, window: WindowId, parked: Point) -> Result<(), WindowError>;
    fn show_for_workspace(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError>;
    /// Give Snappy itself focus, so typing doesn't reach a parked window.
    fn activate_self(&mut self);
    /// Where the pointer is now.
    fn pointer(&mut self) -> Point;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(pid: u32, launch_time: f64, window: &str) -> RecoveryKey {
        RecoveryKey {
            pid,
            launch_time,
            window: window.to_owned(),
        }
    }

    #[test]
    fn a_window_is_the_same_only_within_the_same_process_start() {
        let original = key(100, 1_700_000_000.25, "42");
        assert!(original.same_window(&key(100, 1_700_000_000.25, "42")));
        assert!(
            !original.same_window(&key(100, 1_700_000_000.25, "43")),
            "another window"
        );
        assert!(
            !original.same_window(&key(100, 1_700_000_100.0, "42")),
            "the process ID taken by a later process"
        );
        assert!(!original.same_window(&key(101, 1_700_000_000.25, "42")));
    }
}
