//! Running workspaces against real windows: discovery, switching and sending, crash
//! recovery, containment, and reacting to desktop and display changes.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use snappy_core::bounds::fitted_origin;
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Platform as PlatformName;
use snappy_core::workspace::{
    FocusPolicy, Found, JournalEntry, Journaled, Member, PARKED_TOLERANCE, Relaunch, Session, Tabs,
    WorkspaceIo, is_parked, reachable, relaunch,
};
use snappy_ipc::ToSettings;
use snappy_platform::{Platform, RecoveryKey, WindowId, WindowInfo, WindowState};

use crate::core::Core;
use crate::diagnostics::SwitchTiming;
use crate::ops::Shared;
use crate::selection;
use crate::store::{read_state, remove_state, write_state};

/// Wait this long for a display change to settle.
const DISPLAY_SETTLE: Duration = Duration::from_millis(350);
/// Retry a failed display update once after this long.
const DISPLAY_RETRY: Duration = Duration::from_millis(750);
/// Re-park a revealed window at most this often.
const CONTAINMENT_INTERVAL: f64 = 1.0;
/// A recovered window this far from its parked spot was moved by the user.
const USER_MOVED: f64 = 4.0;

/// One window's saved workspace state on disk, in the journal and membership files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SavedWindow {
    pub key: RecoveryKey,
    pub workspace_id: String,
    pub frame: Rect,
    pub parked_origin: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior_parked_origin: Option<Point>,
}

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
struct CoreIo<'a, P> {
    core: &'a mut Core<P>,
}

impl<'a, P: Platform> CoreIo<'a, P> {
    fn new(core: &'a mut Core<P>) -> Self {
        Self { core }
    }
}

