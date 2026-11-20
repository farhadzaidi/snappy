//! The engine thread's handle. Window reads and moves happen here directly; hotkeys,
//! hooks, overlays, the tray, and the desktop guard belong to the main thread, so those
//! calls go through the dispatcher.

use std::sync::Arc;
use std::time::{Duration, Instant};

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, Mods, Os};
use snappy_core::send_to_back::Restacking;
use snappy_core::workspace::ParkingPolicy;
use snappy_platform::{
    Capabilities, CapsLock, CapsLockRefusal, CapsLockUndo, DesktopGuard, Display, HotKeyRequest, HotKeys,
    Input, LiveControl, LoginItem, Overlay, OverlayMarks, OverlayScene, Permissions, Platform, Tray,
    TrayMenu, WindowId, WindowInfo, WindowState, WindowSystem,
};

use crate::dispatch::Dispatcher;
use crate::managed::Windows;
use crate::shared::{Shared, lock};
use crate::{caps_lock, displays, login_item, messages};

pub struct WindowsPlatform {
    windows: Windows,
    shared: Arc<Shared>,
    dispatcher: Dispatcher,
    epoch: Instant,
}

const _: () = {
    const fn assert_send<T: Send + 'static>() {}
    assert_send::<WindowsPlatform>();
};

impl WindowsPlatform {
    pub(crate) fn new(shared: Arc<Shared>, dispatcher: Dispatcher) -> Self {
        WindowsPlatform {
            windows: Windows::new(Arc::clone(&shared), dispatcher.clone()),
            shared,
            dispatcher,
            epoch: Instant::now(),
        }
    }

    fn set_monitoring(&self, change: impl FnOnce(&mut crate::shared::Monitoring)) {
        let monitoring = {
            let mut monitoring = lock(&self.shared.monitoring);
            change(&mut monitoring);
            *monitoring
        };
        self.dispatcher
            .post(move |main| main.set_input_hooks(monitoring.drag));
    }
}

impl WindowSystem for WindowsPlatform {
    fn displays(&mut self) -> Vec<Display> {
        let displays = displays::read();
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
        Ok(self.windows.windows_occupying(frames))
    }

    fn focus(&mut self, window: WindowId) {
        self.windows.focus(window);
    }

    fn lower(&mut self, window: WindowId) -> Result<(), WindowError> {
        self.windows.lower(window)
    }

    fn stack_below(&mut self, window: WindowId, sibling: WindowId) -> Result<(), WindowError> {
        self.windows.stack_below(window, sibling)
    }

    fn begin_bounds_change(&mut self, _window: WindowId) {}

    fn end_bounds_change(&mut self, _window: WindowId) -> Duration {
        Duration::ZERO
    }

    /// Off-screen parking moves the window.
    fn hide_for_workspace(&mut self, window: WindowId, parked: Point) -> Result<(), WindowError> {
        self.windows.place(window, parked)
    }

    fn show_for_workspace(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.windows.place(window, origin)
    }

    fn activate_self(&mut self) {
        self.windows.activate_self();
    }

    fn pointer(&mut self) -> Point {
        self.windows.pointer()
    }
}

impl Input for WindowsPlatform {
    fn set_press_monitoring(&mut self, on: bool) {
        self.set_monitoring(|monitoring| monitoring.press = on);
    }

    fn set_drag_monitoring(&mut self, on: bool) {
        self.set_monitoring(|monitoring| monitoring.drag = on);
    }
}

impl HotKeys for WindowsPlatform {
    fn register(&mut self, shortcuts: &[HotKeyRequest]) -> Vec<u32> {
        let shortcuts = shortcuts.to_vec();
        let all: Vec<u32> = shortcuts.iter().map(|shortcut| shortcut.id).collect();
        self.dispatcher
            .call(move |main| main.register_hotkeys(&shortcuts))
            .unwrap_or(all)
    }

    fn unregister_all(&mut self) {
        self.dispatcher.post(|main| main.unregister_hotkeys());
    }

