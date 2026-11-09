//! The engine thread's handle: every platform trait, carried out by asking the script, or KDE's
//! shortcut service for shortcuts.

use std::sync::Arc;
use std::time::{Duration, Instant};

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, Platform as PlatformName};
use snappy_core::workspace::ParkingPolicy;
use snappy_linux::{journal, login_item};
use snappy_platform::{
    Capabilities, DesktopGuard, Display, HotKeys, Input, LiveControl, LoginItem, Overlay, OverlayScene,
    Permissions, Platform, Tray, TrayMenu, WindowId, WindowInfo, WindowState, WindowSystem,
};

use crate::bus::CallError;
use crate::messages::{
    HIDE_REFUSED, KWIN_UNREACHABLE, NO_FOCUSED_WINDOW, NOT_STANDARD, ON_ALL_DESKTOPS, UNEXPECTED_REPLY,
    WINDOW_GONE, unavailable,
};
use crate::overlay::{Command, Target};
use crate::protocol::{GONE, Monitor, PointerPosition, REFUSED_ALL_DESKTOPS, Request, WindowRecord};
use crate::shared::{Grab, Incoming, Shared};
use crate::{displays, keys, shortcuts, windows};

/// How long showing a parked window waits for its saved position to apply before bringing it
/// back, and how often it looks. Wayland apps confirm moves asynchronously.
const SHOW_TIMEOUT: Duration = Duration::from_millis(250);
const SHOW_POLL: Duration = Duration::from_millis(5);
/// A move has landed when the frame is this close to its target.
const LANDED: f64 = 1.0;
/// How long a write waits for a maximized window to be restored, and how often it looks.
/// A Wayland app confirms the restored size asynchronously; writing before that would be
/// undone when it does.
const RESTORE_TIMEOUT: Duration = Duration::from_millis(250);
const RESTORE_POLL: Duration = Duration::from_millis(10);

pub struct KWinPlatform {
    shared: Arc<Shared>,
    epoch: Instant,
}

const _: () = {
    const fn assert_send<T: Send + 'static>() {}
    assert_send::<KWinPlatform>();
};

fn window_error(error: CallError) -> WindowError {
    match error {
        CallError::Refused(reason) if reason == GONE => unavailable(WINDOW_GONE),
        CallError::Unreachable(_) => unavailable(KWIN_UNREACHABLE),
        CallError::Refused(_) | CallError::Malformed(_) => unavailable(UNEXPECTED_REPLY),
    }
}

impl KWinPlatform {
    pub(crate) fn new(shared: Arc<Shared>) -> KWinPlatform {
        KWinPlatform {
            shared,
            epoch: Instant::now(),
        }
    }

    fn call<T: serde::de::DeserializeOwned>(&self, request: &Request) -> Result<T, CallError> {
        self.shared.bus.call(request)
    }

    fn send(&self, request: &Request) -> Result<(), WindowError> {
        self.shared.bus.send(request).map_err(window_error)
    }

    /// Every managed window, top of the stack first. Empty when the script doesn't answer.
    fn records(&self) -> Vec<WindowRecord> {
        self.call(&Request::Windows).unwrap_or_default()
    }

    fn record(&self, window: WindowId) -> Result<WindowRecord, WindowError> {
        self.call::<Option<WindowRecord>>(&Request::Window { id: window.0 })
            .map_err(window_error)?
            .ok_or_else(|| unavailable(WINDOW_GONE))
    }

    /// Restores a maximized or tiled window and waits until its frame has changed, so a write
    /// that follows isn't undone by the restore.
    fn restored(&self, window: WindowId) -> Result<(), WindowError> {
        let record = self.record(window)?;
        if !record.maximized {
            return Ok(());
        }
        self.send(&Request::Unmaximize { id: window.0 })?;
        let maximized = Rect::from(record.frame);
        let deadline = Instant::now() + RESTORE_TIMEOUT;
        while Instant::now() < deadline {
            let current = self.record(window)?;
            if !current.maximized && Rect::from(current.frame) != maximized {
                break;
            }
            std::thread::sleep(RESTORE_POLL);
        }
        Ok(())
    }

