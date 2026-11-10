//! An in-memory operating system for engine tests: displays, windows that behave like
//! real apps (lagging, refusing, constraining), and recorders for everything Snappy shows.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::keys::{Combo, Modifier, Platform as PlatformName};
use snappy_core::workspace::ParkingPolicy;
use snappy_platform::{
    Capabilities, DesktopGuard, Display, HotKeys, Input, LiveControl, LoginItem, Overlay, OverlayMarks,
    OverlayScene, Permissions, Platform, RecoveryKey, Remedy, Tray, TrayMenu, WindowId, WindowInfo,
    WindowState, WindowSystem,
};

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
    pub hidden: bool,
}

impl FakeWindow {
    pub fn new(app: &str, pid: u32, frame: Rect) -> Self {
        Self {
            app: app.to_owned(),
            pid,
            frame,
            state: WindowState::default(),
            min_size: Size::new(1.0, 1.0),
            strict_resize: false,
            frozen: false,
            hidden: false,
        }
    }

    /// Not hidden by its app and not on the parking desktop.
    pub fn is_shown(&self) -> bool {
        !self.hidden && !self.state.on_parking_desktop
    }
}

/// One thing Snappy did to the overlay.
#[derive(Debug, Clone, PartialEq)]
pub enum OverlayCall {
    Prepare(String),
    Show(String, OverlayScene),
    Hide,
}

#[derive(Debug, Clone)]
pub struct FakePlatform {
    pub platform: PlatformName,
    pub parking: ParkingPolicy,
    pub needs_permission: bool,
    /// What `live_control` answers.
    pub live_control: LiveControl,
    /// What `notes` answers.
    pub notes: Vec<String>,
    /// Remedies applied, in order.
    pub remedies: Vec<Remedy>,
    /// The drag key the engine last said it uses.
    pub drag_modifier: Option<Modifier>,
    pub displays: Vec<Display>,
    pub windows: BTreeMap<WindowId, FakeWindow>,
    /// Front to back.
    pub stacking: Vec<WindowId>,
    pub focused: Option<WindowId>,
    pub granted: bool,
    pub settings_page_opens: usize,
    pub access_requests: usize,
    pub waiting_for_grant: bool,
    pub registered: Vec<(u32, Combo)>,
    /// How many times shortcuts were registered.
    pub registrations: usize,
    /// Chords some other app owns.
    pub taken_elsewhere: BTreeSet<Combo>,
    pub press_monitoring: bool,
    pub drag_monitoring: bool,
    pub overlay: Vec<OverlayCall>,
    /// The windows each display outlines now; displays without any are absent.
    pub marks: BTreeMap<String, Vec<Rect>>,
    /// How many times marks were sent.
    pub mark_calls: usize,
    pub tray: TrayMenu,
    pub on_managed_desktop: bool,
    pub login_item: bool,
    /// Changing the login item fails with this message.
    pub login_item_error: Option<String>,
    pub self_activations: usize,
    pub time: f64,
    /// Window writes and bounds-change brackets, in order.
    pub writes: Vec<(WindowId, &'static str)>,
    /// What `end_bounds_change` reports the app needs before its frame is final.
    pub restore_delay: Duration,
    /// How many times Snappy looked for windows filling zones.
    pub occupancy_queries: usize,
    pub pointer: Point,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self {
            platform: PlatformName::MacOs,
            parking: ParkingPolicy::CornerSliver,
            needs_permission: true,
            live_control: LiveControl::Available,
            notes: Vec::new(),
            remedies: Vec::new(),
            drag_modifier: None,
            displays: vec![Self::main_display()],
            windows: BTreeMap::new(),
            stacking: Vec::new(),
            focused: None,
            granted: true,
            settings_page_opens: 0,
            access_requests: 0,
            waiting_for_grant: false,
            registered: Vec::new(),
            registrations: 0,
            taken_elsewhere: BTreeSet::new(),
            press_monitoring: false,
            drag_monitoring: false,
            overlay: Vec::new(),
            marks: BTreeMap::new(),
            mark_calls: 0,
            tray: TrayMenu::default(),
            on_managed_desktop: true,
            login_item: false,
            login_item_error: None,
            self_activations: 0,
            time: 0.0,
            writes: Vec::new(),
            restore_delay: Duration::ZERO,
            occupancy_queries: 0,
            pointer: Point::new(720.0, 450.0),
        }
    }
}

impl FakePlatform {
    pub fn main_display() -> Display {
        Display {
            id: "main".into(),
            name: "Built-in Display".into(),
            frame: Rect::new(0.0, 0.0, 1440.0, 900.0),
            work_area: Rect::new(0.0, 25.0, 1440.0, 875.0),
            scale: 2.0,
        }
    }