impl<P: Platform> WorkspaceIo for CoreIo<'_, P> {
    fn frame(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        self.core.platform.frame(window)
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

// MARK: Starting and stopping

/// Start or stop workspaces to match settings and permission.
pub(crate) fn configure<P: Platform>(core: &mut Core<P>) {
    let wanted =
        core.settings.workspaces.enabled && core.permission_granted && core.live_control.is_available();
    let lost_control =
        core.settings.workspaces.enabled && core.permission_granted && !core.live_control.is_available();
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
    let journal = session.journal();
    if let Err(error) = CoreIo::new(&mut *core).write_journal(&journal) {
        core.announce_error(&error.to_string());
    }

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

/// A window a previous run left parked for a workspace that isn't showing joins it again,
/// keeping its home frame, and stays where it is.
fn rejoin<P: Platform>(core: &mut Core<P>, session: &mut Session, info: &WindowInfo, entry: &SavedWindow) {
    let Some(key) = info.recovery_key.clone() else {
        return;
    };
    let displays = core.display_frames();
    let policy = core.capabilities.parking;
    let parked_now = core
        .platform
        .state(info.id)
        .is_ok_and(|state| is_parked(policy, info.frame, &displays, state.on_parking_desktop));
    // A home frame that is itself a parking spot would strand the window when it returns.
    let home_origin = if is_parked(policy, entry.frame, &displays, false) {
        let area = core.display_of(info.frame).map(|display| display.work_area);
        fitted_origin(entry.frame.origin(), info.frame.size(), area)
    } else {
        entry.frame.origin()
    };
    let member = Member {
        workspace: entry.workspace_id.clone(),
        frame: Rect::from_origin_size(home_origin, info.frame.size()),
        parked: if parked_now {
            Some(info.frame.origin())
        } else {
            entry.parked_origin.or(Some(info.frame.origin()))
        },
    };
    core.trace("workspaceJoin", || {
        serde_json::json!({
            "window": info.id.0,
            "app": info.app,
            "workspace": member.workspace,
            "frame": member.frame,
            "rejoined": true,
        })
    });
    core.workspaces.keys.insert(info.id, key);
    session.enroll(info.id, member);
}

/// Stop for a restart: every window stays where it is, parked ones included, and the journal
/// and membership are saved for the next launch to pick up.
pub(crate) fn detach<P: Platform>(core: &mut Core<P>) {
    selection::clear(core, "restarting");
    let Some(mut session) = core.workspaces.session.take() else {
        return;
    };
    let closed: Vec<WindowId> = session
        .members()
        .keys()
        .copied()
        .filter(|window| core.platform.is_closed(*window))
        .collect();
    for window in closed {
        session.discard(window);
    }
    sync_all_tabs(core, &mut session);
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
    let closed: Vec<WindowId> = session
        .members()
        .keys()
        .copied()
        .filter(|window| core.platform.is_closed(*window))
        .collect();
    for window in closed {
        session.discard(window);
    }
    sync_all_tabs(core, &mut session);
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

/// Add a window to the session: back to its remembered workspace, or the active one.
fn enroll<P: Platform>(core: &mut Core<P>, session: &mut Session, info: &WindowInfo) {
    if session.member(info.id).is_some() {
        return;
    }
    let skipped = |core: &mut Core<P>, reason: &str| {
        core.trace(
            "workspaceJoinSkipped",
            || serde_json::json!({ "window": info.id.0, "app": info.app, "reason": reason }),
        );
    };
    let Some(key) = info.recovery_key.clone() else {
        return skipped(core, "no recovery key");
    };
    let state = match core.platform.state(info.id) {
        Ok(state) if state.movable() => state,
        Ok(_) => return skipped(core, "full screen or showing a sheet"),
        Err(error) => return skipped(core, &error.to_string()),
    };
    let minimized = state.minimized;
    let displays = core.display_frames();
    let policy = core.capabilities.parking;
    let parked_now = is_parked(policy, info.frame, &displays, state.on_parking_desktop);

    // A visible window joins the active workspace. A parked or minimized one goes back to
    // the workspace remembered for it.
    let remembered = remembered_entry(core, info);
    let workspaces = &core.settings.workspaces;
    let owned = remembered
        .as_ref()
        .filter(|_| parked_now || minimized)
        .map(|entry| entry.workspace_id.clone())
        .filter(|id| workspaces.item(id).is_some());
    // A shown tab with no workspace of its own, such as a new tab, goes where its group is.
    if owned.is_none()
        && let Some(group) = core
            .platform
            .tab_groups(&[info.id])
            .into_iter()
            .find(|group| group.shown.id == info.id)
        && join_tab_group(core, session, info, &group.hidden)
    {
        return;
    }
    let owner = owned.unwrap_or_else(|| core.settings.workspaces.active_id.clone());
    // A window found parked keeps the visible position remembered for it. One with none
    // comes back just inside the screen rather than staying stranded.
    let visible_origin = match (parked_now, &remembered) {
        (false, _) => info.frame.origin(),
        (true, Some(entry)) if !is_parked(policy, entry.frame, &displays, false) => entry.frame.origin(),
        (true, _) => {
            let area = core.display_of(info.frame).map(|display| display.work_area);
            let rescued = fitted_origin(info.frame.origin(), info.frame.size(), area);
            core.flag("workspaceRescued", || {
                serde_json::json!({ "window": info.id.0, "app": info.app, "from": info.frame, "to": rescued })
            });
            rescued
        }
    };
    let visible_frame = Rect::from_origin_size(visible_origin, info.frame.size());

    core.workspaces.keys.insert(info.id, key);
    let active = session.active().to_owned();
    core.trace("workspaceJoin", || {
        serde_json::json!({ "window": info.id.0, "app": info.app, "workspace": owner, "frame": visible_frame })
    });
    // Already parked for an inactive workspace: join it there, keeping its home frame, and
    // journal it so a crash can still bring it back.
    if parked_now && owner != active {
        session.enroll(
            info.id,
            Member {
                workspace: owner,
                frame: visible_frame,
                parked: Some(info.frame.origin()),
            },
        );
        let journal = session.journal();
        if let Err(error) = CoreIo::new(&mut *core).write_journal(&journal) {
            core.announce_error(&error.to_string());
        }
        return;
    }
    session.enroll(
        info.id,
        Member {
            workspace: active.clone(),
            frame: visible_frame,
            parked: None,
        },
    );
    if parked_now && owner == active {
        let _ = core.platform.show_for_workspace(info.id, visible_origin);
    }
    if owner != active {
        let mut io = CoreIo::new(&mut *core);
        if let Err(error) = session.send(info.id, &owner, &displays, &mut io) {
            core.announce_error(&error.to_string());
        }
    }
}

/// A shown tab Snappy hasn't met, such as a new tab, joins the tabs hidden behind it where
/// they are: in their workspace, with their frame and parking. False when none of them is a
/// member.
fn join_tab_group<P: Platform>(
    core: &mut Core<P>,
    session: &mut Session,
    info: &WindowInfo,
    hidden: &[WindowId],
) -> bool {
    let sibling = session
        .recently_focused()
        .find(|window| hidden.contains(window))
        .or_else(|| {
            hidden
                .iter()
                .copied()
                .find(|window| session.member(*window).is_some())
        });
    let (Some(sibling), Some(key)) = (sibling, info.recovery_key.clone()) else {
        return false;
    };
    let Some(mut member) = session.member(sibling).cloned() else {
        return false;
    };
    if member.parked.is_none() {
        member.frame = info.frame;
    }
    core.workspaces.keys.insert(info.id, key);
    let tabs: Vec<u64> = hidden.iter().map(|window| window.0).collect();
    core.trace("workspaceJoin", || {
        serde_json::json!({
            "window": info.id.0,
            "app": info.app,
            "workspace": member.workspace,
            "frame": member.frame,
            "tabOf": tabs,
        })
    });
    session.enroll(info.id, member);
    let mut refreshed = hidden.to_vec();
    refreshed.push(info.id);
    let group = Tabs {
        shown: info.id,
        hidden: hidden.to_vec(),
    };
    session.update_tabs(&[group], &refreshed);
    let journal = session.journal();
    if let Err(error) = CoreIo::new(&mut *core).write_journal(&journal) {
        core.announce_error(&error.to_string());
    }
    true
}

/// Learn the tab groups of every member's app.
fn sync_all_tabs<P: Platform>(core: &mut Core<P>, session: &mut Session) {
    let members: Vec<WindowId> = session.members().keys().copied().collect();
    sync_tabs(core, session, &members);
}

/// Learn from the platform which members of the apps owning `windows` are tabs of one
/// window, and keep each group in its shown tab's workspace. A shown tab Snappy hasn't met
/// joins the tabs behind it.
fn sync_tabs<P: Platform>(core: &mut Core<P>, session: &mut Session, windows: &[WindowId]) {
    let groups = core.platform.tab_groups(windows);
    for group in &groups {
        if session.member(group.shown.id).is_none() {
            join_tab_group(core, session, &group.shown, &group.hidden);
        }
    }
    let pids: BTreeSet<u32> = windows
        .iter()
        .filter_map(|window| core.workspaces.keys.get(window).map(|key| key.pid))
        .chain(groups.iter().map(|group| group.shown.pid))
        .collect();
    let refreshed: Vec<WindowId> = session
        .members()
        .keys()
        .copied()
        .filter(|window| {
            core.workspaces
                .keys
                .get(window)
                .is_some_and(|key| pids.contains(&key.pid))
        })
        .collect();
    let tabs: Vec<Tabs> = groups
        .iter()
        .map(|group| Tabs {
            shown: group.shown.id,
            hidden: group.hidden.clone(),
        })
        .collect();
    let moved = session.update_tabs(&tabs, &refreshed);
    if moved.is_empty() {
        return;
    }
    let unified: Vec<serde_json::Value> = tabs
        .iter()
        .filter(|group| group.hidden.iter().any(|window| moved.contains(window)))
        .map(|group| {
            let workspace = session.member(group.shown).map(|member| member.workspace.clone());
            let hidden: Vec<u64> = group.hidden.iter().map(|window| window.0).collect();
            serde_json::json!({ "shown": group.shown.0, "hidden": hidden, "workspace": workspace })
        })
        .collect();
    core.trace("tabsUnified", || serde_json::json!({ "groups": unified }));
    let journal = session.journal();
    if let Err(error) = CoreIo::new(&mut *core).write_journal(&journal) {
        core.announce_error(&error.to_string());
    }
    save_membership(core, session);
}

fn remembered_entry<P: Platform>(core: &Core<P>, info: &WindowInfo) -> Option<SavedWindow> {
    let key = info.recovery_key.as_ref()?;
    core.workspaces
        .remembered
        .iter()
        .find(|entry| entry.key.same_window(key))
        .cloned()
}

// MARK: Recovery

/// What launch recovery did with the windows a previous run left parked.
#[derive(Default)]
struct Recovered {
    /// No window that should have come back is still parked.
    complete: bool,
    /// Windows still parked for a workspace that isn't showing, which stay that way.
    kept: Vec<(WindowInfo, SavedWindow)>,
    returned: usize,
    /// Moved away from their parked spot while Snappy wasn't running.
    left_alone: usize,
    /// Their process is gone, or its ID now belongs to a later process.
    stale: usize,
}

/// Go through the windows a previous run left parked. With `keep`, windows still parked
/// for a workspace that isn't showing stay parked, journaled, to rejoin it; otherwise every
/// one comes back.
fn recover<P: Platform>(core: &mut Core<P>, keep: bool) -> Recovered {
    let path = core.paths.journal();
    let Some(entries): Option<Vec<SavedWindow>> = read_state(&path) else {
        return Recovered {
            complete: !path.exists(),
            ..Recovered::default()
        };
    };
    let windows = core.platform.all_windows();
    let (displays, areas) = (core.display_frames(), core.work_areas());
    let policy = core.capabilities.parking;
    let active = keep.then(|| core.settings.workspaces.active_id.clone());
    let mut recovered = Recovered::default();
    let mut remaining = Vec::new();

    for entry in entries {
        let Some(parked) = entry.parked_origin else {
            continue;
        };
        let exact = windows.iter().find(|w| {
            w.recovery_key
                .as_ref()
                .is_some_and(|key| key.same_window(&entry.key))
        });
        let by_position: Vec<&WindowInfo> = windows
            .iter()
            .filter(|w| {
                w.recovery_key
                    .as_ref()
                    .is_some_and(|key| key.same_process(&entry.key))
            })
            .filter(|w| {
                let size_matches = (w.frame.width - entry.frame.width).abs() < PARKED_TOLERANCE
                    && (w.frame.height - entry.frame.height).abs() < PARKED_TOLERANCE;
                size_matches
                    && [Some(parked), entry.prior_parked_origin]
                        .into_iter()
                        .flatten()
                        .any(|p| near(w.frame, p))
            })
            .collect();
        let found = exact.or(if by_position.len() == 1 {
            Some(by_position[0])
        } else {
            None
        });

        let Some(window) = found else {
            let still_running = windows.iter().any(|w| {
                w.recovery_key
                    .as_ref()
                    .is_some_and(|key| key.same_process(&entry.key))
            });
            if still_running || !by_position.is_empty() {
                let process: Vec<&RecoveryKey> = windows
                    .iter()
                    .filter_map(|w| w.recovery_key.as_ref())
                    .filter(|key| key.pid == entry.key.pid)
                    .collect();
                core.flag("workspaceWindowNotFound", || {
                    serde_json::json!({ "key": entry.key, "sameProcess": process, "atParkedSpot": by_position.len() })
                });
                remaining.push(entry);
            } else {
                recovered.stale += 1;
            }
            continue;
        };
        let state = core.platform.state(window.id).ok();
        let journaled = Journaled {
            workspace: &entry.workspace_id,
            workspace_exists: core.settings.workspaces.item(&entry.workspace_id).is_some(),
            parked,
            prior_parked: entry.prior_parked_origin,
        };
        let now = Found {
            frame: window.frame,
            minimized: state.is_some_and(|state| state.minimized),
            on_parking_desktop: state.is_some_and(|state| state.on_parking_desktop),
            movable: state.is_some_and(WindowState::movable),
        };
        match relaunch(policy, &displays, &journaled, &now, active.as_deref()) {
            Relaunch::StaysParked => recovered.kept.push((window.clone(), entry)),
            Relaunch::LeftAlone => recovered.left_alone += 1,
            Relaunch::ComesBack => {
                let home = reachable(entry.frame, &areas).origin();
                let returned = core.platform.show_for_workspace(window.id, home).is_ok()
                    && core
                        .platform
                        .frame(window.id)
                        .is_ok_and(|frame| near(frame, home))
                    && !core
                        .platform
                        .state(window.id)
                        .is_ok_and(|state| state.on_parking_desktop);
                if returned {
                    recovered.returned += 1;
                } else {
                    remaining.push(entry);
                }
            }
        }
    }

    // Kept windows stay journaled until the session takes them over, so a crash before then
    // still finds them.
    let still_parked: Vec<SavedWindow> = remaining
        .iter()
        .cloned()
        .chain(recovered.kept.iter().map(|(_, entry)| entry.clone()))
        .collect();
    let saved = if still_parked.is_empty() {
        remove_state(&path)
    } else {
        write_state(&path, &still_parked)
    };
    recovered.complete = saved.is_ok() && remaining.is_empty();
    recovered
}

fn near(frame: Rect, origin: Point) -> bool {
    (frame.x - origin.x).abs() < USER_MOVED && (frame.y - origin.y).abs() < USER_MOVED
}

fn save_membership<P: Platform>(core: &mut Core<P>, session: &Session) {
    let tracked: Vec<SavedWindow> = session
        .members()
        .iter()
        .filter_map(|(window, member)| {
            core.workspaces.keys.get(window).map(|key| SavedWindow {
                key: key.clone(),
                workspace_id: member.workspace.clone(),
                frame: member.frame,
                parked_origin: member.parked,
                prior_parked_origin: None,
            })
        })
        .collect();
    let running: Vec<RecoveryKey> = core
        .platform
        .all_windows()
        .into_iter()
        .filter_map(|info| info.recovery_key)
        .collect();
    let workspaces = &core.settings.workspaces;
    let kept = core.workspaces.remembered.iter().filter(|entry| {
        !tracked.iter().any(|t| t.key == entry.key)
            && workspaces.item(&entry.workspace_id).is_some()
            && running.iter().any(|key| key.same_process(&entry.key))
    });
    let entries: Vec<SavedWindow> = tracked.iter().cloned().chain(kept.cloned()).collect();
    if let Err(error) = write_state(&core.paths.membership(), &entries) {
        core.announce_error(&format!("Could not save workspace membership: {error}"));
    }
    core.workspaces.remembered = entries;
}

// MARK: Switching and sending

/// Switch to `target`. `focus_settings` gives focus to the settings window afterwards
/// instead of the workspace's own last window.
pub(crate) fn request_switch<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    target: &str,
    focus_settings: bool,
) {
    {
        let mut core = shared.core.borrow_mut();
        let current = core
            .workspaces
            .requested
            .clone()
            .unwrap_or_else(|| core.settings.workspaces.active_id.clone());
        let refused = if core.workspaces.session.is_none() {
            Some("workspaces aren't running")
        } else if core.workspaces.paused {
            Some("paused on another desktop")
        } else if core.workspaces.display_update_pending {
            core.workspaces.deferred_switch = Some((target.to_owned(), focus_settings));
            Some("waiting for displays to settle")
        } else if core.settings.workspaces.item(target).is_none() {
            Some("no such workspace")
        } else if current == target {
            Some("already there")
        } else {
            None
        };
        if let Some(reason) = refused {
            core.trace(
                "workspaceSwitchSkipped",
                || serde_json::json!({ "to": target, "reason": reason }),
            );
            return;
        }
        core.workspaces.requested = Some(target.to_owned());
    }
    let target = target.to_owned();
    let requested_at = shared.core.borrow().platform.now();
    shared.submit(move |shared, _cancel| async move {
        let mut core = shared.core.borrow_mut();
        switch_now(&mut core, &target, focus_settings, requested_at);
        core.workspaces.requested = None;
    });
}

fn switch_now<P: Platform>(core: &mut Core<P>, target: &str, focus_settings: bool, requested_at: f64) {
    let Some(mut session) = core.workspaces.session.take() else {
        return;
    };
    let began = core.platform.now();
    core.workspaces.focus.begin_change();
    discover_focused_into(core, &mut session, false, false);
    let closed: Vec<WindowId> = session
        .members()
        .keys()
        .copied()
        .filter(|window| core.platform.is_closed(*window))
        .collect();
    for window in closed {
        session.discard(window);
    }
    sync_all_tabs(core, &mut session);

    let discovered = core.platform.now();
    let displays = core.display_frames();
    let previous = session.active().to_owned();
    let carried = core.selection.windows.clone();
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.switch_carrying(target, &carried, &displays, !focus_settings, &mut io)
    };
    let arrived: Vec<WindowId> = carried
        .iter()
        .copied()
        .filter(|window| {
            result.is_ok()
                && session
                    .member(*window)
                    .is_some_and(|member| member.workspace == target)
        })
        .collect();
    let moved = core.platform.now();
    let mut timing = SwitchTiming {
        queue_ms: (began - requested_at) * 1000.0,
        discovery_ms: (discovered - began) * 1000.0,
        window_ms: (moved - discovered) * 1000.0,
        windows: session.members().len(),
        ..SwitchTiming::default()
    };

    match result {
        Ok(focused_any) => {
            if focus_settings {
                core.outbox.push(ToSettings::Focus);
            } else if !focused_any {
                core.platform.activate_self();
            }
            save_membership(core, &session);
            let saved = core.platform.now();
            timing.membership_ms = (saved - moved) * 1000.0;
            if commit_active(core, target) {
                timing.settings_ms = (core.platform.now() - saved) * 1000.0;
                let refresh = core.platform.now();
                core.configure_hotkeys();
                core.prepare_overlays();
                timing.refresh_ms = (core.platform.now() - refresh) * 1000.0;
                timing.succeeded = true;
                if !carried.is_empty() {
                    let windows: Vec<u64> = arrived.iter().map(|window| window.0).collect();
                    let tabs = tabs_along(&session, &arrived);
                    core.trace("selectionCarried", || {
                        serde_json::json!({ "to": target, "windows": windows, "selected": carried.len(), "tabs": tabs })
                    });
                    selection::clear(core, "carried along");
                }
            } else {
                let mut io = CoreIo::new(&mut *core);
                let _ = session.switch_carrying(&previous, &arrived, &displays, true, &mut io);
            }
        }
        Err(error) => {
            core.flag(
                "workspaceSwitch",
                || serde_json::json!({ "from": previous, "to": target, "error": error.to_string() }),
            );
            core.announce_error(&error.to_string());
        }
    }

    timing.total_ms = (core.platform.now() - requested_at) * 1000.0;
    let windows = timing.windows;
    let total_ms = timing.total_ms;
    core.trace(
        "workspaceSwitch",
        || serde_json::json!({ "from": previous, "to": target, "windows": windows, "totalMs": total_ms }),
    );
    core.diagnostics.record_switch(timing);
    core.workspaces.session = Some(session);
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);
    core.update_tray();
    core.publish_state();
}

