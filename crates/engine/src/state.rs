//! The engine's state. Its behaviors are split by concern: what is on screen (`screen`),
//! whether live control runs (`live_control`), shortcut registration (`hotkeys`), the zone
//! overlay (`overlay`), the tray menu (`tray`), the debug log (`logging`), status messages
//! (`status`), and what the app is sent (`app_state`).

mod app_state;
mod hotkeys;
mod live_control;
mod logging;
mod overlay;
mod screen;
mod status;
mod tray;

use std::rc::Rc;

use snappy_core::keys::Os;
use snappy_core::placement::drag::DragIntent;
use snappy_core::settings::Settings;
use snappy_core::shortcuts::HotKey;
use snappy_ipc::{Status, ToApp};
use snappy_platform::{Capabilities, Caption, Display, LiveControl, Platform, WindowId};
use tokio::sync::Notify;

use crate::caps_lock::CapsLockController;
use crate::debug_log::DebugLog;
use crate::diagnostics::Diagnostics;
use crate::focus::FocusFlash;
use crate::marks::Marks;
use crate::selection::Selection;
use crate::send_to_back::SendingToBack;
use crate::store::{Paths, SettingsStore};
use crate::workspaces::WorkspaceController;

/// What the engine is doing with the current primary-button press.
pub(crate) enum DragSession {
    Idle,
    Tracking {
        window: WindowId,
        app: String,
        intent: DragIntent,
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

pub(crate) struct EngineState<P> {
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
    /// The display and layout the app is previewing.
    preview: Option<(String, String)>,
    /// The display whose overlay shows, so the debug log records when it appears and goes.
    overlay_on: Option<String>,
    pub recording: bool,
    /// The shortcuts registered, by ID, and whether the platform refused any; only
    /// `configure_hotkeys` and `release_hotkeys` change them.
    registered: Vec<HotKey>,
    registration_failed: bool,
    pub workspaces: WorkspaceController,
    pub selection: Selection,
    /// The app's process, so its window can be found.
    pub app_pid: Option<u32>,
    /// The latest message, which only the `announce` family sets.
    status: Option<Status>,
    status_counter: u64,
    /// Messages for the app not yet taken, and the signal that there are some.
    outbox: Vec<ToApp>,
    outbox_ready: Rc<Notify>,
    pub diagnostics: Diagnostics,
    pub debug: DebugLog,
    pub added_workspace: Option<String>,
    pub focus_flash: FocusFlash,
    pub marks: Marks,
    pub caps_lock: CapsLockController,
    pub sending_to_back: SendingToBack,
}

impl<P: Platform> EngineState<P> {
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
            overlay_on: None,
            recording: false,
            registered: Vec::new(),
            registration_failed: false,
            workspaces: WorkspaceController::default(),
            selection: Selection::default(),
            app_pid: None,
            status: None,
            status_counter: 0,
            outbox: Vec::new(),
            outbox_ready: Rc::new(Notify::new()),
            diagnostics,
            debug,
            added_workspace: None,
            focus_flash: FocusFlash::default(),
            marks: Marks::default(),
            caps_lock: CapsLockController::default(),
            sending_to_back: SendingToBack::default(),
        }
    }

    pub fn platform_name(&self) -> Os {
        self.capabilities.platform
    }
}
