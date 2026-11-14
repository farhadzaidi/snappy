//! Switching workspaces, and sending windows to them.

use std::rc::Rc;

use snappy_core::error::WindowError;
use snappy_core::workspace::Session;
use snappy_ipc::ToSettings;
use snappy_platform::{Platform, WindowId, WindowInfo};

use crate::core::Core;
use crate::diagnostics::{PhaseClock, SwitchTiming};
use crate::ops::Shared;
use crate::selection;

use super::CoreIo;
use super::events::discover_focused_into;
use super::members::{enroll, sync_all_tabs, sync_tabs};
use super::recovery::save_membership;
use super::refresh_members;

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
    let mut clock = PhaseClock::new(requested_at);
    let mut timing = SwitchTiming {
        queue_ms: clock.lap(core.platform.now()),
        ..SwitchTiming::default()
    };
    core.workspaces.focus.begin_change();
    discover_focused_into(core, &mut session, false, false);
    refresh_members(core, &mut session);
    timing.discovery_ms = clock.lap(core.platform.now());

    let displays = core.display_frames();
    let previous = session.active().to_owned();
    let along = core.selection.windows.clone();
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.switch_taking_along(target, &along, &displays, !focus_settings, &mut io)
    };
    let arrived: Vec<WindowId> = along
        .iter()
        .copied()
        .filter(|window| {
            result.is_ok()
                && session
                    .member(*window)
                    .is_some_and(|member| member.workspace == target)
        })
        .collect();
    timing.window_ms = clock.lap(core.platform.now());
    timing.windows = session.members().len();

    match result {
        Ok(focused_any) => {
            if focus_settings {
                core.outbox.push(ToSettings::Focus);
            } else if !focused_any {
                core.platform.activate_self();
            }
            save_membership(core, &session);
            timing.membership_ms = clock.lap(core.platform.now());
            if commit_active(core, target) {
                timing.settings_ms = clock.lap(core.platform.now());
                core.configure_hotkeys();
                core.prepare_overlays();
                timing.refresh_ms = clock.lap(core.platform.now());
                timing.succeeded = true;
                if !along.is_empty() {
                    let windows: Vec<u64> = arrived.iter().map(|window| window.0).collect();
                    let tabs = tabs_along(&session, &arrived);
                    core.trace("selectionTakenAlong", || {
                        serde_json::json!({ "to": target, "windows": windows, "selected": along.len(), "tabs": tabs })
                    });
                    selection::clear(core, "taken along");
                }
            } else {
                let mut io = CoreIo::new(&mut *core);
                let _ = session.switch_taking_along(&previous, &arrived, &displays, true, &mut io);
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

    timing.total_ms = clock.total(core.platform.now());
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
    let groups: serde_json::Map<String, serde_json::Value> = session
        .tab_roots(windows)
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
            if follow || !selection::has_windows(&mut core) {
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
            core.announce_error("Focus a window to send it to a workspace.");
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

    let outcome = send_windows(core, &mut session, &[focused.id], target, stay);
    let sent = match outcome.failure {
        None => {
            let tabs = tabs_along(&session, &[focused.id]);
            core.trace("workspaceSend", || {
                serde_json::json!({
                    "window": focused.id.0,
                    "app": focused.app,
                    "to": target,
                    "stay": stay,
                    "focused": outcome.focused.map(|window| window.0),
                    "tabs": tabs,
                })
            });
            Some(focused.id)
        }
        Some(error) => {
            core.announce_error(&error.to_string());
            None
        }
    };
    core.workspaces.session = Some(session);
    if let Some(window) = sent {
        // The whole tab group went, so any of its tabs selected goes too.
        let group = core.workspaces.tab_root(window);
        selection::remove_groups(core, &[group], "sent to another workspace");
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
    let selected: Vec<WindowId> = session
        .tab_roots(&chosen)
        .into_iter()
        .filter(|root| {
            session
                .member(*root)
                .is_some_and(|member| member.workspace == session.active())
        })
        .collect();

    let outcome = send_windows(core, &mut session, &selected, target, true);
    if !outcome.sent.is_empty() {
        let windows: Vec<u64> = outcome.sent.iter().map(|window| window.0).collect();
        let tabs = tabs_along(&session, &outcome.sent);
        core.trace("workspaceSend", || {
            serde_json::json!({
                "windows": windows,
                "to": target,
                "stay": true,
                "focused": outcome.focused.map(|window| window.0),
                "tabs": tabs,
            })
        });
    }
    if let Some(error) = outcome.failure {
        core.announce_error(&error.to_string());
    }
    core.workspaces.session = Some(session);
    selection::remove_groups(core, &outcome.sent, "sent to another workspace");
    core.publish_state();
    outcome.sent
}

/// What sending windows to another workspace came to.
struct Sent {
    sent: Vec<WindowId>,
    /// The first refusal; the windows before and after it are still sent.
    failure: Option<WindowError>,
    /// The window that got focus after them.
    focused: Option<WindowId>,
}

/// Send `windows` of the active workspace to `target`, saving membership when any went.
/// With `stay`, focus moves to the window used most recently among those left.
fn send_windows<P: Platform>(
    core: &mut Core<P>,
    session: &mut Session,
    windows: &[WindowId],
    target: &str,
    stay: bool,
) -> Sent {
    core.workspaces.focus.begin_change();
    let displays = core.display_frames();
    let mut sent = Vec::new();
    let mut failure = None;
    for window in windows {
        let result = {
            let mut io = CoreIo::new(&mut *core);
            session.send(*window, target, &displays, &mut io)
        };
        match result {
            Ok(()) => sent.push(*window),
            Err(error) => failure = failure.or(Some(error)),
        }
    }
    let focused = if stay && !sent.is_empty() {
        session.focus_recent(&mut CoreIo::new(&mut *core))
    } else {
        None
    };
    let now = core.platform.now();
    core.workspaces.focus.end_change(now);
    if !sent.is_empty() {
        save_membership(core, session);
    }
    Sent {
        sent,
        failure,
        focused,
    }
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
    let shown = session.shows(info.id);
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