    /// Starts reporting moves of windows handed to the engine, as other adapters do.
    fn follow<'a>(&self, records: impl IntoIterator<Item = &'a WindowRecord>) {
        let new: Vec<u64> = {
            let mut state = self.shared.state();
            records
                .into_iter()
                .map(|record| record.id)
                .filter(|id| state.followed.insert(*id))
                .collect()
        };
        if !new.is_empty() {
            let _ = self.shared.bus.send(&Request::Follow { ids: new });
        }
    }

    fn info(&self, record: &WindowRecord) -> WindowInfo {
        self.follow([record]);
        record.info()
    }

    fn infos(&self, records: &[&WindowRecord]) -> Vec<WindowInfo> {
        self.follow(records.iter().copied());
        records.iter().map(|record| record.info()).collect()
    }

    /// Where a display's overlay goes, reading the displays again for one not seen yet.
    fn target(&mut self, display: &str) -> Option<Target> {
        if let Some(target) = self.shared.state().targets.get(display) {
            return Some(target.clone());
        }
        self.displays();
        self.shared.state().targets.get(display).cloned()
    }

    fn send_monitoring(&self) {
        let request = self.shared.state().monitoring();
        let _ = self.shared.bus.send(&request);
    }
}

impl WindowSystem for KWinPlatform {
    fn displays(&mut self) -> Vec<Display> {
        let monitors: Vec<Monitor> = self.call(&Request::Monitors).unwrap_or_default();
        let displays = displays::displays(&monitors);
        self.shared.state().targets = displays
            .iter()
            .zip(&monitors)
            .map(|(display, monitor)| (display.id.clone(), displays::target(monitor)))
            .collect();
        displays
    }

    fn window_at(&mut self, point: Point) -> Result<WindowInfo, WindowError> {
        let records: Vec<WindowRecord> = self.call(&Request::Windows).map_err(window_error)?;
        let record = records
            .iter()
            .find(|record| record.is_visible() && Rect::from(record.frame).contains(point))
            .ok_or_else(|| unavailable(NOT_STANDARD))?;
        record.check()?;
        Ok(self.info(record))
    }

