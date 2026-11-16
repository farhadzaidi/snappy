//! Reacting to what the platform reports: focus changes, windows that close, apps that
//! quit, and desktop and display changes. Windows that move are `containment`'s.

use std::rc::Rc;
use std::time::Duration;

use snappy_core::error::WindowError;
use snappy_core::workspace::Session;
use snappy_platform::{Platform, WindowId};

use crate::log_event::LogEvent;
use crate::ops::Shared;
use crate::state::EngineState;

use super::containment::{self, contain_all};
use super::core_io::CoreIo;
use super::members::{enroll, refresh_members, sync_tabs};
use super::recovery::save_membership;
use super::switching::{SwitchFocus, request_switch};

/// Wait this long for a display change to settle.
const DISPLAY_SETTLE: Duration = Duration::from_millis(350);
/// Retry a failed display update once after this long.
const DISPLAY_RETRY: Duration = Duration::from_millis(750);

// MARK: Discovery and focus

/// Why the focused window is being looked at, which says what joins and whether its
/// workspace may be followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Discovery {
    /// Focus changed, by an app's activation or within the app; only an activation may
    /// follow a window to its workspace. The app's other visible windows join too.
    FocusChanged { by_activation: bool },
    /// Workspaces started: the focused app's visible windows join.
    Start,
    /// A switch is about to run: only the focused window joins.
    Switch,
}

/// React to the frontmost app or its focused window changing.
pub(crate) fn focus_changed<P: Platform + 'static>(shared: &Rc<Shared<P>>, by_activation: bool) {
    let follow = {
        let mut core = shared.core.borrow_mut();
        if !core.workspaces.is_running() {
            return;
        }
        discover_focused(&mut core, Discovery::FocusChanged { by_activation })
    };
    if let Some(target) = follow {
        request_switch(shared, &target, SwitchFocus::Workspace);
    }
}

/// Enroll the focused window, and its app's as `discovery` says. Returns a workspace to
/// switch to when an activation revealed a window of another workspace.
pub(super) fn discover_focused<P: Platform>(
    core: &mut EngineState<P>,
    discovery: Discovery,
) -> Option<String> {
    let mut session = core.workspaces.session.take()?;
    let follow = discover_focused_into(core, &mut session, discovery);
    core.workspaces.session = Some(session);
    follow
}

pub(super) fn discover_focused_into<P: Platform>(
    core: &mut EngineState<P>,
    session: &mut Session,
    discovery: Discovery,
) -> Option<String> {
    let by_activation = discovery == Discovery::FocusChanged { by_activation: true };
    let siblings = discovery != Discovery::Switch;
    if core.workspaces.paused {
        return None;
    }
    let focused = match core.platform.focused_window() {
        Ok(info) => Some(info),
        Err(error) => {
            core.trace(
                LogEvent::FocusedWindowUnknown,
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

pub(crate) fn window_destroyed<P: Platform>(core: &mut EngineState<P>, window: WindowId) {
    if let Some(session) = core.workspaces.session.as_mut() {
        session.discard(window);
    }
    if let Some(key) = core.workspaces.keys.remove(&window) {
        core.workspaces
            .remembered
            .retain(|entry| !entry.key.same_window(&key));
    }
    containment::forget(core, window);
    core.publish_state();
}

pub(crate) fn app_terminated<P: Platform>(core: &mut EngineState<P>, pid: u32) {
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

pub(crate) fn native_desktop_changed<P: Platform + 'static>(shared: &Rc<Shared<P>>) {
    let failed = {
        let mut core = shared.core.borrow_mut();
        if core.workspaces.session.is_none() || core.workspaces.suspended {
            return;
        }
        let was_paused = core.workspaces.paused;
        core.workspaces.paused = !core.platform.on_managed_desktop();
        let paused = core.workspaces.paused;
        core.trace(
            LogEvent::NativeDesktop,
            || serde_json::json!({ "paused": paused }),
        );
        // Back on the managed desktop, windows are placed for the displays as they are now,
        // unless displays are still settling, whose update does it once they have.
        let failed = if was_paused && !paused && !core.workspaces.display_update_pending {
            update_displays(&mut core, 1).err()
        } else {
            None
        };
        let now = core.platform.now();
        let active = core.workspaces.session.as_ref().map(|s| s.active().to_owned());
        let count = core.workspaces.session.as_ref().map_or(0, |s| s.members().len());
        core.diagnostics
            .record_lifecycle(now, "nativeDesktopChanged", paused, active, count);
        core.update_tray();
        core.publish_state();
        failed
    };
    if let Some(error) = failed {
        hide_one_by_one(shared, &error);
    }
}

/// Wait for the display change to settle, then update in place with one retry. Until then
/// windows the system moves are left alone (`containment`), since the displays Snappy knows
/// are out of date and the system may still be moving them.
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
        if !apply_display_change(&shared, generation, 1) {
            tokio::time::sleep(DISPLAY_RETRY).await;
            apply_display_change(&shared, generation, 2);
        }
        let deferred = {
            let mut core = shared.core.borrow_mut();
            if core.workspaces.display_update_pending {
                None
            } else {
                core.workspaces.deferred_switch.take()
            }
        };
        if let Some(deferred) = deferred {
            request_switch(&shared, &deferred.target, deferred.focus);
        }
    });
}

/// Returns true when finished, successfully or not.
fn apply_display_change<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    generation: u64,
    attempt: u32,
) -> bool {
    let failed = {
        let mut core = shared.core.borrow_mut();
        if core.workspaces.display_generation != generation {
            return true;
        }
        core.displays = core.platform.displays();
        let failed = update_displays(&mut core, attempt).err();
        if failed.is_some() && attempt == 1 {
            return false;
        }
        core.workspaces.display_update_pending = false;
        core.configure_hotkeys();
        core.prepare_overlays();
        core.refresh_marks();
        core.update_tray();
        core.publish_state();
        failed
    };
    if let Some(error) = failed {
        hide_one_by_one(shared, &error);
    }
    true
}

/// Re-place every window for the displays as they are now, and record how it went.
fn update_displays<P: Platform>(core: &mut EngineState<P>, attempt: u32) -> Result<(), WindowError> {
    if core.workspaces.paused {
        return Ok(());
    }
    let Some(mut session) = core.workspaces.session.take() else {
        return Ok(());
    };
    core.workspaces.focus.begin_change();
    refresh_members(core, &mut session);
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

    let held = core.workspaces.containment.held();
    let count = displays.len();
    match result {
        Ok(moved) => {
            core.trace(
                LogEvent::DisplaysUpdated,
                || serde_json::json!({ "attempt": attempt, "displays": count, "moved": moved, "held": held }),
            );
            containment::all_hidden(core);
            Ok(())
        }
        Err(error) => {
            core.flag(LogEvent::DisplaysUpdated, || {
                serde_json::json!({ "attempt": attempt, "displays": count, "held": held, "error": error.to_string() })
            });
            Err(error)
        }
    }
}

/// Placing windows for the displays is all or nothing, so one app refusing would leave
/// every window where the system put it. Hide windows of workspaces that aren't showing one
/// by one instead, unless the displays aren't usable yet, which the next change fixes.
fn hide_one_by_one<P: Platform + 'static>(shared: &Rc<Shared<P>>, error: &WindowError) {
    if *error != WindowError::InvalidFrame {
        contain_all(shared);
    }
}