/// The hidden tabs that went along with `windows`, by the shown tab they sit behind, for
/// the debug log.
fn tabs_along(session: &Session, windows: &[WindowId]) -> serde_json::Value {
    let mut roots: Vec<WindowId> = Vec::new();
    for window in windows {
        let root = session.tab_root(*window);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    let groups: serde_json::Map<String, serde_json::Value> = roots
        .into_iter()
        .filter_map(|root| {
            let hidden: Vec<u64> = session.tabs_behind(root).iter().map(|window| window.0).collect();
            (!hidden.is_empty()).then(|| (root.0.to_string(), serde_json::json!(hidden)))
        })
        .collect();
    serde_json::Value::Object(groups)
}

/// Save only the new active workspace.
fn commit_active<P: Platform>(core: &mut Core<P>, target: &str) -> bool {
    let mut next = core.settings.clone();
    next.workspaces.active_id = target.to_owned();
    match core.store.save(&next) {
        Ok(()) => {
            core.settings = next;
            true
        }
        Err(error) => {
            core.announce_error(&format!("Could not save the workspace selection: {error}"));
            false
        }
    }
}

/// Previous or next workspace in saved order.
pub(crate) fn cycle<P: Platform + 'static>(shared: &Rc<Shared<P>>, step: isize) {
    if let Some(target) = adjacent(shared, step) {
        request_switch(shared, &target, false);
    }
}

