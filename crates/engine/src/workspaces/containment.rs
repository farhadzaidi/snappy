//! Keeping parked windows hidden when their app or the system brings them back on screen.
//!
//! While displays change, the system moves windows and the displays Snappy knows are out of
//! date, so a window revealed then is left to the display update, which parks every window
//! again against the final displays. Otherwise a revealed window is parked again at once.
//! One that won't hide is tried once more a moment later, and only one still showing after
//! that is reported, with what the user can do about it.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::time::Duration;

use snappy_platform::{Platform, WindowId};

use crate::debug_log::Level;
use crate::log_event::LogEvent;
use crate::ops::Shared;
use crate::state::EngineState;

use super::core_io::CoreIo;

/// Park a revealed window again at most this often, in seconds.
const CONTAINMENT_INTERVAL: f64 = 1.0;
/// Try a window that wouldn't hide once more after this long.
const CONTAINMENT_RETRY: Duration = Duration::from_secs(1);

/// What containment is waiting on, and what it has told the user.
#[derive(Default)]
pub(crate) struct Containment {
    /// When each window was last parked again.
    tried_at: BTreeMap<WindowId, f64>,
    /// Windows revealed while displays settled, which the display update parks again.
    held: BTreeSet<WindowId>,
    /// Windows that wouldn't hide, waiting to be tried once more.
    retrying: BTreeSet<WindowId>,
    /// Windows still showing after their retry.
    showing: BTreeSet<WindowId>,
    /// The status that tells the user about `showing`.
    notice: Option<u64>,
}

impl Containment {
    /// How many windows were revealed while displays settled.
    pub fn held(&self) -> usize {
        self.held.len()
    }
}

/// What one try at hiding a window again came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// Hidden already, or not a window of a workspace that isn't showing.
    Hidden,
    HiddenAgain,
    Showing,
}

/// A window moved: park it again when it belongs to a workspace that isn't showing.
pub(crate) fn window_moved<P: Platform + 'static>(shared: &Rc<Shared<P>>, window: WindowId) {
    let outcome = {
        let mut core = shared.core.borrow_mut();
        let workspaces = &core.workspaces;
        if !workspaces.is_running()
            || workspaces.focus.is_changing()
            || !workspaces.is_inactive(window)
            || workspaces.is_hidden_tab(window)
            || !core.platform.on_managed_desktop()
        {
            return;
        }
        if core.workspaces.display_update_pending {
            core.workspaces.containment.held.insert(window);
            return;
        }
        let now = core.platform.now();
        let containment = &core.workspaces.containment;
        let recently = containment
            .tried_at
            .get(&window)
            .is_some_and(|at| now - at < CONTAINMENT_INTERVAL);
        if recently || containment.retrying.contains(&window) {
            return;
        }
        attempt(&mut core, window, 1)
    };
    if outcome == Outcome::Showing {
        retry_later(shared, vec![window]);
    }
}

/// Park again, one by one, every window of a workspace that isn't showing that the
/// platform has shown. Returns how many were hidden again and how many wait for a retry.
pub(super) fn contain_all<P: Platform + 'static>(shared: &Rc<Shared<P>>) -> (usize, usize) {
    let (hidden_again, showing) = {
        let mut core = shared.core.borrow_mut();
        core.workspaces.containment.held.clear();
        let inactive: Vec<WindowId> = core
            .workspaces
            .session
            .as_ref()
            .map(|session| {
                session
                    .members()
                    .keys()
                    .copied()
                    .filter(|window| session.is_inactive(*window) && !session.is_hidden_tab(*window))
                    .collect()
            })
            .unwrap_or_default();
        let mut hidden_again = 0;
        let mut showing = Vec::new();
        for window in inactive {
            match attempt(&mut core, window, 1) {
                Outcome::Hidden => {}
                Outcome::HiddenAgain => hidden_again += 1,
                Outcome::Showing => showing.push(window),
            }
        }
        (hidden_again, showing)
    };
    let retrying = showing.len();
    if !showing.is_empty() {
        retry_later(shared, showing);
    }
    (hidden_again, retrying)
}

/// The display update parked every window again, so none is left revealed.
pub(super) fn all_hidden<P: Platform>(core: &mut EngineState<P>) {
    let containment = &mut core.workspaces.containment;
    containment.held.clear();
    containment.showing.clear();
    withdraw_notice(core);
}

/// Stop reporting windows that no longer belong to a workspace that isn't showing, such as
/// after a switch to theirs, and take the notice back once none is left.
pub(super) fn review<P: Platform>(core: &mut EngineState<P>) {
    let workspaces = &core.workspaces;
    let still: BTreeSet<WindowId> = workspaces
        .containment
        .showing
        .iter()
        .copied()
        .filter(|window| workspaces.is_inactive(*window))
        .collect();
    core.workspaces.containment.showing = still;
    if core.workspaces.containment.showing.is_empty() {
        withdraw_notice(core);
    }
}

/// A window closed.
pub(super) fn forget<P: Platform>(core: &mut EngineState<P>, window: WindowId) {
    let containment = &mut core.workspaces.containment;
    containment.tried_at.remove(&window);
    containment.held.remove(&window);
    containment.retrying.remove(&window);
    stop_reporting(core, window);
}

