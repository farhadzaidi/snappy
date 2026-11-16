//! The engine's state and the behaviors that don't belong to a single feature.

use std::collections::BTreeMap;
use std::path::Path;

use snappy_core::bindings::{HotKey, desired_hotkeys};
use snappy_core::drag::DragIntent;
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Platform as PlatformName;
use snappy_core::layout::{Layout, Spacing, zone_frame};
use snappy_core::settings::Settings;
use snappy_core::window::WindowSnapshot;
use snappy_ipc::{AppState, SnappingState, Status, ToSettings, WorkspacesState};
use snappy_platform::{
    Capabilities, Caption, Display, LiveControl, OverlayScene, OverlayZone, Platform, TrayDisplay, TrayItem,
    TrayMenu, WindowId,
};

use crate::actions::action_record;
use crate::caps_lock::{self, CapsLockController};
use crate::debug_log::{DebugLog, Level};
use crate::diagnostics::Diagnostics;
use crate::focus::FocusFlash;
use crate::marks::Marks;
use crate::selection::{self, Selection};
use crate::store::{Paths, ReadState, SettingsStore, StateFile, read_state};
use crate::workspaces::WorkspaceController;

/// What the engine is doing with the current primary-button press.
pub(crate) enum DragSession {
    Idle,
    Tracking {
        window: WindowId,
        app: String,
        intent: DragIntent,
        pointer: Point,
        /// The drag modifier was down at some point, so the drop is diagnosed.
        modifier_held: bool,
        /// The caption last shown, so the debug log records only changes to it.
        caption: Option<Caption>,
    },
}

/// A move-between-zones destination still settling.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PendingNavigation {
    pub window: WindowId,
    pub layout: String,
    pub display: String,
    pub zone: String,
}

pub(crate) struct Core<P> {
    pub platform: P,
    pub capabilities: Capabilities,
    /// What the platform last said about live control.
    pub live_control: LiveControl,
    /// What the platform last said about the desktop.
    pub platform_notes: Vec<String>,
    pub paths: Paths,
    pub store: SettingsStore,
    pub settings: Settings,
    pub displays: Vec<Display>,
    /// Whether the platform's permission is granted, and whether live control runs; both
    /// follow from the settings and the platform, so only `refresh_live` changes them.
    permission_granted: bool,
    live: bool,
    pub drag: DragSession,
    pub op_in_flight: bool,
    /// Windows an in-flight swap or undo may write to.
    pub op_windows: Vec<WindowId>,
    /// Windows a new gesture has claimed; recovery leaves them alone.
    pub protected: Vec<WindowId>,
    pub pending_navigation: Option<PendingNavigation>,
    /// The display and layout the settings app is previewing.
    preview: Option<(String, String)>,
    pub recording: bool,
    /// The shortcuts registered, by ID, and whether the platform refused any; only
    /// `configure_hotkeys` and `release_hotkeys` change them.
    registered: Vec<HotKey>,
    registration_failed: bool,
    pub workspaces: WorkspaceController,
    pub selection: Selection,
    /// The settings app's process, so its window can be found.
    pub settings_app_pid: Option<u32>,
    /// The latest message, which only the `announce` family sets.
    status: Option<Status>,
    status_counter: u64,
    pub outbox: Vec<ToSettings>,
    pub diagnostics: Diagnostics,
    pub debug: DebugLog,
    pub added_workspace: Option<String>,
    pub focus_flash: FocusFlash,
    pub marks: Marks,
    pub caps_lock: CapsLockController,
}

