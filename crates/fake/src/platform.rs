//! The fake platform's state, the helpers tests script it with, and what it says it supports.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{Combo, Modifier, Mods, Os};
use snappy_core::workspace::ParkingPolicy;
use snappy_platform::{
    Capabilities, CapsLockRefusal, CapsLockUndo, Display, LiveControl, MarkedWindow, Platform, RecoveryKey,
    Remedy, TrayMenu, WindowId, WindowInfo, WindowSystem,
};

use crate::fake_window::FakeWindow;
use crate::overlay::OverlayCall;

#[derive(Debug, Clone)]
pub struct FakePlatform {
    pub platform: Os,
    pub parking: ParkingPolicy,
    pub needs_permission: bool,
    /// Why Caps Lock can never be the Snappy key, as a build without it declares.
    pub caps_lock_unavailable: Option<&'static str>,
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
    /// The modifiers Caps Lock stands for while held, while it is the Snappy key.
    pub caps_lock_held_as: Option<Mods>,
    /// Holding Caps Lock as the Snappy key is refused with this, as when the system
    /// already remaps it.
    pub caps_lock_refusal: Option<CapsLockRefusal>,
    /// What holding Caps Lock returns to undo it in a later run.
    pub caps_lock_undo: CapsLockUndo,
    /// Changes of earlier runs undone, in order.
    pub caps_lock_undone: Vec<CapsLockUndo>,
    /// How many times Caps Lock was turned on or off.
    pub caps_lock_toggles: usize,
    /// Turning Caps Lock on or off fails with this, as when the system doesn't take it.
    pub caps_lock_toggle_failure: Option<String>,
    pub press_monitoring: bool,
    pub drag_monitoring: bool,
    pub overlay: Vec<OverlayCall>,
    /// The windows each display outlines now; displays without any are absent.
    pub marks: BTreeMap<String, Vec<MarkedWindow>>,
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
    /// How many times Snappy listed every window, which is slow on some platforms.
    pub window_listings: usize,
    pub pointer: Point,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self {
            platform: Os::MacOs,
            parking: ParkingPolicy::CornerSliver,
            needs_permission: true,
            caps_lock_unavailable: None,
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
            caps_lock_held_as: None,
            caps_lock_refusal: None,
            caps_lock_undo: CapsLockUndo("fake".into()),
            caps_lock_undone: Vec::new(),
            caps_lock_toggles: 0,
            caps_lock_toggle_failure: None,
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
            window_listings: 0,
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

    /// Open a new tab in `shown`'s window, as its app does: the new tab shows, at the
    /// group's frame, with focus, and every other tab hides behind it.
    pub fn add_tab(&mut self, id: u64, shown: WindowId) -> WindowId {
        let group = self.windows[&shown].clone();
        let tab = self.open(id, FakeWindow::new(&group.app, group.pid, group.frame));
        self.hide_group_behind(shown, tab);
        tab
    }

    /// Show `tab` in its group, as clicking it does: it takes the group's frame and the
    /// shown tab's place, and the others hide behind it.
    pub fn select_tab(&mut self, tab: WindowId) {
        let Some(shown) = self.windows.get(&tab).and_then(|window| window.tab_of) else {
            return;
        };
        let frame = self.windows[&shown].frame;
        self.hide_group_behind(shown, tab);
        if let Some(window) = self.windows.get_mut(&tab) {
            window.tab_of = None;
            window.frame = frame;
        }
        self.focus(tab);
    }

    /// Hide `shown` and every tab behind it behind `tab`, which shows in their place.
    fn hide_group_behind(&mut self, shown: WindowId, tab: WindowId) {
        for window in self.windows.values_mut() {
            if window.tab_of == Some(shown) {
                window.tab_of = Some(tab);
            }
        }
        if let Some(window) = self.windows.get_mut(&shown) {
            window.tab_of = Some(tab);
        }
    }

    /// Drag `tab` out of its group into a window of its own at `frame`, focused. When it
    /// was the shown tab, the group shows its first other tab where it was.
    pub fn tear_out(&mut self, tab: WindowId, frame: Rect) {
        let behind: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, window)| window.tab_of == Some(tab))
            .map(|(id, _)| *id)
            .collect();
        if let Some(next) = behind.first().copied() {
            let group_frame = self.windows[&tab].frame;
            for other in &behind {
                if let Some(window) = self.windows.get_mut(other) {
                    window.tab_of = (*other != next).then_some(next);
                }
            }
            if let Some(window) = self.windows.get_mut(&next) {
                window.frame = group_frame;
            }
        }
        if let Some(window) = self.windows.get_mut(&tab) {
            window.tab_of = None;
            window.frame = frame;
        }
        self.focus(tab);
    }

    pub fn last_overlay(&self) -> Option<&OverlayCall> {
        self.overlay.last()
    }

    pub(crate) fn window(&self, id: WindowId) -> Result<&FakeWindow, WindowError> {
        self.windows
            .get(&id)
            .ok_or_else(|| WindowError::unavailable("The window is no longer available."))
    }

    pub(crate) fn window_mut(&mut self, id: WindowId) -> Result<&mut FakeWindow, WindowError> {
        self.windows
            .get_mut(&id)
            .ok_or_else(|| WindowError::unavailable("The window is no longer available."))
    }

    pub(crate) fn info(&self, id: WindowId) -> Option<WindowInfo> {
        self.windows.get(&id).map(|window| WindowInfo {
            id,
            app: window.app.clone(),
            pid: window.pid,
            frame: window.frame,
            recovery_key: Some(RecoveryKey {
                pid: window.pid,
                launch_time: window.launch_time,
                window: id.0.to_string(),
            }),
        })
    }

    pub(crate) fn eligible(&self, id: WindowId) -> Result<WindowInfo, WindowError> {
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
    pub(crate) fn set_parking_desktop(&mut self, window: WindowId, on: bool) -> Result<(), WindowError> {
        self.writes.push((window, "desktop"));
        let target = self.window_mut(window)?;
        if !target.frozen {
            target.state.on_parking_desktop = on;
        }
        Ok(())
    }

    pub(crate) fn fully_on_a_display(&self, frame: Rect) -> bool {
        self.displays
            .iter()
            .any(|display| display.frame.contains_rect(frame))
    }
}

impl Platform for FakePlatform {
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            platform: self.platform,
            parking: self.parking,
            needs_permission: self.needs_permission,
            caps_lock_unavailable: self.caps_lock_unavailable,
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