    /// Add a window on top of the others and focus it.
    pub fn open(&mut self, id: u64, window: FakeWindow) -> WindowId {
        let id = WindowId(id);
        self.windows.insert(id, window);
        self.stacking.insert(0, id);
        self.focused = Some(id);
        id
    }

    pub fn frame_of(&self, id: WindowId) -> Rect {
        self.windows[&id].frame
    }

    pub fn last_overlay(&self) -> Option<&OverlayCall> {
        self.overlay.last()
    }

    fn window(&self, id: WindowId) -> Result<&FakeWindow, WindowError> {
        self.windows
            .get(&id)
            .ok_or_else(|| WindowError::unavailable("The window is no longer available."))
    }

    fn window_mut(&mut self, id: WindowId) -> Result<&mut FakeWindow, WindowError> {
        self.windows
            .get_mut(&id)
            .ok_or_else(|| WindowError::unavailable("The window is no longer available."))
    }

    fn info(&self, id: WindowId) -> Option<WindowInfo> {
        self.windows.get(&id).map(|window| WindowInfo {
            id,
            app: window.app.clone(),
            pid: window.pid,
            frame: window.frame,
            recovery_key: Some(RecoveryKey {
                pid: window.pid,
                launch_time: 1.0,
                window: id.0.to_string(),
            }),
        })
    }

    fn eligible(&self, id: WindowId) -> Result<WindowInfo, WindowError> {
        let window = self.window(id)?;
        if window.state.full_screen {
            return Err(WindowError::unavailable(
                "Leave full screen before tiling this window.",
            ));
        }
        self.info(id)
            .ok_or_else(|| WindowError::unavailable("The window is no longer available."))
    }

    /// Move a window onto or off the parking desktop, unless it ignores every write.
    fn set_parking_desktop(&mut self, window: WindowId, on: bool) -> Result<(), WindowError> {
        self.writes.push((window, "desktop"));
        let target = self.window_mut(window)?;
        if !target.frozen {
            target.state.on_parking_desktop = on;
        }
        Ok(())
    }

    fn fully_on_a_display(&self, frame: Rect) -> bool {
        self.displays
            .iter()
            .any(|display| display.frame.contains_rect(frame))
    }
}

impl WindowSystem for FakePlatform {
    fn displays(&mut self) -> Vec<Display> {
        self.displays.clone()
    }

    fn window_at(&mut self, point: Point) -> Result<WindowInfo, WindowError> {
        let id = self
            .stacking
            .iter()
            .copied()
            .find(|id| {
                self.windows
                    .get(id)
                    .is_some_and(|w| w.is_shown() && w.frame.contains(point))
            })
            .ok_or_else(|| WindowError::unavailable("No window at the pointer."))?;
        self.eligible(id)
    }

    fn focused_window(&mut self) -> Result<WindowInfo, WindowError> {
        let id = self
            .focused
            .ok_or_else(|| WindowError::unavailable("No movable window is focused."))?;
        self.eligible(id)
    }

    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        Ok(self.window(window)?.frame)
    }

    fn set_size(&mut self, window: WindowId, size: Size) -> Result<(), WindowError> {
        self.writes.push((window, "size"));
        let frame = self.window(window)?.frame;
        let on_display = self.fully_on_a_display(frame);
        let target = self.window_mut(window)?;
        if target.frozen || (target.strict_resize && !on_display) {
            return Ok(());
        }
        let accepted = Size::new(
            size.width.max(target.min_size.width),
            size.height.max(target.min_size.height),
        );
        target.frame = Rect::from_origin_size(target.frame.origin(), accepted);
        Ok(())
    }

    fn set_position(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.writes.push((window, "position"));
        let target = self.window_mut(window)?;
        if !target.frozen {
            target.frame = target.frame.with_origin(origin);
        }
        Ok(())
    }

    fn state(&mut self, window: WindowId) -> Result<WindowState, WindowError> {
        Ok(self.window(window)?.state)
    }

    fn is_closed(&mut self, window: WindowId) -> bool {
        !self.windows.contains_key(&window)
    }

    fn visible_windows(&mut self, app: Option<&str>) -> Vec<WindowInfo> {
        self.windows
            .iter()
            .filter(|(_, w)| w.is_shown() && !w.state.minimized && app.is_none_or(|app| w.app == app))
            .filter(|(_, w)| !w.state.full_screen)
            .filter_map(|(id, _)| self.info(*id))
            .collect()
    }

    fn all_windows(&mut self) -> Vec<WindowInfo> {
        self.windows.keys().filter_map(|id| self.info(*id)).collect()
    }

    fn windows_occupying(&mut self, frames: &[Rect]) -> Result<Vec<WindowInfo>, WindowError> {
        self.occupancy_queries += 1;
        let mut found = Vec::new();
        for (id, window) in &self.windows {
            let fills = frames
                .iter()
                .any(|frame| snappy_core::swap::fills_zone(window.frame, *frame));
            if fills && window.is_shown() {
                found.extend(self.info(*id));
            }
        }
        Ok(found)
    }

    fn focus(&mut self, window: WindowId) {
        self.focused = Some(window);
    }

    fn begin_bounds_change(&mut self, window: WindowId) {
        self.writes.push((window, "beginBoundsChange"));
    }

    fn end_bounds_change(&mut self, window: WindowId) -> Duration {
        self.writes.push((window, "endBoundsChange"));
        self.restore_delay
    }

    fn hide_for_workspace(&mut self, window: WindowId, parked: Point) -> Result<(), WindowError> {
        if self.parking.moves_windows() {
            return self.set_position(window, parked);
        }
        self.set_parking_desktop(window, true)
    }

    fn show_for_workspace(&mut self, window: WindowId, origin: Point) -> Result<(), WindowError> {
        self.set_position(window, origin)?;
        if self.parking.moves_windows() {
            return Ok(());
        }
        self.set_parking_desktop(window, false)
    }

    fn activate_self(&mut self) {
        self.self_activations += 1;
        self.focused = None;
    }

    fn pointer(&mut self) -> Point {
        self.pointer
    }
}