impl<P: Platform> Core<P> {
    pub fn new(mut platform: P, paths: Paths) -> Self {
        let capabilities = platform.capabilities();
        let live_control = platform.live_control();
        let platform_notes = platform.notes();
        let store = SettingsStore::new(paths.settings.clone(), capabilities.platform);
        let settings = Settings::defaults(capabilities.platform);
        let diagnostics = Diagnostics::new(paths.log_dir.clone());
        let debug = DebugLog::new(paths.log_dir.clone());
        Self {
            platform,
            capabilities,
            live_control,
            platform_notes,
            paths,
            store,
            settings,
            displays: Vec::new(),
            permission_granted: false,
            live: false,
            drag: DragSession::Idle,
            op_in_flight: false,
            op_windows: Vec::new(),
            protected: Vec::new(),
            pending_navigation: None,
            preview: None,
            recording: false,
            registered: Vec::new(),
            registration_failed: false,
            workspaces: WorkspaceController::default(),
            selection: Selection::default(),
            settings_app_pid: None,
            status: None,
            status_counter: 0,
            outbox: Vec::new(),
            diagnostics,
            debug,
            added_workspace: None,
            focus_flash: FocusFlash::default(),
            marks: Marks::default(),
            caps_lock: CapsLockController::default(),
        }
    }

    pub fn platform_name(&self) -> PlatformName {
        self.capabilities.platform
    }

    pub fn display_ids(&self) -> Vec<String> {
        self.displays.iter().map(|display| display.id.clone()).collect()
    }

    pub fn display_frames(&self) -> Vec<Rect> {
        self.displays.iter().map(|display| display.frame).collect()
    }

    pub fn work_areas(&self) -> Vec<Rect> {
        self.displays.iter().map(|display| display.work_area).collect()
    }

    /// The display under `point`, by full frame.
    pub fn display_at(&self, point: Point) -> Option<&Display> {
        self.displays.iter().find(|display| display.frame.contains(point))
    }

    /// The display under the pointer, else the first, for shortcuts that act on a display
    /// rather than a window.
    pub fn display_under_pointer(&mut self) -> Option<&Display> {
        let pointer = self.platform.pointer();
        self.display_at(pointer).or_else(|| self.displays.first())
    }

    /// Whether `window` is on screen: on the active workspace, not minimized, and not on
    /// the parking desktop.
    pub fn is_on_screen(&mut self, window: WindowId) -> bool {
        !self.workspaces.is_inactive(window)
            && self
                .platform
                .state(window)
                .is_ok_and(|state| !state.minimized && !state.on_parking_desktop)
    }

    /// The windows filling any of `frames` that count as occupants: those of workspaces
    /// that aren't showing never do.
    pub fn occupants(&mut self, frames: &[Rect]) -> Result<Vec<WindowSnapshot>, WindowError> {
        let found = self.platform.windows_occupying(frames)?;
        Ok(found
            .iter()
            .filter(|window| !self.workspaces.is_inactive(window.id))
            .map(|window| WindowSnapshot {
                id: window.id,
                frame: window.frame,
            })
            .collect())
    }

    /// The display a window overlaps most.
    pub fn display_of(&self, frame: Rect) -> Option<&Display> {
        snappy_core::geometry::largest_overlap(frame, &self.display_frames())
            .map(|index| &self.displays[index])
    }

    /// The gap and padding for a display, in its window units: points on macOS, physical
    /// pixels elsewhere.
    pub fn spacing(&self, display: &Display) -> Spacing {
        match self.capabilities.platform {
            PlatformName::MacOs => Spacing::STANDARD,
            PlatformName::Windows | PlatformName::Linux => Spacing::STANDARD.scaled(display.scale),
        }
    }

    pub fn zone_frames(&self, layout: &Layout, display: &Display) -> Vec<Rect> {
        let spacing = self.spacing(display);
        layout
            .zones
            .iter()
            .map(|zone| zone_frame(zone.rect, display.work_area, spacing))
            .collect()
    }

    // MARK: Live control

    pub fn is_live(&self) -> bool {
        self.live
    }

    pub fn has_permission(&self) -> bool {
        self.permission_granted
    }