    fn focused_window(&mut self) -> Result<WindowInfo, WindowError> {
        let record = self
            .call::<Option<WindowRecord>>(&Request::Focused)
            .map_err(window_error)?
            .ok_or_else(|| unavailable(NO_FOCUSED_WINDOW))?;
        record.check()?;
        Ok(self.info(&record))
    }

    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        Ok(Rect::from(self.record(window)?.frame))
    }

    fn set_size(&mut self, window: WindowId, size: Size) -> Result<(), WindowError> {
        if !(size.width.is_finite() && size.height.is_finite() && size.width > 0.0 && size.height > 0.0) {
            return Err(WindowError::InvalidFrame);
        }
        self.restored(window)?;
        self.send(&Request::SetSize {
            id: window.0,
            width: size.width,
            height: size.height,
        })
    }

    fn set_position(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        if !(origin.x.is_finite() && origin.y.is_finite()) {
            return Err(WindowError::InvalidFrame);
        }
        self.restored(window)?;
        self.send(&Request::SetPosition {
            id: window.0,
            x: origin.x,
            y: origin.y,
        })
    }

    fn state(&mut self, window: WindowId) -> Result<WindowState, WindowError> {
        Ok(self.record(window)?.state())
    }

    /// A KWin that doesn't answer says nothing about the window, so it isn't closed.
    fn is_closed(&mut self, window: WindowId) -> bool {
        matches!(
            self.call::<Option<WindowRecord>>(&Request::Window { id: window.0 }),
            Ok(None)
        )
    }

    fn visible_windows(&mut self, app: Option<&str>) -> Vec<WindowInfo> {
        let records = self.records();
        let visible: Vec<&WindowRecord> = records
            .iter()
            .filter(|record| record.check().is_ok() && record.is_visible())
            .filter(|record| app.is_none_or(|app| record.app() == app))
            .collect();
        self.infos(&visible)
    }

    fn all_windows(&mut self) -> Vec<WindowInfo> {
        let records = self.records();
        let eligible: Vec<&WindowRecord> = records.iter().filter(|record| record.check().is_ok()).collect();
        self.infos(&eligible)
    }

    fn windows_occupying(&mut self, frames: &[Rect]) -> Result<Vec<WindowInfo>, WindowError> {
        let records: Vec<WindowRecord> = self.call(&Request::Windows).map_err(window_error)?;
        let occupants = windows::occupying(&records, frames);
        let followed: Vec<&WindowRecord> = records
            .iter()
            .filter(|record| occupants.iter().any(|info| info.id.0 == record.id))
            .collect();
        self.follow(followed);
        Ok(occupants)
    }

    fn focus(&mut self, window: WindowId) {
        let _ = self.send(&Request::Activate { id: window.0 });
    }

    fn begin_bounds_change(&mut self, _window: WindowId) {}

    fn end_bounds_change(&mut self, _window: WindowId) -> Duration {
        Duration::ZERO
    }

    /// Parking moves the window to Snappy's desktop and leaves its frame alone.
    fn hide_for_workspace(&mut self, window: WindowId, _parked: Point) -> Result<(), WindowError> {
        let refusal: Option<String> = self.call(&Request::Park { id: window.0 }).map_err(window_error)?;
        match refusal.as_deref() {
            None => Ok(()),
            Some(REFUSED_ALL_DESKTOPS) => Err(unavailable(ON_ALL_DESKTOPS)),
            Some(_) => Err(unavailable(HIDE_REFUSED)),
        }
    }

    /// Re-applies the saved position while the window is still hidden, then brings it back
    /// from Snappy's desktop, so it never shows anywhere else.
    fn show_for_workspace(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        let landed = |frame: Rect| (frame.x - origin.x).abs() < LANDED && (frame.y - origin.y).abs() < LANDED;
        let record = self.record(window)?;
        if !landed(Rect::from(record.frame)) {
            self.set_position(window, origin)?;
            let deadline = Instant::now() + SHOW_TIMEOUT;
            while !self.frame(window).is_ok_and(landed) && Instant::now() < deadline {
                std::thread::sleep(SHOW_POLL);
            }
        }
        if record.parked {
            self.send(&Request::Unpark { id: window.0 })?;
        }
        Ok(())
    }

    fn activate_self(&mut self) {
        let _ = self.send(&Request::ClearFocus);
    }

    fn pointer(&mut self) -> Point {
        self.call::<PointerPosition>(&Request::Pointer)
            .map_or(Point::default(), |pointer| Point::new(pointer.x, pointer.y))
    }
}

impl Input for KWinPlatform {
    fn set_press_monitoring(&mut self, on: bool) {
        self.shared.state().press_monitoring = on;
        self.send_monitoring();
    }

    /// Modifiers are followed only while a drag runs.
    fn set_drag_monitoring(&mut self, on: bool) {
        self.shared.state().drag_monitoring = on;
        self.shared.follow_modifiers(on);
        self.send_monitoring();
    }
}

impl HotKeys for KWinPlatform {
    /// Chords that resolve to the same key share one action, the last ID winning.
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32> {
        let mut refused = Vec::new();
        let mut wanted: Vec<(u32, i32)> = Vec::new();
        let mut sharing: Vec<(i32, u32)> = Vec::new();
        for (id, combo) in keys {
            let Some(key) = keys::key_code(combo) else {
                refused.push(*id);
                continue;
            };
            sharing.push((key, *id));
            match wanted.iter_mut().find(|(_, wanted_key)| *wanted_key == key) {
                Some(grab) => grab.0 = *id,
                None => wanted.push((*id, key)),
            }
        }
        let previous = std::mem::take(&mut self.shared.state().registered);
        let (registered, refused_keys) = shortcuts::register(&self.shared.bus, &previous, &wanted);
        for key in refused_keys {
            refused.extend(
                sharing
                    .iter()
                    .filter(|(shared, _)| *shared == key)
                    .map(|(_, id)| *id),
            );
        }
        self.shared.state().registered = registered;
        refused.sort_unstable();
        refused.dedup();
        refused
    }

