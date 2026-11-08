//! The engine's state and the behaviors that don't belong to a single feature.

use std::collections::BTreeMap;

use snappy_core::bindings::{HotKey, desired_hotkeys};
use snappy_core::drag::DragIntent;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Platform as PlatformName;
use snappy_core::layout::{Layout, Spacing, zone_frame};
use snappy_core::settings::Settings;
use snappy_ipc::{AppState, SnappingState, Status, ToSettings, WorkspacesState};
use snappy_platform::{
    Capabilities, Caption, Display, LiveControl, OverlayScene, OverlayZone, Platform, TrayDisplay, TrayItem,
    TrayMenu, WindowId,
};

use crate::debug_log::{DebugLog, Level};
use crate::diagnostics::Diagnostics;
use crate::store::{Paths, SettingsStore};
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
    pub permission_granted: bool,
    pub live: bool,
    pub drag: DragSession,
    pub op_in_flight: bool,
    /// Windows an in-flight swap or undo may write to.
    pub op_windows: Vec<WindowId>,
    /// Windows a new gesture has claimed; recovery leaves them alone.
    pub protected: Vec<WindowId>,
    pub pending_navigation: Option<PendingNavigation>,
    pub preview: Option<(String, String)>,
    pub recording: bool,
    pub registered: Vec<HotKey>,
    pub registration_failed: bool,
    pub workspaces: WorkspaceController,
    /// The settings app's process, so its window can be found.
    pub settings_app_pid: Option<u32>,
    pub status: Option<Status>,
    status_counter: u64,
    pub outbox: Vec<ToSettings>,
    pub diagnostics: Diagnostics,
    pub debug: DebugLog,
    pub added_workspace: Option<String>,
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
            settings_app_pid: None,
            status: None,
            status_counter: 0,
            outbox: Vec::new(),
            diagnostics,
            debug,
            added_workspace: None,
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
            }
            if announce {
                let message = match (should_run, self.live_control.reason()) {
                    (true, _) => "Snapping is on.".to_owned(),
                    (false, Some(reason)) => reason.to_owned(),
                    (false, None) => "Accessibility access is off. Snapping is paused.".to_owned(),
                };
                self.announce(&message, false, true);
            }
        }
        self.platform
            .set_drag_modifier(self.settings.snapping.drag_modifier.modifier());
        self.platform.set_press_monitoring(self.live);
        self.configure_hotkeys();
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
            .map(|(i, hotkey)| (i as u32, hotkey.chord.combo()))
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
            let shortcuts: Vec<String> = wanted
                .iter()
                .map(|hotkey| format!("{} → {:?}", hotkey.chord.long_label(platform), hotkey.action))
                .collect();
            serde_json::json!({ "shortcuts": shortcuts })
        });
        if self.registration_failed {
            let labels: Vec<String> = refused
                .iter()
                .map(|id| wanted[*id as usize].chord.long_label(self.platform_name()))
                .collect();
            self.announce(
                &format!("Shortcuts already used by another app: {}.", labels.join(", ")),
                true,
                true,
            );
        }
        self.registered = wanted;
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

    /// Report a message. Quiet messages aren't shown; they clear a visible error.
    /// Add a record to the debug log, building it only when the log is on.
    pub fn trace(&mut self, event: &str, details: impl FnOnce() -> serde_json::Value) {
        self.log(Level::Info, event, details);
    }

    /// Like `trace`, for a result that differs from what was asked for.
    pub fn flag(&mut self, event: &str, details: impl FnOnce() -> serde_json::Value) {
        self.log(Level::Warn, event, details);
    }

    pub fn log(&mut self, level: Level, event: &str, details: impl FnOnce() -> serde_json::Value) {
        if self.debug.is_on() {
            let now = self.platform.now();
            self.debug.record(now, level, event, details());
        }
    }

    pub fn announce(&mut self, message: &str, is_error: bool, notify: bool) {
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

    pub fn announce_quietly(&mut self, message: &str) {
        self.announce(message, false, false);
    }

    pub fn announce_error(&mut self, message: &str) {
        self.announce(message, true, true);
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
        let platform = match self.capabilities.platform {
            PlatformName::MacOs => "macos",
            PlatformName::Windows => "windows",
            PlatformName::Linux => "linux",
        };
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
            platform: platform.to_owned(),
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
        }
    }
}