    /// Recompute whether live control runs, and reconfigure what depends on it.
    pub fn refresh_live(&mut self, announce: bool) {
        self.permission_granted = !self.capabilities.needs_permission || self.platform.is_granted();
        let should_run =
            self.settings.snapping.enabled && self.permission_granted && self.live_control.is_available();
        let waiting = self.settings.snapping.enabled && !self.permission_granted;
        self.platform.watch(waiting);

        if should_run != self.live {
            self.live = should_run;
            let (enabled, granted) = (self.settings.snapping.enabled, self.permission_granted);
            self.trace("snapping", || {
                serde_json::json!({ "live": should_run, "enabled": enabled, "permissionGranted": granted })
            });
            if !should_run {
                self.end_drag();
                // Its shortcuts go with live control, so nothing could clear its outlines.
                selection::clear(self, "live control off");
            }
            if announce {
                let reason = self.live_control.reason().map(str::to_owned);
                match (should_run, reason) {
                    (true, _) => self.announce("Snapping is on."),
                    // The settings window already shows the reason where it stays.
                    (false, Some(reason)) => self.announce_quietly(&reason),
                    (false, None) => self.announce("Accessibility access is off. Snapping is paused."),
                }
            }
        }
        self.platform
            .set_drag_modifier(self.settings.snapping.drag_modifier.modifier());
        self.platform.set_press_monitoring(self.live);
        self.configure_hotkeys();
        caps_lock::configure(self);
        self.prepare_overlays();
        self.update_tray();
    }

    /// Register the shortcuts the current state needs.
    pub fn configure_hotkeys(&mut self) {
        let allowed = self.permission_granted && !self.recording && self.live_control.is_available();
        let wanted = if allowed {
            desired_hotkeys(&self.settings, &self.display_ids(), self.live)
        } else {
            Vec::new()
        };
        if wanted == self.registered && !self.registration_failed {
            return;
        }
        let keys: Vec<(u32, _)> = wanted
            .iter()
            .enumerate()
            .map(|(i, hotkey)| (i as u32, hotkey.combo.clone()))
            .collect();
        let refused = if keys.is_empty() {
            self.platform.unregister_all();
            Vec::new()
        } else {
            self.platform.register(&keys)
        };
        self.registration_failed = !refused.is_empty();
        let platform = self.platform_name();
        self.trace("shortcutsRegistered", || {
            let shortcuts: Vec<serde_json::Value> = wanted
                .iter()
                .map(|hotkey| {
                    let mut record = action_record(&hotkey.action);
                    record.insert("chord".to_owned(), hotkey.chord.long_label(platform).into());
                    serde_json::Value::Object(record)
                })
                .collect();
            serde_json::json!({ "shortcuts": shortcuts })
        });
        if self.registration_failed {
            let labels: Vec<String> = refused
                .iter()
                .map(|id| wanted[*id as usize].chord.long_label(self.platform_name()))
                .collect();
            let all = labels.clone();
            self.trace("shortcutsTaken", || serde_json::json!({ "shortcuts": all }));
            self.announce_error(&taken_message(&labels));
        }
        self.registered = wanted;
    }

    /// The registered shortcut with this ID.
    pub fn hotkey(&self, id: u32) -> Option<&HotKey> {
        self.registered.get(id as usize)
    }

    /// Whether the platform refused any shortcut the last time they were registered.
    pub fn shortcuts_refused(&self) -> bool {
        self.registration_failed
    }

    /// Unregister every shortcut, as asking the OS about chords Snappy holds needs. The
    /// next `configure_hotkeys` registers them again.
    pub fn release_hotkeys(&mut self) {
        self.platform.unregister_all();
        self.registered.clear();
    }

    // MARK: Overlay

    pub fn prepare_overlays(&mut self) {
        if !self.live {
            return;
        }
        let ids = self.display_ids();
        self.platform.retain(&ids);
        for display in self.displays.clone() {
            let layout = self.settings.layout_for_display(&display.id).clone();
            let scene = self.scene(&layout, &display, None, None, None);
            self.platform.prepare(&display.id, &scene);
        }
    }

