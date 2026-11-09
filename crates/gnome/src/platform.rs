//! The engine thread's handle: every platform trait, carried out by asking the extension.

use std::sync::Arc;
use std::time::{Duration, Instant};

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, Modifier, Platform as PlatformName};
use snappy_core::workspace::ParkingPolicy;
use snappy_linux::{login_item, scene};
use snappy_platform::{
    Capabilities, DesktopGuard, Display, HotKeys, Input, LiveControl, LoginItem, Overlay, OverlayScene,
    Permissions, Platform, Remedy, Tray, TrayMenu, WindowId, WindowInfo, WindowState, WindowSystem,
};

use crate::bus::CallError;
use crate::messages::{
    HIDE_REFUSED, LOG_OUT_FAILED, NO_FOCUSED_WINDOW, NOT_STANDARD, ON_ALL_WORKSPACES, SHELL_UNREACHABLE,
    UNEXPECTED_REPLY, WINDOW_GONE, unavailable,
};
use crate::protocol::{
    Accelerator, GONE, Monitor, PointerPosition, REFUSED_ALL_WORKSPACES, Request, WindowRecord,
};
use crate::shared::Shared;
use crate::{displays, extension, keys, menu, windows};

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

pub struct GnomePlatform {
    shared: Arc<Shared>,
    epoch: Instant,
}

const _: () = {
    const fn assert_send<T: Send + 'static>() {}
    assert_send::<GnomePlatform>();
};

fn window_error(error: CallError) -> WindowError {
    match error {
        CallError::Refused(reason) if reason == GONE => unavailable(WINDOW_GONE),
        CallError::Unreachable(_) => unavailable(SHELL_UNREACHABLE),
        CallError::Refused(_) | CallError::Malformed(_) => unavailable(UNEXPECTED_REPLY),
    }
}

