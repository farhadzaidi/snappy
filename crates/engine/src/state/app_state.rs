//! What goes to the app: the engine's state, and every message queued for it.

use std::collections::BTreeMap;
use std::rc::Rc;

use snappy_ipc::{AppState, SnappingState, ToApp, WorkspacesState};
use snappy_platform::Platform;
use tokio::sync::Notify;

use super::EngineState;

impl<P: Platform> EngineState<P> {
    pub fn publish_state(&mut self) {
        if self.app_pid.is_none() {
            return;
        }
        let state = self.app_state();
        self.outbox.retain(|message| !matches!(message, ToApp::State(_)));
        self.send_to_app(ToApp::State(Box::new(state)));
    }

    /// Queues a message for the app and signals that one is waiting, so work that
    /// finishes between inputs reaches the app at once.
    pub fn send_to_app(&mut self, message: ToApp) {
        self.outbox.push(message);
        self.outbox_ready.notify_one();
    }

    /// The messages queued since the last call.
    pub fn take_outbox(&mut self) -> Vec<ToApp> {
        std::mem::take(&mut self.outbox)
    }

    /// Signalled whenever a message is queued.
    pub fn outbox_ready(&self) -> Rc<Notify> {
        Rc::clone(&self.outbox_ready)
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
            caps_lock_unavailable: self.capabilities.caps_lock_unavailable.map(str::to_owned),
        }
    }
}