    /// What a display's overlay shows: `selected` highlighted with `caption` under its
    /// number, and the zone `swap_source` outlined.
    pub fn scene(
        &self,
        layout: &Layout,
        display: &Display,
        selected: Option<&str>,
        caption: Option<Caption>,
        swap_source: Option<&str>,
    ) -> OverlayScene {
        let frames = self.zone_frames(layout, display);
        let zones = layout
            .zones
            .iter()
            .zip(frames)
            .enumerate()
            .map(|(index, (zone, frame))| {
                let highlighted = selected == Some(zone.id.as_str());
                OverlayZone {
                    frame,
                    number: index + 1,
                    highlighted,
                    caption: caption.filter(|_| highlighted),
                    swap_source: !highlighted && swap_source == Some(zone.id.as_str()),
                }
            })
            .collect();
        OverlayScene {
            work_area: display.work_area,
            zones,
        }
    }

    /// Show the browsed layout on a display, or hide the preview.
    pub fn set_preview(&mut self, preview: Option<(String, String)>) {
        if self.preview == preview {
            return;
        }
        self.preview = preview;
        match &self.preview {
            Some((display_id, layout_id)) => {
                let display = self.displays.iter().find(|d| &d.id == display_id).cloned();
                let layout = self.settings.layout(layout_id).cloned();
                if let (Some(display), Some(layout)) = (display, layout) {
                    let scene = self.scene(&layout, &display, None, None, None);
                    self.platform.show(&display.id, &scene);
                }
            }
            None => {
                self.platform.hide();
                self.outbox.push(ToSettings::PreviewHidden);
            }
        }
        self.publish_state();
    }

    /// Forget the current press; the overlay disappears.
    pub fn end_drag(&mut self) {
        self.drag = DragSession::Idle;
        if self.preview.is_none() {
            self.platform.hide();
        }
        self.platform.set_drag_monitoring(false);
    }

    // MARK: Menu bar

    pub fn update_tray(&mut self) {
        let workspaces = &self.settings.workspaces;
        let running = self.workspaces.is_running();
        let tooltip = match workspaces.active() {
            Some(active) if running => format!("Snappy · {}", active.name),
            _ => "Snappy — window zones".to_owned(),
        };
        let displays = self
            .displays
            .iter()
            .map(|display| {
                let current = self.settings.layout_for_display(&display.id).id.clone();
                let item = |layout: &Layout| TrayItem {
                    id: layout.id.clone(),
                    title: layout.name.clone(),
                    checked: layout.id == current,
                    enabled: self.settings.is_enabled(layout),
                };
                TrayDisplay {
                    id: display.id.clone(),
                    name: display.name.clone(),
                    presets: snappy_core::layout::presets().iter().map(item).collect(),
                    custom: self.settings.layouts.custom.iter().map(item).collect(),
                }
            })
            .collect();
        let items = workspaces
            .items
            .iter()
            .map(|item| TrayItem {
                id: item.id.clone(),
                title: item.name.clone(),
                checked: item.id == workspaces.active_id,
                enabled: running,
            })
            .collect();
        let menu = TrayMenu {
            live: self.live,
            tooltip,
            displays,
            workspaces: items,
            workspaces_running: running,
        };
        self.platform.update(&menu);
    }

    // MARK: Status and the settings app

    /// Add a record to the debug log, building it only when the log is on.
    pub fn trace(&mut self, event: &str, details: impl FnOnce() -> serde_json::Value) {
        self.log(Level::Info, event, details);
    }

    /// Like `trace`, for a result that differs from what was asked for.
    pub fn flag(&mut self, event: &str, details: impl FnOnce() -> serde_json::Value) {
        self.log(Level::Warn, event, details);
    }

    /// Read a state file, logging one that had to be moved aside.
    pub fn read_state<T: StateFile>(&mut self, path: &Path) -> ReadState<T> {
        let read = read_state(path);
        if let ReadState::MovedAside(aside) = &read {
            let name = |path: &Path| path.file_name().map(|name| name.to_string_lossy().into_owned());
            let (file, moved_to) = (name(path), aside.as_deref().and_then(name));
            self.flag(
                "stateFileUnreadable",
                || serde_json::json!({ "file": file, "movedTo": moved_to }),
            );
        }
        read
    }

    pub fn log(&mut self, level: Level, event: &str, details: impl FnOnce() -> serde_json::Value) {
        if self.debug.is_on() {
            let now = self.platform.now();
            self.debug.record(now, level, event, details());
        }
    }

