//! Running workspaces against real windows: starting and stopping them, and the window
//! access a session moves windows through. The rest is split by what drives it: bringing
//! windows into the session (`members`), switching and sending (`switching`), what the
//! platform reports (`events`), and what a restart keeps (`recovery`).

mod events;
mod members;
mod recovery;
mod switching;

use std::collections::BTreeMap;

use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Platform as PlatformName;
use snappy_core::workspace::{FocusPolicy, JournalEntry, Session, WorkspaceIo};
use snappy_platform::{Platform, RecoveryKey, WindowId};

use crate::core::Core;
use crate::selection;
use crate::store::{read_state, remove_state, write_state};

use events::discover_focused;
use members::{enroll, sync_all_tabs};
use recovery::{SavedWindow, recover, rejoin, remembered_entry, save_membership};

pub(crate) use events::{
    app_terminated, displays_changed, focus_changed, native_desktop_changed, window_destroyed, window_moved,
};
pub(crate) use switching::{
    cycle, remove_workspace, request_switch, send_and_stay, send_focused, shown_on_active_workspace,
};

#[derive(Default)]
pub(crate) struct WorkspaceController {
    pub session: Option<Session>,
    /// On another native desktop.
    pub paused: bool,
    /// Live control went away; nothing moves until it returns (`resume`).
    pub suspended: bool,
    pub focus: FocusPolicy,
    pub keys: BTreeMap<WindowId, RecoveryKey>,
    pub remembered: Vec<SavedWindow>,
    pub contained_at: BTreeMap<WindowId, f64>,
    pub display_update_pending: bool,
    display_generation: u64,
    /// A switch on its way; previous/next count from here.
    pub requested: Option<String>,
    /// A switch asked for while displays were changing, run once they settle.
    pub deferred_switch: Option<(String, bool)>,
}

impl WorkspaceController {
    pub fn is_running(&self) -> bool {
        self.session.is_some() && !self.paused && !self.suspended
    }

    pub fn window_count(&self, workspace: &str) -> usize {
        self.session
            .as_ref()
            .map_or(0, |session| session.window_count(workspace))
    }

    pub fn is_inactive(&self, window: WindowId) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.is_inactive(window))
    }

    /// The shown tab `window` sits behind, as last learned, else `window` itself.
    pub fn tab_root(&self, window: WindowId) -> WindowId {
        self.session
            .as_ref()
            .map_or(window, |session| session.tab_root(window))
    }

    /// The windows that stand for `windows`' tab groups, each once (`Session::tab_roots`).
    pub fn tab_roots(&self, windows: &[WindowId]) -> Vec<WindowId> {
        match &self.session {
            Some(session) => session.tab_roots(windows),
            None => windows.to_vec(),
        }
    }

    pub fn is_hidden_tab(&self, window: WindowId) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.is_hidden_tab(window))
    }

    pub fn pause_message(&self, platform: PlatformName) -> Option<String> {
        self.paused
            .then_some(match platform {
                PlatformName::MacOs => {
                    "Paused on another macOS desktop. Return to the original desktop to resume."
                }
                _ => "Paused on another virtual desktop. Return to the original desktop to resume.",
            })
            .map(str::to_owned)
    }
}

/// Window access for a session, with the journal mapped to recovery keys.
pub(super) struct CoreIo<'a, P> {
    core: &'a mut Core<P>,
}

impl<'a, P: Platform> CoreIo<'a, P> {
    pub(super) fn new(core: &'a mut Core<P>) -> Self {
        Self { core }
    }
}

impl<P: Platform> WorkspaceIo for CoreIo<'_, P> {
    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        self.core.platform.frame(window)
    }

    fn is_closed(&mut self, window: WindowId) -> bool {
        self.core.platform.is_closed(window)
    }

    fn is_movable(&mut self, window: WindowId) -> bool {
        !self.core.platform.is_closed(window)
            && self
                .core
                .platform
                .state(window)
                .is_ok_and(|state| state.movable())
    }

    fn is_minimized(&mut self, window: WindowId) -> bool {
        self.core
            .platform
            .state(window)
            .is_ok_and(|state| state.minimized)
    }

    fn is_on_parking_desktop(&mut self, window: WindowId) -> bool {
        self.core
            .platform
            .state(window)
            .is_ok_and(|state| state.on_parking_desktop)
    }

    fn place(&mut self, window: WindowId, origin: Point, hidden: bool) -> Result<(), WindowError> {
        if hidden {
            self.core.platform.hide_for_workspace(window, origin)
        } else {
            self.core.platform.show_for_workspace(window, origin)
        }
    }

    fn focus(&mut self, window: WindowId) {
        self.core.platform.focus(window);
    }

    fn write_journal(&mut self, entries: &[JournalEntry]) -> Result<(), WindowError> {
        let path = self.core.paths.journal();
        if entries.is_empty() {
            return remove_state(&path).map_err(|_| unavailable_journal());
        }
        let saved: Option<Vec<SavedWindow>> = entries
            .iter()
            .map(|entry| {
                self.core
                    .workspaces
                    .keys
                    .get(&entry.window)
                    .map(|key| SavedWindow {
                        key: key.clone(),
                        workspace_id: entry.workspace.clone(),
                        frame: entry.frame,
                        parked_origin: Some(entry.parked),
                        prior_parked_origin: entry.prior_parked,
                    })
            })
            .collect();
        let saved = saved.ok_or_else(unavailable_journal)?;
        write_state(&path, &saved).map_err(|_| unavailable_journal())
    }
}