/// Send the focused window to the previous or next workspace in saved order, wrapping like
/// `cycle`, and stay.
pub(crate) fn send_and_stay<P: Platform + 'static>(shared: &Rc<Shared<P>>, step: isize) {
    if let Some(target) = adjacent(shared, step) {
        send_focused(shared, &target, false);
    }
}

/// The workspace `step` away from the one being switched to, or the active one.
fn adjacent<P: Platform>(shared: &Shared<P>, step: isize) -> Option<String> {
    let core = shared.core.borrow();
    let from = core
        .workspaces
        .requested
        .clone()
        .unwrap_or_else(|| core.settings.workspaces.active_id.clone());
    core.settings
        .workspaces
        .adjacent(&from, step)
        .map(|item| item.id.clone())
}

/// Send the focused window to `target`, then switch there when `follow`. Without `follow`,
/// a selection is sent instead of the focused window, and focus moves to the window used
/// most recently among those left, so pressing again sends that one.
pub(crate) fn send_focused<P: Platform + 'static>(shared: &Rc<Shared<P>>, target: &str, follow: bool) {
    if !shared.core.borrow().workspaces.is_running() {
        return;
    }
    let target = target.to_owned();
    shared.submit(move |shared, _cancel| async move {
        let sent = {
            let mut core = shared.core.borrow_mut();
            selection::prune(&mut core);
            if follow || core.selection.is_empty() {
                send_now(&mut core, &target, !follow).into_iter().collect()
            } else {
                send_selection_now(&mut core, &target)
            }
        };
        for window in &sent {
            for history in shared.histories.borrow_mut().values_mut() {
                history.forget(*window);
            }
        }
        if follow && !sent.is_empty() {
            let mut core = shared.core.borrow_mut();
            let now = core.platform.now();
            switch_now(&mut core, &target, false, now);
        }
    });
}