    /// Report a message the settings app shows.
    pub fn announce(&mut self, message: &str) {
        self.post(message, false, true);
    }

    /// Report a routine confirmation the settings app doesn't show; it clears a visible
    /// error.
    pub fn announce_quietly(&mut self, message: &str) {
        self.post(message, false, false);
    }

    pub fn announce_error(&mut self, message: &str) {
        self.post(message, true, true);
    }

    pub fn status(&self) -> Option<&Status> {
        self.status.as_ref()
    }

    pub fn dismiss_status(&mut self) {
        self.status = None;
    }

    fn post(&mut self, message: &str, is_error: bool, notify: bool) {
        let details = || serde_json::json!({ "message": message });
        if is_error {
            self.flag("status", details);
        } else {
            self.trace("status", details);
        }
        self.status_counter += 1;
        self.status = Some(Status {
            id: self.status_counter,
            message: message.to_owned(),
            is_error,
            notify,
        });
        self.publish_state();
    }

    pub fn publish_state(&mut self) {
        if self.settings_app_pid.is_none() {
            return;
        }
        let state = self.app_state();
        self.outbox
            .retain(|message| !matches!(message, ToSettings::State(_)));
        self.outbox.push(ToSettings::State(Box::new(state)));
    }

    pub fn app_state(&mut self) -> AppState {
        let window_counts: BTreeMap<String, usize> = self
            .settings
            .workspaces
            .items
            .iter()
            .map(|item| (item.id.clone(), self.workspaces.window_count(&item.id)))
            .collect();
        AppState {
            settings: self.settings.clone(),
            displays: self.displays.clone(),
            platform: self.capabilities.platform,
            snapping: SnappingState {
                live: self.live,
                waiting_for_permission: self.settings.snapping.enabled && !self.permission_granted,
                permission_granted: self.permission_granted,
                needs_permission: self.capabilities.needs_permission,
                preview_visible: self.preview.is_some(),
                unavailable_reason: self.live_control.reason().map(str::to_owned),
                unavailable_remedy: self.live_control.remedy(),
            },
            platform_notes: self.platform_notes.clone(),
            workspaces: WorkspacesState {
                running: self.workspaces.is_running(),
                pause_message: self.workspaces.pause_message(self.capabilities.platform),
                window_counts,
            },
            login_item_enabled: self.platform.is_enabled(),
            debug_log_path: self.debug.path().display().to_string(),
            status: self.status.clone(),
            added_workspace: self.added_workspace.take(),
            caps_lock_problem: self.caps_lock.problem().map(str::to_owned),
        }
    }
}

/// Most chords a message about taken shortcuts names; past that it counts them, and the
/// debug log has the list.
const TAKEN_LISTED: usize = 3;

/// What the user is told when another app holds some of Snappy's shortcuts, short enough
/// to read at a glance.
fn taken_message(labels: &[String]) -> String {
    match labels {
        [one] => format!("Another app is using {one}."),
        [first, second] => format!("Another app is using {first} and {second}."),
        [rest @ .., last] if labels.len() <= TAKEN_LISTED => {
            format!("Another app is using {}, and {last}.", rest.join(", "))
        }
        _ => format!("Another app is using {} of Snappy's shortcuts.", labels.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taken_shortcuts_are_named_when_few_and_counted_when_many() {
        let labels = |count: usize| -> Vec<String> { (1..=count).map(|n| format!("Snappy + {n}")).collect() };
        assert_eq!(taken_message(&labels(1)), "Another app is using Snappy + 1.");
        assert_eq!(
            taken_message(&labels(2)),
            "Another app is using Snappy + 1 and Snappy + 2."
        );
        assert_eq!(
            taken_message(&labels(3)),
            "Another app is using Snappy + 1, Snappy + 2, and Snappy + 3."
        );
        assert_eq!(
            taken_message(&labels(10)),
            "Another app is using 10 of Snappy's shortcuts."
        );
    }
}