fn unavailable_journal() -> WindowError {
    WindowError::unavailable("Workspace recovery data is unavailable; no windows were hidden.")
}

/// Write the session's parked windows to the journal, telling the user when it can't be.
pub(super) fn save_journal<P: Platform>(core: &mut Core<P>, session: &Session) {
    let journal = session.journal();
    if let Err(error) = CoreIo::new(&mut *core).write_journal(&journal) {
        core.announce_error(&error.to_string());
    }
}

/// Stop tracking members that closed without the platform saying so, then learn every
/// remaining member's tab groups, as every change to all of them starts.
pub(super) fn refresh_members<P: Platform>(core: &mut Core<P>, session: &mut Session) {
    let closed: Vec<WindowId> = session
        .members()
        .keys()
        .copied()
        .filter(|window| core.platform.is_closed(*window))
        .collect();
    for window in closed {
        session.discard(window);
    }
    sync_all_tabs(core, session);
}

// MARK: Starting and stopping

/// Start or stop workspaces to match settings and permission.
pub(crate) fn configure<P: Platform>(core: &mut Core<P>) {
    let wanted =
        core.settings.workspaces.enabled && core.has_permission() && core.live_control.is_available();
    let lost_control =
        core.settings.workspaces.enabled && core.has_permission() && !core.live_control.is_available();
    match (wanted, core.workspaces.session.is_some()) {
        (true, false) => start(core),
        (false, true) if lost_control => suspend(core),
        (false, true) => {
            stop(core);
        }
        (true, true) | (false, false) => {}
    }
    core.update_tray();
}

/// Live control went away. The session stays as it was and nothing is moved: the platform
/// may already have shown the hidden windows, and `resume` hides them again.
fn suspend<P: Platform>(core: &mut Core<P>) {
    if std::mem::replace(&mut core.workspaces.suspended, true) {
        return;
    }
    selection::clear(core, "workspaces suspended");
    core.trace("workspacesSuspended", || serde_json::json!({}));
    core.publish_state();
}

/// Live control is back, or the platform started over: bind to the desktop in use again, and
/// hide again any window of an inactive workspace that was shown meanwhile.
pub(crate) fn resume<P: Platform>(core: &mut Core<P>) {
    let Some(mut session) = core.workspaces.session.take() else {
        return;
    };
    core.workspaces.suspended = false;
    let displays = core.displays.clone();
    core.platform.bind(&displays);
    core.workspaces.paused = !core.platform.on_managed_desktop();

    sync_all_tabs(core, &mut session);
    let frames = core.display_frames();
    let inactive: Vec<WindowId> = session
        .members()
        .keys()
        .copied()
        .filter(|window| session.is_inactive(*window))
        .collect();
    let (hidden_again, failed) = {
        let mut io = CoreIo::new(&mut *core);
        inactive.iter().fold((0, 0), |(hidden, failed), window| {
            match session.contain(*window, &frames, &mut io) {
                Ok(true) => (hidden + 1, failed),
                Ok(false) => (hidden, failed),
                Err(_) => (hidden, failed + 1),
            }
        })
    };
    let active = session.active().to_owned();
    core.workspaces.session = Some(session);
    core.trace(
        "workspacesResumed",
        || serde_json::json!({ "active": active, "hiddenAgain": hidden_again, "failed": failed }),
    );
    if failed > 0 {
        core.announce_error("Some windows from other workspaces couldn't be hidden again.");
    }
    core.publish_state();
}

