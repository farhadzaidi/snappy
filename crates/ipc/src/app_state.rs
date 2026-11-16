//! What the app shows: the settings and the daemon's state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use snappy_core::keys::Os;
use snappy_core::settings::Settings;
use snappy_platform::{Display, Remedy};

/// What the app renders.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppState {
    pub settings: Settings,
    pub displays: Vec<Display>,
    /// Decides modifier names and permission controls.
    pub platform: Os,
    pub snapping: SnappingState,
    pub workspaces: WorkspacesState,
    /// Notes from the platform about the desktop, such as another tool that also snaps.
    #[serde(default)]
    pub platform_notes: Vec<String>,
    pub login_item_enabled: bool,
    /// Where the debug log is written while it's on.
    pub debug_log_path: String,
    pub status: Option<Status>,
    /// The ID of a workspace newly added by `AddWorkspace`, so the page can select it.
    pub added_workspace: Option<String>,
    /// Why Caps Lock isn't the Snappy key although the setting asks for it, such as the
    /// system remapping Caps Lock since it was turned on.
    #[serde(default)]
    pub caps_lock_problem: Option<String>,
    /// Why Caps Lock can never be the Snappy key in this build, such as the Mac App Store's.
    /// The app shows the setting as unavailable, with this under it.
    #[serde(default)]
    pub caps_lock_unavailable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnappingState {
    pub live: bool,
    pub waiting_for_permission: bool,
    pub permission_granted: bool,
    pub needs_permission: bool,
    pub preview_visible: bool,
    /// Why live control can't run at all, for example in a Wayland session.
    pub unavailable_reason: Option<String>,
    /// What Snappy can do about it when asked (`ApplyRemedy`).
    #[serde(default)]
    pub unavailable_remedy: Option<Remedy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspacesState {
    pub running: bool,
    pub pause_message: Option<String>,
    /// Workspace ID to the number of windows in it.
    pub window_counts: BTreeMap<String, usize>,
}

/// A message for the toast. `notify` false means a quiet update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub id: u64,
    pub message: String,
    pub is_error: bool,
    pub notify: bool,
}
