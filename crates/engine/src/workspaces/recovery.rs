//! What a restart keeps: the journal and membership files, picking up the windows a
//! previous run left parked, and windows rejoining their workspaces.

use serde::{Deserialize, Serialize};
use snappy_core::bounds::fitted_origin;
use snappy_core::geometry::{Point, Rect};
use snappy_core::workspace::{
    Found, Journaled, Member, Relaunch, Session, is_parked, reachable, relaunch, sits_at,
};
use snappy_platform::{Platform, RecoveryKey, WindowInfo, WindowState};

use crate::core::Core;
use crate::store::{read_state, remove_state, write_state};

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

/// A window a previous run left parked for a workspace that isn't showing joins it again,
/// keeping its home frame, and stays where it is.
pub(super) fn rejoin<P: Platform>(
    core: &mut Core<P>,
    session: &mut Session,
    info: &WindowInfo,
    entry: &SavedWindow,
) {
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

pub(super) fn remembered_entry<P: Platform>(core: &Core<P>, info: &WindowInfo) -> Option<SavedWindow> {
    let key = info.recovery_key.as_ref()?;
    core.workspaces
        .remembered
        .iter()
        .find(|entry| entry.key.same_window(key))
        .cloned()
}

/// What launch recovery did with the windows a previous run left parked.
#[derive(Default)]
pub(super) struct Recovered {
    /// No window that should have come back is still parked.
    pub(super) complete: bool,
    /// Windows still parked for a workspace that isn't showing, which stay that way.
    pub(super) kept: Vec<(WindowInfo, SavedWindow)>,
    pub(super) returned: usize,
    /// Moved away from their parked spot while Snappy wasn't running.
    pub(super) left_alone: usize,
    /// Their process is gone, or its ID now belongs to a later process.
    pub(super) stale: usize,
}

/// Go through the windows a previous run left parked. With `keep`, windows still parked
/// for a workspace that isn't showing stay parked, journaled, to rejoin it; otherwise every
/// one comes back.
pub(super) fn recover<P: Platform>(core: &mut Core<P>, keep: bool) -> Recovered {
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
        let journaled = Journaled {
            workspace: &entry.workspace_id,
            workspace_exists: core.settings.workspaces.item(&entry.workspace_id).is_some(),
            size: entry.frame.size(),
            parked,
            prior_parked: entry.prior_parked_origin,
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
            .filter(|w| journaled.found_at(w.frame))
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
                        .is_ok_and(|frame| sits_at(frame, home))
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

pub(super) fn save_membership<P: Platform>(core: &mut Core<P>, session: &Session) {
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
