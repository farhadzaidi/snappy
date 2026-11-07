//! The interfaces every operating-system adapter implements, and the events adapters
//! report. The engine calls these from its own thread; calls may block only up to the
//! platform's messaging timeout.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, Mods, Platform as PlatformName};
use snappy_core::workspace::ParkingPolicy;

pub use snappy_core::window::WindowId;

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

/// Identifies a window across a daemon restart while its app keeps running.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryKey {
    pub pid: u32,
    pub launch_time: f64,
    pub window: String,
}

impl RecoveryKey {
    pub fn same_process(&self, other: &RecoveryKey) -> bool {
        self.pid == other.pid && (self.launch_time - other.launch_time).abs() < 0.001
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

pub trait Input {
    /// Primary-button presses and releases.
    fn set_press_monitoring(&mut self, on: bool);
    /// Drags, modifier changes, and Escape; on only during drags and window operations.
    fn set_drag_monitoring(&mut self, on: bool);
}

/// Global shortcuts. Each arrives as a `Combo`: the key and the concrete modifiers to
/// hold, with the Snappy key already expanded, so adapters never deal with it.
pub trait HotKeys {
    /// Register shortcuts, replacing any registered before. Returns the ones refused.
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32>;
    fn unregister_all(&mut self);
    /// Whether the system and other apps leave `combo` free.
    fn is_available(&mut self, combo: &Combo) -> bool;
}

/// What one display's overlay shows.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayScene {
    pub work_area: Rect,
    pub zones: Vec<OverlayZone>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayZone {
    pub frame: Rect,
    pub number: usize,
    pub highlighted: bool,
    /// Text under the number, naming what the drop does when it isn't the default.
    pub caption: Option<Caption>,
    /// Outline as the swap source.
    pub swap_source: bool,
}

/// What a drop does, shown under the hovered zone's number while the swap key reverses
/// "When the zone is taken".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Caption {
    Swap,
    Stack,
}

impl Caption {
    pub fn text(self) -> &'static str {
        match self {
            Caption::Swap => "Swap",
            Caption::Stack => "Stack",
        }
    }
}

pub trait Overlay {
    fn prepare(&mut self, display: &str, scene: &OverlayScene);
    fn show(&mut self, display: &str, scene: &OverlayScene);
    fn hide(&mut self);
    /// Release surfaces of displays not in `displays`.
    fn retain(&mut self, displays: &[String]);
}

pub trait Permissions {
    fn is_granted(&mut self) -> bool;
    /// Ask the system for permission, listing Snappy where the user grants it.
    /// False when the request couldn't be made.
    fn request_access(&mut self) -> bool;
    fn open_settings_page(&mut self) -> bool;
    /// Poll for a grant only while one is awaited.
    fn watch(&mut self, waiting_for_grant: bool);
}

pub trait DesktopGuard {
    fn bind(&mut self, displays: &[Display]);
    fn on_managed_desktop(&mut self) -> bool;
    fn clear(&mut self);
}

/// The menu bar menu's content.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayMenu {
    pub live: bool,
    pub tooltip: String,
    pub displays: Vec<TrayDisplay>,
    pub workspaces: Vec<TrayItem>,
    pub workspaces_running: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayDisplay {
    pub id: String,
    pub name: String,
    pub presets: Vec<TrayItem>,
    pub custom: Vec<TrayItem>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TrayItem {
    pub id: String,
    pub title: String,
    pub checked: bool,
    pub enabled: bool,
}

pub trait Tray {
    fn update(&mut self, menu: &TrayMenu);
}

pub trait LoginItem {
    fn is_enabled(&mut self) -> bool;
    fn set_enabled(&mut self, on: bool) -> Result<(), String>;
}

/// What a platform supports.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Capabilities {
    pub platform: PlatformName,
    pub parking: ParkingPolicy,
    /// macOS asks for Accessibility; Windows and X11 need nothing.
    pub needs_permission: bool,
    /// False in a Wayland session, where X11 can't see or move other apps' windows.
    pub live_control_available: bool,
}

pub trait Platform:
    WindowSystem + Input + HotKeys + Overlay + Permissions + DesktopGuard + Tray + LoginItem
{
    fn capabilities(&self) -> Capabilities;
    /// Monotonic seconds.
    fn now(&self) -> f64;
}

/// Something that happened outside the engine.
#[derive(Debug, Clone, PartialEq)]
pub enum PlatformEvent {
    /// A primary-button press. Windows names the window from its move/size start event.
    PointerDown {
        at: Point,
        mods: Mods,
        window: Option<WindowId>,
    },
    PointerDragged {
        at: Point,
        mods: Mods,
    },
    PointerUp {
        at: Point,
        mods: Mods,
    },
    ModifiersChanged {
        at: Point,
        mods: Mods,
    },
    Escape,
    HotKey(u32),
    WindowCreated {
        pid: u32,
    },
    /// The frontmost app or its focused window changed.
    FocusChanged {
        pid: u32,
        by_activation: bool,
    },
    /// A tracked window was moved, resized, or un-minimized by its app.
    WindowMoved(WindowId),
    WindowDestroyed(WindowId),
    AppLaunched {
        pid: u32,
    },
    AppTerminated {
        pid: u32,
    },
    DisplaysChanged,
    NativeDesktopChanged,
    PermissionChanged,
    WillSleep,
    /// A second launch, a reopen, or the menu's "Open Snappy…".
    OpenSettingsRequested,
    Tray(TrayCommand),
    QuitRequested,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrayCommand {
    ManageWorkspaces,
    SelectLayout { display: String, layout: String },
    SwitchWorkspace(String),
}
