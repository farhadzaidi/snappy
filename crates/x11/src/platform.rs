//! The engine thread's handle. Window requests go out on the engine connection; key
//! grabs and input selection use the event connection, so their events reach the event
//! thread; overlay and tray work goes to the overlay thread.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, Platform as PlatformName};
use snappy_core::workspace::ParkingPolicy;
use snappy_platform::{
    Capabilities, DesktopGuard, Display, HotKeys, Input, LiveControl, LoginItem, Overlay, OverlayScene,
    Permissions, Platform, Tray, TrayMenu, WindowId, WindowInfo, WindowState, WindowSystem,
};
use x11rb::rust_connection::RustConnection;

use crate::messages::{NO_PARKING_DESKTOP, WAYLAND, unavailable};
use crate::overlay::UiCommand;
use crate::shared::Shared;
use crate::windows::Windows;
use crate::x::lock;
use snappy_linux::login_item;

use crate::{displays, hotkeys, input};

pub struct X11Platform {
    windows: Windows,
    shared: Arc<Shared>,
    ui: Sender<UiCommand>,
    live: bool,
    /// The native desktop workspaces were bound to.
    bound_desktop: Option<u32>,
    /// The desktop Snappy added to park workspace windows on.
    parking_desktop: Option<u32>,
    epoch: Instant,
}

const _: () = {
    const fn assert_send<T: Send + 'static>() {}
    assert_send::<X11Platform>();
};

impl X11Platform {
    pub(crate) fn new(
        conn: Arc<RustConnection>,
        shared: Arc<Shared>,
        ui: Sender<UiCommand>,
        live: bool,
    ) -> Self {
        let windows = Windows::new(conn, Arc::clone(&shared));
        let parking_desktop = windows.find_parking_desktop();
        X11Platform {
            windows,
            shared,
            ui,
            live,
            bound_desktop: None,
            parking_desktop,
            epoch: Instant::now(),
        }
    }

    /// Where shown workspace windows live: the bound desktop, else the current one.
    fn home_desktop(&self) -> u32 {
        self.bound_desktop
            .or_else(|| self.windows.current_desktop())
            .unwrap_or(0)
    }

    fn ui(&self, command: UiCommand) {
        let _ = self.ui.send(command);
    }

    fn login_entry() -> Result<PathBuf, String> {
        login_item::entry_path().ok_or_else(|| "Snappy could not find your configuration folder.".to_owned())
    }
}

impl WindowSystem for X11Platform {
    fn displays(&mut self) -> Vec<Display> {
        let displays = displays::read(self.windows.conn(), self.windows.screen(), self.windows.atoms());
        *lock(&self.shared.scales) = displays
            .iter()
            .map(|display| (display.id.clone(), display.scale))
            .collect();
        displays
    }

    fn window_at(&mut self, point: Point) -> Result<WindowInfo, WindowError> {
        self.windows.window_at(point)
    }

    fn focused_window(&mut self) -> Result<WindowInfo, WindowError> {
        self.windows.focused_window()
    }

    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        self.windows.frame(window)
    }

    fn set_size(&mut self, window: WindowId, size: Size) -> Result<(), WindowError> {
        self.windows.set_size(window, size)
    }

    fn set_position(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.windows.set_position(window, origin)
    }

    fn state(&mut self, window: WindowId) -> Result<WindowState, WindowError> {
        self.windows.state(window, self.parking_desktop)
    }

    fn is_closed(&mut self, window: WindowId) -> bool {
        self.windows.is_closed(window)
    }

    fn visible_windows(&mut self, app: Option<&str>) -> Vec<WindowInfo> {
        self.windows.visible_windows(app)
    }

    fn all_windows(&mut self) -> Vec<WindowInfo> {
        self.windows.all_windows()
    }

    fn windows_occupying(&mut self, frames: &[Rect]) -> Result<Vec<WindowInfo>, WindowError> {
        Ok(self.windows.windows_occupying(frames))
    }

    fn focus(&mut self, window: WindowId) {
        self.windows.focus(window);
    }

    fn begin_bounds_change(&mut self, _window: WindowId) {}

    fn end_bounds_change(&mut self, _window: WindowId) -> Duration {
        Duration::ZERO
    }

    /// Parking moves the window to the parking desktop and leaves its frame alone.
    fn hide_for_workspace(&mut self, window: WindowId, _parked: Point) -> Result<(), WindowError> {
        let desktop = self
            .parking_desktop
            .ok_or_else(|| unavailable(NO_PARKING_DESKTOP))?;
        self.windows.move_to_desktop(window, desktop)
    }

    /// Re-applies the saved position while the window is still hidden, then brings it
    /// back from the parking desktop, so it never shows anywhere else.
    fn show_for_workspace(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.windows.move_and_wait(window, origin)?;
        let parked = self.windows.record(window)?.desktop;
        if parked.is_some() && parked == self.parking_desktop {
            self.windows.move_to_desktop(window, self.home_desktop())?;
        }
        Ok(())
    }

    fn activate_self(&mut self) {
        self.windows.activate_self();
    }

    fn pointer(&mut self) -> Point {
        self.windows.pointer()
    }
}