    fn unregister_all(&mut self) {
        let previous = std::mem::take(&mut self.shared.state().registered);
        shortcuts::unregister(&self.shared.bus, &previous);
    }

    /// Snappy's own registrations count as free; anything else is asked about.
    fn is_available(&mut self, combo: &Combo) -> bool {
        let Some(key) = keys::key_code(combo) else {
            return false;
        };
        let held = |grab: &Grab| grab.key == key;
        if self.shared.state().registered.iter().any(held) {
            return true;
        }
        shortcuts::is_free(&self.shared.bus, key)
    }
}

/// Overlays are drawn by the overlay thread (`overlay.rs`); the engine never waits for them.
impl Overlay for KWinPlatform {
    /// Makes the display's overlay surface ahead of the first drag, invisibly, so KWin's
    /// effects never see it appear while it shows anything.
    fn prepare(&mut self, display: &str, scene: &OverlayScene) {
        let Some(target) = self.target(display) else {
            journal::warn(format_args!(
                "the zone overlay can't be prepared: no display {display}"
            ));
            return;
        };
        self.shared.overlays.send(Command::Prepare {
            display: display.to_owned(),
            target,
            scene: scene.clone(),
        });
    }

    /// The engine shows the overlay on every pointer move; only changes are drawn.
    fn show(&mut self, display: &str, scene: &OverlayScene) {
        let shown = Some((display.to_owned(), scene.clone()));
        if self.shared.state().overlay_shown == shown {
            return;
        }
        let Some(target) = self.target(display) else {
            journal::warn(format_args!(
                "the zone overlay can't be shown: no display {display}"
            ));
            return;
        };
        self.shared.state().overlay_shown = shown;
        self.shared.overlays.send(Command::Show {
            display: display.to_owned(),
            target,
            scene: scene.clone(),
        });
    }

    fn hide(&mut self) {
        if self.shared.state().overlay_shown.take().is_some() {
            self.shared.overlays.send(Command::Hide);
        }
    }

    fn retain(&mut self, displays: &[String]) {
        {
            let mut state = self.shared.state();
            state.targets.retain(|display, _| displays.contains(display));
            if state
                .overlay_shown
                .as_ref()
                .is_some_and(|(display, _)| !displays.contains(display))
            {
                state.overlay_shown = None;
            }
        }
        self.shared.overlays.send(Command::Retain(displays.to_vec()));
    }
}

/// KDE Plasma has no permission step; the script is the requirement, and live control
/// reports on it.
impl Permissions for KWinPlatform {
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

/// Workspaces belong to the virtual desktop in use when they started; on any other, Snappy's
/// own included, they pause.
impl DesktopGuard for KWinPlatform {
    fn bind(&mut self, _displays: &[Display]) {
        let _ = self.send(&Request::Bind);
    }

    fn on_managed_desktop(&mut self) -> bool {
        self.call(&Request::OnBoundDesktop).unwrap_or(false)
    }

    fn clear(&mut self) {
        let _ = self.shared.bus.send(&Request::ClearDesktops);
    }
}

/// The tray icon lives on the event loop's thread, so the engine never waits for it.
impl Tray for KWinPlatform {
    fn update(&mut self, tray: &TrayMenu) {
        self.shared.send_incoming(Incoming::Tray(tray.clone()));
    }
}

impl LoginItem for KWinPlatform {
    fn is_enabled(&mut self) -> bool {
        login_item::enabled()
    }

    fn set_enabled(&mut self, on: bool) -> Result<(), String> {
        login_item::set_enabled_for_this_program(on)
    }
}

impl Platform for KWinPlatform {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: PlatformName::Linux,
            parking: ParkingPolicy::Desktop,
            needs_permission: false,
        }
    }

    fn live_control(&mut self) -> LiveControl {
        self.shared.live_control()
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }
}