/// Start tracking. Windows a previous run left parked for a workspace that isn't showing
/// rejoin it where they are, so a restart while apps keep running changes nothing on
/// screen; every other window joins the active workspace.
fn start<P: Platform>(core: &mut Core<P>) {
    let recovered = recover(core, true);
    if !recovered.complete {
        let message = if core.capabilities.needs_permission {
            "Some hidden windows could not be restored. Check Accessibility, then quit and reopen Snappy to retry."
        } else {
            "Some hidden windows could not be restored. Quit and reopen Snappy to retry."
        };
        core.announce_error(message);
        return;
    }
    let displays = core.displays.clone();
    core.platform.bind(&displays);
    core.workspaces.paused = false;
    let remembered: Vec<SavedWindow> = read_state(&core.paths.membership()).unwrap_or_default();
    let running: Vec<RecoveryKey> = core
        .platform
        .all_windows()
        .into_iter()
        .filter_map(|info| info.recovery_key)
        .collect();
    let (live, stale): (Vec<SavedWindow>, Vec<SavedWindow>) = remembered
        .into_iter()
        .partition(|entry| running.iter().any(|key| key.same_process(&entry.key)));
    core.workspaces.remembered = live;
    let mut session = Session::new(
        core.settings.workspaces.active_id.clone(),
        core.capabilities.parking,
    );

    for (info, entry) in &recovered.kept {
        rejoin(core, &mut session, info, entry);
    }
    // Visible windows, plus minimized ones that remember a workspace, and
    // windows still waiting on the parking desktop.
    let visible = core.platform.visible_windows(None);
    let mut candidates = visible.clone();
    for info in core.platform.all_windows() {
        let hidden = !visible.iter().any(|v| v.id == info.id);
        let state = core.platform.state(info.id).unwrap_or_default();
        let remembered_minimized = state.minimized && remembered_entry(core, &info).is_some();
        if hidden && (remembered_minimized || state.on_parking_desktop) {
            candidates.push(info);
        }
    }
    for info in candidates {
        enroll(core, &mut session, &info);
    }
    sync_all_tabs(core, &mut session);
    save_journal(core, &session);

    let windows = session.members().len();
    let active = session.active().to_owned();
    let mut rejoined: BTreeMap<String, usize> = BTreeMap::new();
    for (info, _) in &recovered.kept {
        if let Some(member) = session.member(info.id) {
            *rejoined.entry(member.workspace.clone()).or_default() += 1;
        }
    }
    core.workspaces.session = Some(session);
    core.trace("workspacesStarted", || {
        serde_json::json!({
            "active": active,
            "windows": windows,
            "rejoined": rejoined,
            "returned": recovered.returned,
            "leftAlone": recovered.left_alone,
            "staleJournal": recovered.stale,
            "staleMembership": stale.len(),
        })
    });
    discover_focused(core, false, true);
    core.publish_state();
}

/// Stop for a restart: every window stays where it is, parked ones included, and the journal
/// and membership are saved for the next launch to pick up.
pub(crate) fn detach<P: Platform>(core: &mut Core<P>) {
    selection::clear(core, "restarting");
    let Some(mut session) = core.workspaces.session.take() else {
        return;
    };
    refresh_members(core, &mut session);
    let journal = {
        let mut io = CoreIo::new(&mut *core);
        session.capture_active_frames(&mut io);
        let journal = session.journal();
        io.write_journal(&journal).map(|()| journal.len())
    };
    save_membership(core, &session);
    let active = session.active().to_owned();
    let windows = session.members().len();
    match journal {
        Ok(parked) => core.trace(
            "workspacesDetached",
            || serde_json::json!({ "active": active, "windows": windows, "parked": parked }),
        ),
        Err(error) => core.flag(
            "workspacesDetached",
            || serde_json::json!({ "active": active, "windows": windows, "error": error.to_string() }),
        ),
    }
}

/// Return every parked window and stop tracking.
pub(crate) fn stop<P: Platform>(core: &mut Core<P>) -> bool {
    selection::clear(core, "workspaces stopped");
    let Some(mut session) = core.workspaces.session.take() else {
        return recover(core, false).complete;
    };
    core.workspaces.focus.begin_change();
    refresh_members(core, &mut session);
    let areas = core.work_areas();
    let returned = {
        let mut io = CoreIo::new(&mut *core);
        session.capture_active_frames(&mut io);
        session.restore_all(&areas, &mut io)
    };
    save_membership(core, &session);
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);

    if returned {
        core.workspaces.keys.clear();
        core.platform.clear();
    } else {
        core.workspaces.session = Some(session);
        core.announce_error("A window could not be restored yet. Turn workspaces off again to retry.");
    }
    core.update_tray();
    core.publish_state();
    returned
}