impl Input for X11Platform {
    fn set_press_monitoring(&mut self, on: bool) {
        input::set(&self.shared, |monitoring| monitoring.press = on);
    }

    fn set_drag_monitoring(&mut self, on: bool) {
        input::set(&self.shared, |monitoring| monitoring.drag = on);
    }
}

impl HotKeys for X11Platform {
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32> {
        hotkeys::register(&self.shared, keys)
    }

    fn unregister_all(&mut self) {
        hotkeys::unregister_all(&self.shared);
    }

    fn is_available(&mut self, combo: &Combo) -> bool {
        hotkeys::is_available(&self.shared, combo)
    }
}

impl Overlay for X11Platform {
    fn prepare(&mut self, display: &str, scene: &OverlayScene) {
        self.ui(UiCommand::Prepare(display.to_owned(), scene.clone()));
    }

    fn show(&mut self, display: &str, scene: &OverlayScene) {
        self.ui(UiCommand::Show(display.to_owned(), scene.clone()));
    }

    fn hide(&mut self) {
        self.ui(UiCommand::Hide);
    }

    fn retain(&mut self, displays: &[String]) {
        self.ui(UiCommand::Retain(displays.to_vec()));
    }
}

/// X11 has no permission step.
impl Permissions for X11Platform {
    fn is_granted(&mut self) -> bool {
        true
    }

    fn request_access(&mut self) -> bool {
        false
    }

    fn open_settings_page(&mut self) -> bool {
        false
    }

    fn watch(&mut self, _waiting_for_grant: bool) {}
}

/// Workspaces belong to the native desktop that was current when they started; on any
/// other desktop, the parking desktop included, they pause. Window managers
/// without desktops count as one. Binding also adds the parking desktop, and clearing
/// removes it.
impl DesktopGuard for X11Platform {
    fn bind(&mut self, _displays: &[Display]) {
        self.bound_desktop = Some(self.windows.current_desktop().unwrap_or(0));
        let count = self.windows.desktop_count().unwrap_or(0);
        if self.parking_desktop.is_none_or(|desktop| desktop >= count) {
            self.parking_desktop = self
                .windows
                .find_parking_desktop()
                .or_else(|| self.windows.add_parking_desktop().ok());
        }
    }

    fn on_managed_desktop(&mut self) -> bool {
        let current = self.windows.current_desktop().unwrap_or(0);
        self.bound_desktop == Some(current)
    }

    fn clear(&mut self) {
        self.bound_desktop = None;
        if let Some(desktop) = self.parking_desktop
            && self.windows.remove_parking_desktop(desktop)
        {
            self.parking_desktop = None;
        }
    }
}

impl Tray for X11Platform {
    fn update(&mut self, menu: &TrayMenu) {
        self.ui(UiCommand::Tray(menu.clone()));
    }
}

impl LoginItem for X11Platform {
    fn is_enabled(&mut self) -> bool {
        Self::login_entry().is_ok_and(|path| login_item::is_enabled(&path))
    }

    fn set_enabled(&mut self, on: bool) -> Result<(), String> {
        let path = Self::login_entry()?;
        let executable = std::env::current_exe()
            .map_err(|error| format!("Snappy could not find its own program: {error}"))?;
        login_item::set_enabled(&path, &executable, on)
    }
}

impl Platform for X11Platform {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: PlatformName::Linux,
            parking: ParkingPolicy::Desktop,
            needs_permission: false,
        }
    }

    /// X11 clients in a Wayland session see only XWayland's windows.
    fn live_control(&mut self) -> LiveControl {
        if self.live {
            LiveControl::Available
        } else {
            LiveControl::Unavailable(WAYLAND.to_owned())
        }
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }
}
