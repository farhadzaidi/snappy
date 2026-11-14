//! Reacting to what the platform reports: focus changes, windows that move or close, apps
//! that quit, and desktop and display changes.

use std::rc::Rc;
use std::time::Duration;

use snappy_core::error::WindowError;
use snappy_core::workspace::Session;
use snappy_platform::{Platform, WindowId};

use crate::core::Core;
use crate::ops::Shared;

use super::CoreIo;
use super::members::{enroll, sync_all_tabs, sync_tabs};
use super::recovery::save_membership;
use super::switching::request_switch;

/// Wait this long for a display change to settle.
const DISPLAY_SETTLE: Duration = Duration::from_millis(350);
/// Retry a failed display update once after this long.
const DISPLAY_RETRY: Duration = Duration::from_millis(750);
/// Re-park a revealed window at most this often.
const CONTAINMENT_INTERVAL: f64 = 1.0;

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
pub(super) fn discover_focused<P: Platform>(
    core: &mut Core<P>,
    by_activation: bool,
    siblings: bool,
) -> Option<String> {
    let mut session = core.workspaces.session.take()?;
    let follow = discover_focused_into(core, &mut session, by_activation, siblings);
    core.workspaces.session = Some(session);
    follow
}

pub(super) fn discover_focused_into<P: Platform>(
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