fn send_now<P: Platform>(core: &mut Core<P>, target: &str, stay: bool) -> Option<WindowId> {
    let focused = match core.platform.focused_window() {
        Ok(info) => info,
        Err(_) => {
            core.announce_error("Focus a window to move it to a workspace.");
            return None;
        }
    };
    let mut session = core.workspaces.session.take()?;
    enroll(core, &mut session, &focused);
    sync_tabs(core, &mut session, &[focused.id]);
    let in_active = session
        .member(focused.id)
        .is_some_and(|member| member.workspace == session.active());
    if !in_active {
        core.workspaces.session = Some(session);
        core.announce_error("Focus a window on the current workspace first.");
        return None;
    }

    core.workspaces.focus.begin_change();
    let displays = core.display_frames();
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.send(focused.id, target, &displays, &mut io)
    };
    let left_focused = match (&result, stay) {
        (Ok(()), true) => session.focus_recent(&mut CoreIo::new(&mut *core)),
        _ => None,
    };
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);
    let sent = match result {
        Ok(()) => {
            save_membership(core, &session);
            let tabs = tabs_along(&session, &[focused.id]);
            core.trace("workspaceSend", || {
                serde_json::json!({
                    "window": focused.id.0,
                    "app": focused.app,
                    "to": target,
                    "stay": stay,
                    "focused": left_focused.map(|window| window.0),
                    "tabs": tabs,
                })
            });
            Some(focused.id)
        }
        Err(error) => {
            core.announce_error(&error.to_string());
            None
        }
    };
    core.workspaces.session = Some(session);
    if sent.is_some() {
        selection::remove(core, focused.id, "sent to another workspace");
    }
    core.publish_state();
    sent
}

