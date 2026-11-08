//! Running workspaces against real windows: discovery, switching and sending, crash
//! recovery, containment, and reacting to desktop and display changes.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use snappy_core::bounds::fitted_origin;
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Platform as PlatformName;
use snappy_core::workspace::{
    FocusPolicy, JournalEntry, Member, PARKED_TOLERANCE, Session, WorkspaceIo, is_parked, reachable,
};
use snappy_ipc::ToSettings;
use snappy_platform::{Platform, RecoveryKey, WindowId, WindowInfo};

use crate::core::Core;
use crate::diagnostics::SwitchTiming;
use crate::ops::Shared;
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

fn start<P: Platform>(core: &mut Core<P>) {
    if !recover(core) {
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
    core.workspaces.remembered = read_state(&core.paths.membership()).unwrap_or_default();
    let mut session = Session::new(
        core.settings.workspaces.active_id.clone(),
        core.capabilities.parking,
    );

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
    let windows = session.members().len();
    let active = session.active().to_owned();
    core.workspaces.session = Some(session);
    core.trace(
        "workspacesStarted",
        || serde_json::json!({ "active": active, "windows": windows }),
    );
    discover_focused(core, false, true);
    core.publish_state();
}

/// Return every parked window and stop tracking.
pub(crate) fn stop<P: Platform>(core: &mut Core<P>) -> bool {
    let Some(mut session) = core.workspaces.session.take() else {
        return recover(core);
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
    let owner = remembered
        .as_ref()
        .filter(|_| parked_now || minimized)
        .map(|entry| entry.workspace_id.clone())
        .filter(|id| workspaces.item(id).is_some())
        .unwrap_or_else(|| workspaces.active_id.clone());
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

fn remembered_entry<P: Platform>(core: &Core<P>, info: &WindowInfo) -> Option<SavedWindow> {
    let key = info.recovery_key.as_ref()?;
    core.workspaces
        .remembered
        .iter()
        .find(|entry| &entry.key == key)
        .cloned()
}

// MARK: Recovery

/// Bring back windows a previous run left parked. False when some remain.
pub(crate) fn recover<P: Platform>(core: &mut Core<P>) -> bool {
    let path = core.paths.journal();
    let Some(entries): Option<Vec<SavedWindow>> = read_state(&path) else {
        return !path.exists();
    };
    let windows = core.platform.all_windows();
    let areas = core.work_areas();
    let policy = core.capabilities.parking;
    let mut remaining = Vec::new();

    for entry in entries {
        let Some(parked) = entry.parked_origin else {
            continue;
        };
        let exact = windows
            .iter()
            .find(|w| w.recovery_key.as_ref() == Some(&entry.key));
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
            let still_running = windows.iter().any(|w| w.pid == entry.key.pid);
            if still_running || !by_position.is_empty() {
                remaining.push(entry);
            }
            continue;
        };
        let on_parking_desktop = |core: &mut Core<P>| {
            core.platform
                .state(window.id)
                .is_ok_and(|state| state.on_parking_desktop)
        };
        let at_park = if policy.moves_windows() {
            [Some(parked), entry.prior_parked_origin]
                .into_iter()
                .flatten()
                .any(|p| near(window.frame, p))
        } else {
            on_parking_desktop(core)
        };
        if !at_park {
            continue;
        }
        let home = reachable(entry.frame.with_origin(entry.frame.origin()), &areas).origin();
        let returned = core.platform.show_for_workspace(window.id, home).is_ok()
            && core
                .platform
                .frame(window.id)
                .is_ok_and(|frame| near(frame, home))
            && !on_parking_desktop(core);
        if !returned {
            remaining.push(entry);
        }
    }

    let saved = if remaining.is_empty() {
        remove_state(&path)
    } else {
        write_state(&path, &remaining)
    };
    saved.is_ok() && remaining.is_empty()
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
    let running: Vec<u32> = core.platform.all_windows().iter().map(|info| info.pid).collect();
    let workspaces = &core.settings.workspaces;
    let kept = core.workspaces.remembered.iter().filter(|entry| {
        !tracked.iter().any(|t| t.key == entry.key)
            && workspaces.item(&entry.workspace_id).is_some()
            && running.contains(&entry.key.pid)
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

    let discovered = core.platform.now();
    let displays = core.display_frames();
    let previous = session.active().to_owned();
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.switch_to(target, &displays, !focus_settings, &mut io)
    };
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
            } else {
                let mut io = CoreIo::new(&mut *core);
                let _ = session.switch_to(&previous, &displays, true, &mut io);
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

/// Take the focused window to the previous or next workspace in saved order and switch
/// there, wrapping like `cycle`.
pub(crate) fn take_focused<P: Platform + 'static>(shared: &Rc<Shared<P>>, step: isize) {
    if let Some(target) = adjacent(shared, step) {
        send_focused(shared, &target, true);
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

/// Send the focused window to `target`, then switch there when `follow`.
pub(crate) fn send_focused<P: Platform + 'static>(shared: &Rc<Shared<P>>, target: &str, follow: bool) {
    if !shared.core.borrow().workspaces.is_running() {
        return;
    }
    let target = target.to_owned();
    shared.submit(move |shared, _cancel| async move {
        let sent = {
            let mut core = shared.core.borrow_mut();
            send_now(&mut core, &target)
        };
        if let Some(window) = sent {
            for history in shared.histories.borrow_mut().values_mut() {
                history.forget(window);
            }
            if follow {
                let mut core = shared.core.borrow_mut();
                let now = core.platform.now();
                switch_now(&mut core, &target, false, now);
            }
        }
    });
}

fn send_now<P: Platform>(core: &mut Core<P>, target: &str) -> Option<WindowId> {
    let focused = match core.platform.focused_window() {
        Ok(info) => info,
        Err(_) => {
            core.announce_error("Focus a window to move it to a workspace.");
            return None;
        }
    };
    let mut session = core.workspaces.session.take()?;
    enroll(core, &mut session, &focused);
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
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);
    let sent = match result {
        Ok(()) => {
            save_membership(core, &session);
            Some(focused.id)
        }
        Err(error) => {
            core.announce_error(&error.to_string());
            None
        }
    };
    core.workspaces.session = Some(session);
    core.publish_state();
    sent
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
    if let Some(info) = &focused
        && let Some(member) = session.member(info.id)
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