    fn is_available(&mut self, combo: &Combo) -> bool {
        let combo = combo.clone();
        self.dispatcher
            .call(move |main| main.hotkey_available(&combo))
            .unwrap_or(false)
    }
}

/// The keyboard hook does it all, so nothing outlives the process and nothing needs undoing.
impl CapsLock for WindowsPlatform {
    fn hold_caps_lock_as(&mut self, modifiers: Mods) -> Result<CapsLockUndo, CapsLockRefusal> {
        if caps_lock::system_remaps_caps_lock() {
            self.restore_caps_lock();
            return Err(CapsLockRefusal::Refused(messages::CAPS_LOCK_REMAPPED.to_owned()));
        }
        match self.dispatcher.call(move |main| main.hold_caps_lock(modifiers)) {
            Some(true) => Ok(CapsLockUndo::default()),
            _ => Err(CapsLockRefusal::Refused(
                messages::CAPS_LOCK_UNAVAILABLE.to_owned(),
            )),
        }
    }

    fn restore_caps_lock(&mut self) {
        // Waits, so that quitting gives Caps Lock back before the process ends.
        self.dispatcher.call(|main| main.restore_caps_lock());
    }

    fn undo_caps_lock(&mut self, _undo: &CapsLockUndo) {}

    fn toggle_caps_lock(&mut self) -> Result<(), String> {
        caps_lock::toggle()
            .then_some(())
            .ok_or_else(|| "Windows didn't take the Caps Lock press".to_owned())
    }
}

impl Overlay for WindowsPlatform {
    fn prepare(&mut self, display: &str, scene: &OverlayScene) {
        let (display, scene) = (display.to_owned(), scene.clone());
        let scale = self.shared.scale(&display);
        self.dispatcher.post(move |main| {
            main.overlays.prepare(&display, &scene, scale);
        });
    }

    fn show(&mut self, display: &str, scene: &OverlayScene) {
        let (display, scene) = (display.to_owned(), scene.clone());
        let scale = self.shared.scale(&display);
        self.dispatcher
            .post(move |main| main.overlays.show(&display, &scene, scale));
    }

    fn hide(&mut self) {
        self.dispatcher.post(|main| main.overlays.hide());
    }

    fn retain(&mut self, displays: &[String]) {
        let displays = displays.to_vec();
        self.dispatcher.post(move |main| main.overlays.retain(&displays));
    }

    fn mark(&mut self, display: &str, marks: &OverlayMarks) {
        let (display, marks) = (display.to_owned(), marks.clone());
        let scale = self.shared.scale(&display);
        self.dispatcher
            .post(move |main| main.overlays.mark(&display, &marks, scale));
    }
}

/// Windows has no permission step.
impl Permissions for WindowsPlatform {
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

impl DesktopGuard for WindowsPlatform {
    /// Virtual desktops span every display, so the displays don't matter here.
    fn bind(&mut self, _displays: &[Display]) {
        self.dispatcher.post(|main| main.guard.bind());
    }

    fn on_managed_desktop(&mut self) -> bool {
        self.dispatcher
            .call(|main| main.guard.on_managed_desktop())
            .unwrap_or(true)
    }

    fn clear(&mut self) {
        self.dispatcher.post(|main| main.guard.clear());
    }
}

impl Tray for WindowsPlatform {
    fn update(&mut self, menu: &TrayMenu) {
        let menu = menu.clone();
        self.dispatcher.post(move |main| main.tray.update(&menu));
    }
}

impl LoginItem for WindowsPlatform {
    fn is_enabled(&mut self) -> bool {
        login_item::is_enabled()
    }

    fn set_enabled(&mut self, on: bool) -> Result<(), String> {
        login_item::set_enabled(on)
    }
}

impl Platform for WindowsPlatform {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: Os::Windows,
            parking: ParkingPolicy::OffScreen,
            restacking: Restacking::Lower { stack_below: true },
            needs_permission: false,
            caps_lock_unavailable: None,
        }
    }

    fn live_control(&mut self) -> LiveControl {
        LiveControl::Available
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }
}