/// Send every selected window on the active workspace to `target` and stay, focusing the
/// window used most recently among those left. Returns the windows sent; they leave the
/// selection, and any that couldn't go stay in it.
fn send_selection_now<P: Platform>(core: &mut Core<P>, target: &str) -> Vec<WindowId> {
    let Some(mut session) = core.workspaces.session.take() else {
        return Vec::new();
    };
    let chosen = core.selection.windows.clone();
    sync_tabs(core, &mut session, &chosen);
    // A selected tab stands for its group, which goes once.
    let mut selected: Vec<WindowId> = Vec::new();
    for window in chosen {
        let root = session.tab_root(window);
        let here = session
            .member(root)
            .is_some_and(|member| member.workspace == session.active());
        if here && !selected.contains(&root) {
            selected.push(root);
        }
    }

    core.workspaces.focus.begin_change();
    let displays = core.display_frames();
    let mut sent = Vec::new();
    let mut failure = None;
    for window in selected {
        let result = {
            let mut io = CoreIo::new(&mut *core);
            session.send(window, target, &displays, &mut io)
        };
        match result {
            Ok(()) => sent.push(window),
            Err(error) => failure = failure.or(Some(error)),
        }
    }
    let left_focused = if sent.is_empty() {
        None
    } else {
        session.focus_recent(&mut CoreIo::new(&mut *core))
    };
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);
    if !sent.is_empty() {
        save_membership(core, &session);
        let windows: Vec<u64> = sent.iter().map(|window| window.0).collect();
        let tabs = tabs_along(&session, &sent);
        core.trace("workspaceSend", || {
            serde_json::json!({
                "windows": windows,
                "to": target,
                "stay": true,
                "focused": left_focused.map(|window| window.0),
                "tabs": tabs,
            })
        });
    }
    if let Some(error) = failure {
        core.announce_error(&error.to_string());
    }
    let gone: Vec<WindowId> = core
        .selection
        .windows
        .iter()
        .copied()
        .filter(|window| sent.contains(&session.tab_root(*window)))
        .collect();
    core.workspaces.session = Some(session);
    for window in &gone {
        selection::remove(core, *window, "sent to another workspace");
    }
    core.publish_state();
    sent
}