/// Try `windows` once more after `CONTAINMENT_RETRY`, and tell the user about those still
/// showing then.
fn retry_later<P: Platform + 'static>(shared: &Rc<Shared<P>>, windows: Vec<WindowId>) {
    shared
        .core
        .borrow_mut()
        .workspaces
        .containment
        .retrying
        .extend(&windows);
    let shared = Rc::clone(shared);
    tokio::task::spawn_local(async move {
        tokio::time::sleep(CONTAINMENT_RETRY).await;
        let mut core = shared.core.borrow_mut();
        let mut showing = false;
        for window in windows {
            // Closed meanwhile, or left for the display update or a resume to hide.
            if !core.workspaces.containment.retrying.remove(&window) || !core.workspaces.is_running() {
                continue;
            }
            if core.workspaces.display_update_pending {
                core.workspaces.containment.held.insert(window);
                continue;
            }
            if attempt(&mut core, window, 2) == Outcome::Showing {
                core.workspaces.containment.showing.insert(window);
                showing = true;
            }
        }
        if showing {
            post_notice(&mut core);
        }
    });
}

/// Park `window` again where the current displays put parked windows, and log the outcome.
/// `attempt` counts the tries for this reveal.
fn attempt<P: Platform>(core: &mut EngineState<P>, window: WindowId, attempt: u32) -> Outcome {
    let Some(mut session) = core.workspaces.session.take() else {
        return Outcome::Hidden;
    };
    let now = core.platform.now();
    core.workspaces.containment.tried_at.insert(window, now);
    let displays = core.display_frames();
    let result = {
        let mut io = CoreIo::new(&mut *core);
        session.contain(window, &displays, &mut io)
    };
    let active = session.active().to_owned();
    let count = session.members().len();
    core.workspaces.session = Some(session);

    let (outcome, error) = match result {
        Ok(false) => (Outcome::Hidden, None),
        Ok(true) => {
            core.diagnostics
                .record_lifecycle(now, "containedRevealedWindow", false, Some(active), count);
            (Outcome::HiddenAgain, None)
        }
        Err(error) => (Outcome::Showing, Some(error.to_string())),
    };
    if outcome != Outcome::Showing {
        stop_reporting(core, window);
    }
    log_attempt(core, window, attempt, outcome, error);
    outcome
}

/// Record a try that moved a window or failed to, with its app's identifier.
fn log_attempt<P: Platform>(
    core: &mut EngineState<P>,
    window: WindowId,
    attempt: u32,
    outcome: Outcome,
    error: Option<String>,
) {
    let (label, level) = match (outcome, attempt) {
        (Outcome::Hidden, _) => return,
        (Outcome::HiddenAgain, _) => ("hiddenAgain", Level::Info),
        (Outcome::Showing, 1) => ("retrying", Level::Warn),
        (Outcome::Showing, _) => ("stillShowing", Level::Warn),
    };
    // Looking the app up lists every window, so only while the log is on.
    if !core.debug.is_on() {
        return;
    }
    let app = core
        .platform
        .all_windows()
        .into_iter()
        .find(|info| info.id == window)
        .map(|info| info.app);
    core.log(level, LogEvent::Containment, || {
        let mut details = serde_json::json!({
            "window": window.0,
            "app": app,
            "attempt": attempt,
            "outcome": label,
        });
        if let Some(error) = error {
            details["error"] = serde_json::json!(error);
        }
        details
    });
}

/// Tell the user which workspace's windows won't stay hidden, and how to hide them.
fn post_notice<P: Platform>(core: &mut EngineState<P>) {
    let Some(session) = core.workspaces.session.as_ref() else {
        return;
    };
    let showing = &core.workspaces.containment.showing;
    let workspaces: BTreeSet<&str> = showing
        .iter()
        .filter_map(|window| session.member(*window))
        .map(|member| member.workspace.as_str())
        .collect();
    let names: Vec<&str> = workspaces
        .iter()
        .filter_map(|id| core.settings.workspaces.item(id))
        .map(|workspace| workspace.name.as_str())
        .collect();
    let message = still_showing_message(&names, showing.len());
    core.announce_error(&message);
    core.workspaces.containment.notice = core.status().map(|status| status.id);
}

fn still_showing_message(workspaces: &[&str], windows: usize) -> String {
    match (workspaces, windows) {
        ([workspace], 1) => {
            format!("A window from {workspace} won't stay hidden; switch there and back to hide it.")
        }
        ([workspace], _) => {
            format!("Windows from {workspace} won't stay hidden; switch there and back to hide them.")
        }
        _ => "Windows from other workspaces won't stay hidden; switch to each and back to hide them."
            .to_owned(),
    }
}

/// `window` no longer needs reporting; take the notice back once no window does.
fn stop_reporting<P: Platform>(core: &mut EngineState<P>, window: WindowId) {
    let containment = &mut core.workspaces.containment;
    containment.showing.remove(&window);
    if containment.showing.is_empty() {
        withdraw_notice(core);
    }
}

/// Clear the notice while it is still the current status, the way a quiet status clears
/// an error.
fn withdraw_notice<P: Platform>(core: &mut EngineState<P>) {
    let Some(notice) = core.workspaces.containment.notice.take() else {
        return;
    };
    if core.status().is_some_and(|status| status.id == notice) {
        core.announce_quietly("Every window from another workspace is hidden.");
    }
}

#[cfg(test)]
mod tests {
    use super::still_showing_message;

    #[test]
    fn the_notice_names_the_one_workspace_and_counts_its_windows() {
        assert_eq!(
            still_showing_message(&["Workspace 2"], 1),
            "A window from Workspace 2 won't stay hidden; switch there and back to hide it."
        );
        assert_eq!(
            still_showing_message(&["Workspace 2"], 2),
            "Windows from Workspace 2 won't stay hidden; switch there and back to hide them."
        );
    }

    #[test]
    fn the_notice_about_several_workspaces_names_none() {
        assert_eq!(
            still_showing_message(&["Workspace 2", "Workspace 3"], 2),
            "Windows from other workspaces won't stay hidden; switch to each and back to hide them."
        );
    }
}
