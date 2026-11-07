//! The engine-thread handle. Accessibility calls run here directly; AppKit work is
//! dispatched to the main thread, synchronously only when a value is needed.

use std::time::{Duration, Instant};

use objc2_app_kit::NSApplication;
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, Platform as PlatformName};
use snappy_core::workspace::ParkingPolicy;
use snappy_platform::{
    Capabilities, DesktopGuard, Display, HotKeys, Input, LoginItem, Overlay, OverlayScene, Permissions,
    Platform, Tray, TrayMenu, WindowId, WindowInfo, WindowState, WindowSystem,
};

use crate::ax::{self, AxElement};
use crate::registry::Shared;
use crate::windows::Windows;
use crate::{
    desktop_guard, displays, hotkeys, login_item, main_thread, monitors, overlay, permissions, tray,
};

pub struct MacPlatform {
    windows: Windows,
    epoch: Instant,
}

const _: () = {
    const fn assert_send<T: Send + 'static>() {}
    assert_send::<MacPlatform>();
};

impl MacPlatform {
    pub(crate) fn new(registry: Shared<AxElement>) -> Self {
        Self {
            windows: Windows::new(registry),
            epoch: Instant::now(),
        }
    }
}

impl Drop for MacPlatform {
    /// Outstanding Chrome changes are undone at quit.
    fn drop(&mut self) {
        self.windows.restore_bounds_changes();
    }
}

impl WindowSystem for MacPlatform {
    fn displays(&mut self) -> Vec<Display> {
        main_thread::sync(displays::read)
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
        self.windows.state(window)
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
        self.windows.windows_occupying(frames)
    }

    fn focus(&mut self, window: WindowId) {
        self.windows.focus(window);
    }

    fn begin_bounds_change(&mut self, window: WindowId) {
        self.windows.begin_bounds_change(window);
    }

    fn end_bounds_change(&mut self, window: WindowId) -> Duration {
        self.windows.end_bounds_change(window)
    }

    /// The corner-sliver policy parks by moving.
    fn hide_for_workspace(&mut self, window: WindowId, parked: Point) -> Result<(), WindowError> {
        self.windows.set_position(window, parked)
    }

    fn show_for_workspace(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.windows.set_position(window, origin)
    }

    fn activate_self(&mut self) {
        main_thread::spawn(|mtm| NSApplication::sharedApplication(mtm).activate());
    }

    fn pointer(&mut self) -> Point {
        main_thread::sync(monitors::pointer)
    }
}

impl Input for MacPlatform {
    fn set_press_monitoring(&mut self, on: bool) {
        main_thread::spawn(move |mtm| monitors::set_press_monitoring(on, mtm));
    }

    fn set_drag_monitoring(&mut self, on: bool) {
        main_thread::spawn(move |mtm| monitors::set_drag_monitoring(on, mtm));
    }
}

impl HotKeys for MacPlatform {
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32> {
        let keys = keys.to_vec();
        main_thread::sync(move |_| hotkeys::register(&keys))
    }

    fn unregister_all(&mut self) {
        main_thread::spawn(|_| hotkeys::unregister_all());
    }

    fn is_available(&mut self, combo: &Combo) -> bool {
        let combo = combo.clone();
        main_thread::sync(move |_| hotkeys::is_available(&combo))
    }
}

impl Overlay for MacPlatform {
    fn prepare(&mut self, display: &str, scene: &OverlayScene) {
        let (display, scene) = (display.to_owned(), scene.clone());
        main_thread::spawn(move |mtm| overlay::prepare(&display, &scene, mtm));
    }

    fn show(&mut self, display: &str, scene: &OverlayScene) {
        let (display, scene) = (display.to_owned(), scene.clone());
        main_thread::spawn(move |mtm| overlay::show(&display, &scene, mtm));
    }

    fn hide(&mut self) {
        main_thread::spawn(|_| overlay::hide());
    }

    fn retain(&mut self, displays: &[String]) {
        let displays = displays.to_vec();
        main_thread::spawn(move |_| overlay::retain(&displays));
    }
}

impl Permissions for MacPlatform {
    fn is_granted(&mut self) -> bool {
        ax::is_trusted()
    }

    fn request_access(&mut self) -> bool {
        main_thread::spawn(|_| permissions::request_access());
        true
    }

    fn open_settings_page(&mut self) -> bool {
        permissions::open_settings_page()
    }

    fn watch(&mut self, waiting_for_grant: bool) {
        main_thread::spawn(move |_| permissions::set_polling(waiting_for_grant));
    }
}

impl DesktopGuard for MacPlatform {
    fn bind(&mut self, displays: &[Display]) {
        let displays = displays.to_vec();
        main_thread::spawn(move |mtm| desktop_guard::bind(&displays, mtm));
    }

    fn on_managed_desktop(&mut self) -> bool {
        main_thread::sync(|_| desktop_guard::on_managed_desktop())
    }

    fn clear(&mut self) {
        main_thread::spawn(|_| desktop_guard::clear());
    }
}

impl Tray for MacPlatform {
    fn update(&mut self, menu: &TrayMenu) {
        let menu = menu.clone();
        main_thread::spawn(move |mtm| tray::update(menu, mtm));
    }
}

impl LoginItem for MacPlatform {
    fn is_enabled(&mut self) -> bool {
        main_thread::sync(|_| login_item::is_enabled())
    }

    fn set_enabled(&mut self, on: bool) -> Result<(), String> {
        main_thread::sync(move |_| login_item::set_enabled(on))
    }
}

impl Platform for MacPlatform {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: PlatformName::MacOs,
            parking: ParkingPolicy::CornerSliver,
            needs_permission: true,
            live_control_available: true,
        }
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }
}