/// Whether `info` shows on the active workspace, enrolling it first, so it can be
/// selected. Always true while workspaces don't run.
pub(crate) fn shown_on_active_workspace<P: Platform>(core: &mut Core<P>, info: &WindowInfo) -> bool {
    if !core.workspaces.is_running() {
        return true;
    }
    let Some(mut session) = core.workspaces.session.take() else {
        return true;
    };
    enroll(core, &mut session, info);
    let shown = session
        .member(info.id)
        .is_some_and(|member| member.workspace == session.active() && member.parked.is_none());
    core.workspaces.session = Some(session);
    shown
}

/// Delete a workspace, moving its windows to `destination`.
pub(crate) fn remove_workspace<P: Platform>(
    core: &mut Core<P>,
    id: &str,
    destination: &str,
) -> Result<(), WindowError> {
    let Some(mut session) = core.workspaces.session.take() else {
        return Ok(());
    };
    core.workspaces.focus.begin_change();
    sync_all_tabs(core, &mut session);
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.remove_workspace(id, destination, &mut io)
    };
    if result.is_ok() {
        save_membership(core, &session);
    }
    core.workspaces.session = Some(session);
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);
    result
}

// MARK: Discovery and focus

/// React to the frontmost app or its focused window changing.
pub(crate) fn focus_changed<P: Platform + 'static>(shared: &Rc<Shared<P>>, by_activation: bool) {
    let follow = {
        let mut core = shared.core.borrow_mut();
        if !core.workspaces.is_running() {
            return;
        }
        discover_focused(&mut core, by_activation, true)
    };
    if let Some(target) = follow {
        request_switch(shared, &target, false);
    }
}

/// Enroll the focused app's windows. Returns a workspace to switch to when an activation
/// revealed a window of another workspace.
fn discover_focused<P: Platform>(core: &mut Core<P>, by_activation: bool, siblings: bool) -> Option<String> {
    let mut session = core.workspaces.session.take()?;
    let follow = discover_focused_into(core, &mut session, by_activation, siblings);
    core.workspaces.session = Some(session);
    follow
}

fn discover_focused_into<P: Platform>(
    core: &mut Core<P>,
    session: &mut Session,
    by_activation: bool,
    siblings: bool,
) -> Option<String> {
    if core.workspaces.paused {
        return None;
    }
    let focused = match core.platform.focused_window() {
        Ok(info) => Some(info),
        Err(error) => {
            core.trace(
                "focusedWindowUnknown",
                || serde_json::json!({ "reason": error.to_string() }),
            );
            None
        }
    };
    // Tabs change as the user picks them, so learn the focused app's groups first: a tab
    // shown for the first time joins its group, and follows like a member when that group
    // is on another workspace.
    let known = focused
        .as_ref()
        .is_some_and(|info| session.member(info.id).is_some());
    if let Some(info) = &focused {
        sync_tabs(core, session, &[info.id]);
    }
    if let Some(info) = &focused
        && let Some(member) = session.member(info.id)
        && (known || member.workspace != session.active())
    {
        if member.workspace == session.active() {
            session.note_focus(info.id);
            return None;
        }
        let now = core.platform.now();
        return core
            .workspaces
            .focus
            .follow(&member.workspace, session.active(), by_activation, now)
            .map(str::to_owned);
    }

    if siblings && let Some(info) = &focused {
        for sibling in core.platform.visible_windows(Some(&info.app)) {
            enroll(core, session, &sibling);
        }
    }
    if let Some(info) = focused {
        enroll(core, session, &info);
        session.note_focus(info.id);
    }
    None
}

