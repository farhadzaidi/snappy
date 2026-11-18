//! Bringing windows into the session and dropping closed ones, and keeping each tab group
//! in one workspace.

use std::collections::{BTreeMap, BTreeSet};

use snappy_core::geometry::Rect;
use snappy_core::placement::bounds::fitted_origin;
use snappy_core::workspace::{Member, Session, Tabs, is_parked};
use snappy_platform::{Platform, TabGroup, WindowId, WindowInfo};

use crate::log_event::LogEvent;
use crate::state::EngineState;

use super::core_io::{CoreIo, save_journal};
use super::recovery::{remembered_entry, save_membership};

/// Add a window to the session: back to its remembered workspace, or the active one.
pub(super) fn enroll<P: Platform>(core: &mut EngineState<P>, session: &mut Session, info: &WindowInfo) {
    if session.member(info.id).is_some() {
        return;
    }
    let skipped = |core: &mut EngineState<P>, reason: &str| {
        core.trace(
            LogEvent::WorkspaceJoinSkipped,
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
        && join_tab_group(core, session, &group)
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
            core.flag(LogEvent::WorkspaceRescued, || {
                serde_json::json!({ "window": info.id.0, "app": info.app, "from": info.frame, "to": rescued })
            });
            rescued
        }
    };
    let visible_frame = Rect::from_origin_size(visible_origin, info.frame.size());

    core.workspaces.keys.insert(info.id, key);
    let active = session.active().to_owned();
    core.trace(LogEvent::WorkspaceJoin, || {
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
        save_journal(core, session);
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
fn join_tab_group<P: Platform>(core: &mut EngineState<P>, session: &mut Session, group: &TabGroup) -> bool {
    let info = &group.shown;
    let hidden = group.hidden_windows();
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
    let behind: Vec<u64> = hidden.iter().map(|window| window.0).collect();
    core.trace(LogEvent::WorkspaceJoin, || {
        serde_json::json!({
            "window": info.id.0,
            "app": info.app,
            "workspace": member.workspace,
            "frame": member.frame,
            "hiddenTabs": behind,
        })
    });
    session.enroll(info.id, member);
    let mut refreshed = hidden;
    refreshed.push(info.id);
    learn_tabs(core, session, std::slice::from_ref(group), &refreshed);
    save_journal(core, session);
    true
}

/// Learn the tab groups of every member's app.
pub(super) fn sync_all_tabs<P: Platform>(core: &mut EngineState<P>, session: &mut Session) {
    let members: Vec<WindowId> = session.members().keys().copied().collect();
    sync_tabs(core, session, &members);
}

/// Learn from the platform which members of the apps owning `windows` are tabs of one
/// window, and keep each group in its shown tab's workspace. A shown tab Snappy hasn't met
/// joins the tabs behind it.
pub(super) fn sync_tabs<P: Platform>(core: &mut EngineState<P>, session: &mut Session, windows: &[WindowId]) {
    let groups = core.platform.tab_groups(windows);
    for group in &groups {
        if session.member(group.shown.id).is_none() {
            join_tab_group(core, session, group);
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
    let moved = learn_tabs(core, session, &groups, &refreshed);
    if moved.is_empty() {
        return;
    }
    let unified: Vec<serde_json::Value> = groups
        .iter()
        .filter(|group| group.hidden.iter().any(|tab| moved.contains(&tab.window)))
        .map(|group| {
            let workspace = session
                .member(group.shown.id)
                .map(|member| member.workspace.clone());
            let hidden: Vec<u64> = group.hidden.iter().map(|tab| tab.window.0).collect();
            serde_json::json!({ "shown": group.shown.id.0, "hidden": hidden, "workspace": workspace })
        })
        .collect();
    core.trace(LogEvent::TabsUnified, || serde_json::json!({ "groups": unified }));
    save_journal(core, session);
    save_membership(core, session);
}

/// Replace what the session knows of the tab groups of the `refreshed` members with
/// `groups`, and log each member that starts or stops counting as a hidden tab, with the
/// platform's evidence and its group's tab count. Returns the hidden tabs that changed
/// workspace.
fn learn_tabs<P: Platform>(
    core: &mut EngineState<P>,
    session: &mut Session,
    groups: &[TabGroup],
    refreshed: &[WindowId],
) -> Vec<WindowId> {
    let behind_before: BTreeMap<WindowId, WindowId> = refreshed
        .iter()
        .filter(|window| session.is_hidden_tab(**window))
        .map(|window| (*window, session.tab_root(*window)))
        .collect();
    let tabs: Vec<Tabs> = groups
        .iter()
        .map(|group| Tabs {
            shown: group.shown.id,
            hidden: group.hidden_windows(),
        })
        .collect();
    let moved = session.update_tabs(&tabs, refreshed);

    let joined: Vec<serde_json::Value> = groups
        .iter()
        .flat_map(|group| group.hidden.iter().map(move |tab| (group, tab)))
        .filter(|(group, tab)| {
            session.tab_root(tab.window) == group.shown.id
                && behind_before.get(&tab.window) != Some(&group.shown.id)
        })
        .map(|(group, tab)| {
            serde_json::json!({
                "window": tab.window.0,
                "shown": group.shown.id.0,
                "tabs": group.tabs,
                "evidence": tab.evidence.name(),
            })
        })
        .collect();
    let left: Vec<serde_json::Value> = behind_before
        .iter()
        .filter(|(window, _)| !session.is_hidden_tab(**window))
        .map(|(window, shown)| serde_json::json!({ "window": window.0, "shown": shown.0 }))
        .collect();
    if !joined.is_empty() || !left.is_empty() {
        core.trace(
            LogEvent::TabsChanged,
            || serde_json::json!({ "joined": joined, "left": left }),
        );
    }
    moved
}

/// Stop tracking members that closed without the platform saying so, then learn every
/// remaining member's tab groups, as every change to all of them starts.
pub(super) fn refresh_members<P: Platform>(core: &mut EngineState<P>, session: &mut Session) {
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