impl GnomePlatform {
    pub(crate) fn new(shared: Arc<Shared>) -> GnomePlatform {
        GnomePlatform {
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

    /// Every managed window, top of the stack first. Empty when the extension doesn't answer.
    fn records(&self) -> Vec<WindowRecord> {
        self.call(&Request::Windows).unwrap_or_default()
    }

    /// Restores a maximized window and waits until its frame has changed, so a write that
    /// follows isn't undone by the restore.
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

    fn record(&self, window: WindowId) -> Result<WindowRecord, WindowError> {
        self.call::<Option<WindowRecord>>(&Request::Window { id: window.0 })
            .map_err(window_error)?
            .ok_or_else(|| unavailable(WINDOW_GONE))
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

    fn send_monitoring(&self) {
        let request = self.shared.state().monitoring();
        let _ = self.shared.bus.send(&request);
    }

    fn scale(&self, display: &str) -> f64 {
        self.shared.state().scales.get(display).copied().unwrap_or(1.0)
    }
}

impl WindowSystem for GnomePlatform {
    fn displays(&mut self) -> Vec<Display> {
        let state = self.shared.bus.display_state();
        let monitors: Option<Vec<Monitor>> = self.call(&Request::Monitors).ok();
        let displays = displays::merge(state.as_ref(), monitors.as_deref());
        self.shared.state().scales = displays
            .iter()
            .map(|display| (display.id.clone(), display.scale))
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

    /// A shell that doesn't answer says nothing about the window, so it isn't closed.
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

    /// Parking moves the window to the parking workspace and leaves its frame alone.
    fn hide_for_workspace(&mut self, window: WindowId, _parked: Point) -> Result<(), WindowError> {
        let refusal: Option<String> = self.call(&Request::Park { id: window.0 }).map_err(window_error)?;
        match refusal.as_deref() {
            None => Ok(()),
            Some(REFUSED_ALL_WORKSPACES) => Err(unavailable(ON_ALL_WORKSPACES)),
            Some(_) => Err(unavailable(HIDE_REFUSED)),
        }
    }

    /// Re-applies the saved position while the window is still hidden, then brings it back
    /// from the parking workspace, so it never shows anywhere else.
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

impl Input for GnomePlatform {
    fn set_press_monitoring(&mut self, on: bool) {
        self.shared.state().press_monitoring = on;
        self.send_monitoring();
    }

    fn set_drag_monitoring(&mut self, on: bool) {
        self.shared.state().drag_monitoring = on;
        self.send_monitoring();
    }

    /// GNOME pulls a window moved with Shift onto nearby edges, and with Ctrl makes other
    /// windows' edges resist, so while the drag key is held the extension keeps the window
    /// under the pointer and the zone overlay is the only guide.
    fn set_drag_modifier(&mut self, modifier: Modifier) {
        let follow = keys::mask(modifier);
        let changed = {
            let mut state = self.shared.state();
            let changed = state.follow != follow;
            state.follow = follow;
            changed
        };
        if changed {
            self.send_monitoring();
        }
    }
}

impl HotKeys for GnomePlatform {
    /// Chords that resolve to the same accelerator share one grab, the last ID winning, as
    /// they do on X11.
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32> {
        let mut refused = Vec::new();
        let mut grabs: Vec<Accelerator> = Vec::new();
        let mut sharing: Vec<(String, u32)> = Vec::new();
        for (id, combo) in keys {
            let Some(accelerator) = keys::accelerator(combo) else {
                refused.push(*id);
                continue;
            };
            sharing.push((accelerator.clone(), *id));
            match grabs.iter_mut().find(|grab| grab.accelerator == accelerator) {
                Some(grab) => grab.id = *id,
                None => grabs.push(Accelerator { id: *id, accelerator }),
            }
        }
        let refused_grabs: Vec<u32> = match self.call(&Request::Register { keys: grabs.clone() }) {
            Ok(ids) => ids,
            // GNOME isn't answering, which says nothing about the chords: keep them and grab
            // them when the extension is back, rather than report them as taken.
            Err(CallError::Unreachable(_)) => {
                let mut state = self.shared.state();
                state.registered = grabs;
                state.replay_needed = true;
                return refused;
            }
            Err(_) => grabs.iter().map(|grab| grab.id).collect(),
        };
        for grab in &grabs {
            if refused_grabs.contains(&grab.id) {
                refused.extend(
                    sharing
                        .iter()
                        .filter(|(accelerator, _)| *accelerator == grab.accelerator)
                        .map(|(_, id)| *id),
                );
            }
        }
        self.shared.state().registered = grabs
            .into_iter()
            .filter(|grab| !refused_grabs.contains(&grab.id))
            .collect();
        refused.sort_unstable();
        refused.dedup();
        refused
    }

    fn unregister_all(&mut self) {
        self.shared.state().registered.clear();
        let _ = self.send(&Request::UnregisterAll);
    }

    /// Snappy's own registrations count as free; anything else is probed.
    fn is_available(&mut self, combo: &Combo) -> bool {
        let Some(accelerator) = keys::accelerator(combo) else {
            return false;
        };
        if self
            .shared
            .state()
            .registered
            .iter()
            .any(|grab| grab.accelerator == accelerator)
        {
            return true;
        }
        self.call(&Request::IsFree { accelerator }).unwrap_or(false)
    }
}

impl Overlay for GnomePlatform {
    fn prepare(&mut self, display: &str, scene: &OverlayScene) {
        let scene = scene::scene(scene, self.scale(display));
        if self.shared.state().overlay_prepared.get(display) == Some(&scene) {
            return;
        }
        let request = Request::OverlayPrepare {
            display: display.to_owned(),
            scene: scene.clone(),
        };
        if self.shared.bus.send(&request).is_ok() {
            self.shared
                .state()
                .overlay_prepared
                .insert(display.to_owned(), scene);
        }
    }

    /// The engine shows the overlay on every pointer move; only changes cross the bus.
    fn show(&mut self, display: &str, scene: &OverlayScene) {
        let scene = scene::scene(scene, self.scale(display));
        let shown = Some((display.to_owned(), scene.clone()));
        if self.shared.state().overlay_shown == shown {
            return;
        }
        let request = Request::OverlayShow {
            display: display.to_owned(),
            scene: scene.clone(),
        };
        if self.shared.bus.send(&request).is_ok() {
            let mut state = self.shared.state();
            state.overlay_prepared.insert(display.to_owned(), scene);
            state.overlay_shown = shown;
        }
    }

    fn hide(&mut self) {
        if self.shared.state().overlay_shown.take().is_some() {
            let _ = self.send(&Request::OverlayHide);
        }
    }

    fn retain(&mut self, displays: &[String]) {
        {
            let mut state = self.shared.state();
            state
                .overlay_prepared
                .retain(|display, _| displays.contains(display));
            if state
                .overlay_shown
                .as_ref()
                .is_some_and(|(display, _)| !displays.contains(display))
            {
                state.overlay_shown = None;
            }
        }
        let _ = self.send(&Request::OverlayRetain {
            displays: displays.to_vec(),
        });
    }
}

/// GNOME on Wayland has no permission step; the extension is the requirement, and live
/// control reports on it.
impl Permissions for GnomePlatform {
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

/// Workspaces belong to the GNOME workspace in use when they started; on any other, the
/// parking workspace included, they pause.
impl DesktopGuard for GnomePlatform {
    fn bind(&mut self, _displays: &[Display]) {
        let _ = self.send(&Request::Bind);
    }

    fn on_managed_desktop(&mut self) -> bool {
        self.call(&Request::OnBoundWorkspace).unwrap_or(false)
    }

    fn clear(&mut self) {
        let _ = self.shared.bus.send(&Request::ClearWorkspaces);
    }
}

impl Tray for GnomePlatform {
    fn update(&mut self, tray: &TrayMenu) {
        let (entries, actions) = menu::build(tray);
        let menu = (tray.tooltip.clone(), entries);
        let generation = {
            let mut state = self.shared.state();
            if state.menu.as_ref() == Some(&menu) {
                return;
            }
            state.menu_generation = state.menu_generation.wrapping_add(1);
            state.menu = Some(menu.clone());
            state.menu_actions = actions;
            state.menu_generation
        };
        let (name, entries) = menu;
        let _ = self.send(&Request::Menu {
            name,
            generation,
            entries,
        });
    }
}

impl LoginItem for GnomePlatform {
    fn is_enabled(&mut self) -> bool {
        login_item::enabled()
    }

    fn set_enabled(&mut self, on: bool) -> Result<(), String> {
        login_item::set_enabled_for_this_program(on)
    }
}

impl Platform for GnomePlatform {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: PlatformName::Linux,
            parking: ParkingPolicy::Desktop,
            needs_permission: false,
        }
    }

    fn apply_remedy(&mut self, remedy: Remedy) -> Result<(), String> {
        match remedy {
            Remedy::LogOut => self.shared.bus.log_out().map_err(|error| {
                eprintln!("snappy: GNOME didn't log out: {error:?}");
                LOG_OUT_FAILED.to_owned()
            }),
        }
    }

    fn notes(&mut self) -> Vec<String> {
        extension::notes(&self.shared.bus)
    }

    fn live_control(&mut self) -> LiveControl {
        self.shared.live_control()
    }

    fn now(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }
}
