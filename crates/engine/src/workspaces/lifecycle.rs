//! Starting, suspending, resuming, detaching, and stopping workspaces.

use std::collections::BTreeMap;
use std::rc::Rc;

use snappy_core::workspace::{Session, WorkspaceIo};
use snappy_platform::{Platform, RecoveryKey};

use crate::log_event::LogEvent;
use crate::ops::Shared;
use crate::selection;
use crate::state::EngineState;

use super::containment::{self, contain_all};
use super::core_io::{CoreIo, save_journal};
use super::events::{Discovery, discover_focused};
use super::members::{enroll, refresh_members, sync_all_tabs};
use super::recovery::{
    LeftParked, SavedWindow, read_windows, recover, rejoin, remembered_entry, save_membership,
};

/// Start or stop workspaces to match settings and permission.
pub(crate) fn configure<P: Platform>(core: &mut EngineState<P>) {
    let wanted =
        core.settings.workspaces.enabled && core.has_permission() && core.live_control.is_available();
    let lost_control =
        core.settings.workspaces.enabled && core.has_permission() && !core.live_control.is_available();
    match (wanted, core.workspaces.session.is_some()) {
        (true, false) => start(core),
        (false, true) if lost_control => suspend(core),
        (false, true) => {
            if let Err(error) = stop(core) {
                core.announce_error(&error.to_string());
            }
        }
        (true, true) | (false, false) => {}
    }
    core.update_tray();
}

/// Live control went away. The session stays as it was and nothing is moved: the platform
/// may already have shown the hidden windows, and `resume` hides them again.
fn suspend<P: Platform>(core: &mut EngineState<P>) {
    if std::mem::replace(&mut core.workspaces.suspended, true) {
        return;
    }
    selection::clear(core, "workspaces suspended");
    core.trace(LogEvent::WorkspacesSuspended, || serde_json::json!({}));
    core.publish_state();
}

/// Live control is back, or the platform started over: bind to the desktop in use again, and
/// hide again any window of an inactive workspace that was shown meanwhile.
pub(crate) fn resume<P: Platform + 'static>(shared: &Rc<Shared<P>>) {
    {
        let mut core = shared.core.borrow_mut();
        let Some(mut session) = core.workspaces.session.take() else {
            return;
        };
        core.workspaces.suspended = false;
        let displays = core.displays.clone();
        core.platform.bind(&displays);
        core.workspaces.paused = !core.platform.on_managed_desktop();
        sync_all_tabs(&mut core, &mut session);
        core.workspaces.session = Some(session);
    }
    let (hidden_again, retrying) = contain_all(shared);
    let mut core = shared.core.borrow_mut();
    let active = core.settings.workspaces.active_id.clone();
    core.trace(
        LogEvent::WorkspacesResumed,
        || serde_json::json!({ "active": active, "hiddenAgain": hidden_again, "retrying": retrying }),
    );
    core.publish_state();
}

/// Start tracking. Windows a previous run left parked for a workspace that isn't showing
/// rejoin it where they are, so a restart while apps keep running changes nothing on
/// screen; every other window joins the active workspace.
fn start<P: Platform>(core: &mut EngineState<P>) {
    let recovered = recover(core, LeftParked::Keep);
    if !recovered.complete {
        let message = if core.capabilities.needs_permission {
            "Some hidden windows couldn't be brought back. Check Accessibility, then quit and reopen Snappy to retry."
        } else {
            "Some hidden windows couldn't be brought back. Quit and reopen Snappy to retry."
        };
        core.announce_error(message);
        return;
    }
    let displays = core.displays.clone();
    core.platform.bind(&displays);
    core.workspaces.paused = false;
    let membership = core.paths.membership();
    let remembered = read_windows(core, &membership).unwrap_or_default();
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
    core.trace(LogEvent::WorkspacesStarted, || {
        serde_json::json!({
            "active": active,
            "windows": windows,
            "rejoined": rejoined,
            "returned": recovered.returned,
            "leftAlone": recovered.left_alone,
            "staleJournal": recovered.stale,
            "closed": recovered.closed,
            "staleMembership": stale.len(),
        })
    });
    discover_focused(core, Discovery::Start);
    core.publish_state();
}

/// Stop for a restart: every window stays where it is, parked ones included, and the journal
/// and membership are saved for the next launch to pick up.
pub(crate) fn detach<P: Platform>(core: &mut EngineState<P>) {
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
            LogEvent::WorkspacesDetached,
            || serde_json::json!({ "active": active, "windows": windows, "parked": parked }),
        ),
        Err(error) => core.flag(
            LogEvent::WorkspacesDetached,
            || serde_json::json!({ "active": active, "windows": windows, "error": error.to_string() }),
        ),
    }
}

/// A parked window couldn't come back yet. It stays parked and journaled for a retry, and
/// a running session keeps running.
#[derive(Debug, thiserror::Error)]
#[error("A hidden window couldn't be brought back yet.")]
pub(crate) struct StillHidden;

/// Return every parked window and stop tracking, including windows a previous run left
/// parked when workspaces never started.
pub(crate) fn stop<P: Platform>(core: &mut EngineState<P>) -> Result<(), StillHidden> {
    selection::clear(core, "workspaces stopped");
    let Some(mut session) = core.workspaces.session.take() else {
        return if recover(core, LeftParked::BringBack).complete {
            Ok(())
        } else {
            Err(StillHidden)
        };
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
        containment::review(core);
    } else {
        core.workspaces.session = Some(session);
    }
    core.update_tray();
    core.publish_state();
    if returned { Ok(()) } else { Err(StillHidden) }
}