/// Park a parked window again when its app brought it back.
pub(crate) fn window_moved<P: Platform>(core: &mut Core<P>, window: WindowId) {
    let now = core.platform.now();
    let recently = core
        .workspaces
        .contained_at
        .get(&window)
        .is_some_and(|at| now - at < CONTAINMENT_INTERVAL);
    if !core.workspaces.is_running() || core.workspaces.focus.is_changing() || recently {
        return;
    }
    if !core.platform.on_managed_desktop() {
        return;
    }
    let Some(mut session) = core.workspaces.session.take() else {
        return;
    };
    let displays = core.display_frames();
    core.workspaces.contained_at.insert(window, now);
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.contain(window, &displays, &mut io)
    };
    let active = session.active().to_owned();
    let count = session.members().len();
    core.workspaces.session = Some(session);
    match result {
        Ok(true) => {
            core.diagnostics
                .record_lifecycle(now, "containedRevealedWindow", false, Some(active), count)
        }
        Ok(false) => {}
        Err(error) => core.announce_error(&error.to_string()),
    }
}

pub(crate) fn window_destroyed<P: Platform>(core: &mut Core<P>, window: WindowId) {
    if let Some(session) = core.workspaces.session.as_mut() {
        session.discard(window);
    }
    if let Some(key) = core.workspaces.keys.remove(&window) {
        core.workspaces.remembered.retain(|entry| entry.key != key);
    }
    core.workspaces.contained_at.remove(&window);
    core.publish_state();
}

pub(crate) fn app_terminated<P: Platform>(core: &mut Core<P>, pid: u32) {
    let gone: Vec<WindowId> = core
        .workspaces
        .keys
        .iter()
        .filter(|(_, key)| key.pid == pid)
        .map(|(window, _)| *window)
        .collect();
    for window in gone {
        window_destroyed(core, window);
    }
    core.workspaces.remembered.retain(|entry| entry.key.pid != pid);
}

// MARK: Desktops and displays

pub(crate) fn native_desktop_changed<P: Platform>(core: &mut Core<P>) {
    if core.workspaces.session.is_none() {
        return;
    }
    if core.workspaces.suspended {
        return;
    }
    let was_paused = core.workspaces.paused;
    core.workspaces.paused = !core.platform.on_managed_desktop();
    let paused = core.workspaces.paused;
    core.trace("nativeDesktop", || serde_json::json!({ "paused": paused }));
    if was_paused
        && !core.workspaces.paused
        && let Err(error) = update_displays(core)
    {
        core.announce_error(&error.to_string());
    }
    let now = core.platform.now();
    let active = core.workspaces.session.as_ref().map(|s| s.active().to_owned());
    let count = core.workspaces.session.as_ref().map_or(0, |s| s.members().len());
    core.diagnostics
        .record_lifecycle(now, "nativeDesktopChanged", core.workspaces.paused, active, count);
    core.update_tray();
    core.publish_state();
}

/// Wait for the display change to settle, then update in place with one retry.
pub(crate) fn displays_changed<P: Platform + 'static>(shared: &Rc<Shared<P>>) {
    shared.cancel_current();
    let generation = {
        let mut core = shared.core.borrow_mut();
        core.end_drag();
        core.set_preview(None);
        core.workspaces.display_generation += 1;
        core.workspaces.display_update_pending = true;
        core.workspaces.display_generation
    };
    let shared = Rc::clone(shared);
    tokio::task::spawn_local(async move {
        tokio::time::sleep(DISPLAY_SETTLE).await;
        if !apply_display_change(&shared, generation, false) {
            tokio::time::sleep(DISPLAY_RETRY).await;
            apply_display_change(&shared, generation, true);
        }
        let deferred = {
            let mut core = shared.core.borrow_mut();
            if core.workspaces.display_update_pending {
                None
            } else {
                core.workspaces.deferred_switch.take()
            }
        };
        if let Some((target, focus_settings)) = deferred {
            request_switch(&shared, &target, focus_settings);
        }
    });
}

/// Returns true when finished, successfully or not.
fn apply_display_change<P: Platform>(shared: &Shared<P>, generation: u64, last_try: bool) -> bool {
    let mut core = shared.core.borrow_mut();
    if core.workspaces.display_generation != generation {
        return true;
    }
    core.displays = core.platform.displays();
    match update_displays(&mut core) {
        Ok(()) => {}
        Err(_) if !last_try => return false,
        Err(error) => core.announce_error(&format!("Display changed; workspace tracking was kept. {error}")),
    }
    core.workspaces.display_update_pending = false;
    core.configure_hotkeys();
    core.prepare_overlays();
    core.refresh_marks();
    core.update_tray();
    core.publish_state();
    true
}

fn update_displays<P: Platform>(core: &mut Core<P>) -> Result<(), WindowError> {
    if core.workspaces.paused {
        return Ok(());
    }
    let Some(mut session) = core.workspaces.session.take() else {
        return Ok(());
    };
    core.workspaces.focus.begin_change();
    sync_all_tabs(core, &mut session);
    let (displays, areas) = (core.display_frames(), core.work_areas());
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.update_displays(&displays, &areas, &mut io)
    };
    if result.is_ok() {
        let bound = core.displays.clone();
        core.platform.bind(&bound);
        save_membership(core, &session);
    }
    core.workspaces.session = Some(session);
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);
    result
}