impl Input for FakePlatform {
    fn set_press_monitoring(&mut self, on: bool) {
        self.press_monitoring = on;
    }

    fn set_drag_monitoring(&mut self, on: bool) {
        self.drag_monitoring = on;
    }

    fn set_drag_modifier(&mut self, modifier: Modifier) {
        self.drag_modifier = Some(modifier);
    }
}

impl HotKeys for FakePlatform {
    fn register(&mut self, keys: &[(u32, Combo)]) -> Vec<u32> {
        self.registrations += 1;
        let (refused, accepted): (Vec<_>, Vec<_>) = keys
            .iter()
            .cloned()
            .partition(|(_, combo)| self.taken_elsewhere.contains(combo));
        self.registered = accepted;
        refused.into_iter().map(|(id, _)| id).collect()
    }

    fn unregister_all(&mut self) {
        self.registered.clear();
    }

    fn is_available(&mut self, combo: &Combo) -> bool {
        !self.taken_elsewhere.contains(combo)
    }
}

impl Overlay for FakePlatform {
    fn prepare(&mut self, display: &str, _scene: &OverlayScene) {
        self.overlay.push(OverlayCall::Prepare(display.to_owned()));
    }

    fn show(&mut self, display: &str, scene: &OverlayScene) {
        self.overlay
            .push(OverlayCall::Show(display.to_owned(), scene.clone()));
    }

    fn hide(&mut self) {
        if self.overlay.last() != Some(&OverlayCall::Hide) {
            self.overlay.push(OverlayCall::Hide);
        }
    }

    fn retain(&mut self, displays: &[String]) {
        self.marks.retain(|display, _| displays.contains(display));
    }

    fn mark(&mut self, display: &str, marks: &OverlayMarks) {
        self.mark_calls += 1;
        if marks.frames.is_empty() {
            self.marks.remove(display);
        } else {
            self.marks.insert(display.to_owned(), marks.frames.clone());
        }
    }
}

impl Permissions for FakePlatform {
    fn is_granted(&mut self) -> bool {
        self.granted
    }

    fn request_access(&mut self) -> bool {
        self.access_requests += 1;
        true
    }

    fn open_settings_page(&mut self) -> bool {
        self.settings_page_opens += 1;
        true
    }

    fn watch(&mut self, waiting_for_grant: bool) {
        self.waiting_for_grant = waiting_for_grant;
    }
}

impl DesktopGuard for FakePlatform {
    fn bind(&mut self, _displays: &[Display]) {}

    fn on_managed_desktop(&mut self) -> bool {
        self.on_managed_desktop
    }

    fn clear(&mut self) {}
}

impl Tray for FakePlatform {
    fn update(&mut self, menu: &TrayMenu) {
        self.tray = menu.clone();
    }
}

impl LoginItem for FakePlatform {
    fn is_enabled(&mut self) -> bool {
        self.login_item
    }

    fn set_enabled(&mut self, on: bool) -> Result<(), String> {
        if let Some(error) = &self.login_item_error {
            return Err(error.clone());
        }
        self.login_item = on;
        Ok(())
    }
}

impl Platform for FakePlatform {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: self.platform,
            parking: self.parking,
            needs_permission: self.needs_permission,
        }
    }

    fn live_control(&mut self) -> LiveControl {
        self.live_control.clone()
    }

    fn apply_remedy(&mut self, remedy: Remedy) -> Result<(), String> {
        self.remedies.push(remedy);
        Ok(())
    }

    fn notes(&mut self) -> Vec<String> {
        self.notes.clone()
    }

    fn now(&self) -> f64 {
        self.time
    }
}
