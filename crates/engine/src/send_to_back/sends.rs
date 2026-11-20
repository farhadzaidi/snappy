//! Sending: each press sends the focused window behind the windows it covers, so they show,
//! and focus goes to a window over where it was, so the next press goes on through the
//! windows stacked there. It changes the stack, so it waits in the operation queue like any
//! move, and sends queue behind one another there rather than cancel, so quick presses each
//! count. A hold's put-back (`put_back`) queues behind its send the same way. A platform
//! that can lower another app's window lowers it; one that can't raises the windows that
//! show over it instead (`raising`).

use std::collections::VecDeque;
use std::rc::Rc;

use snappy_core::error::WindowError;
use snappy_core::placement::bounds::CancelToken;
use snappy_core::send_to_back::{Restacking, SendToBack, Sent, StackedWindow};
use snappy_platform::Platform;
use tokio::time::Instant;

use super::press::LastSend;
use super::{put_back, raising};
use crate::diagnostics::millis;
use crate::log_event::LogEvent;
use crate::ops::Shared;
use crate::state::EngineState;

const NOTHING_BEHIND: &str = "There's no window behind this one.";

/// What waits its turn in the send queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Turn {
    /// Send the focused window to the back.
    Send,
    /// Put the window the last turn sent back where it was.
    PutBack,
}

/// Turns waiting, oldest first, and the run of turns under way, which takes each in turn.
#[derive(Default)]
pub(crate) struct Sends {
    waiting: VecDeque<Turn>,
    /// Cancelled once a newer operation takes the queue.
    run: Option<CancelToken>,
}

impl Sends {
    /// Drop a put-back waiting its turn, so a hold let go leaves its window at the back.
    pub(super) fn cancel_put_back(&mut self) {
        self.waiting.retain(|turn| *turn != Turn::PutBack);
    }
}

/// How a send went where it went.
#[derive(Debug, Clone, Copy)]
pub(super) enum SentBy {
    /// The platform lowered the window.
    Lowering,
    /// The windows that show over it were raised in `steps` steps, and `reached` says
    /// whether the window list showed the last order.
    Raising { steps: usize, reached: bool },
}

/// Queue `turn` after the turns before it, or once the operation under way has stopped.
pub(super) fn queue<P: Platform + 'static>(shared: &Rc<Shared<P>>, turn: Turn) {
    {
        let sends = &mut shared.core.borrow_mut().sending_to_back.sends;
        if sends.run.take_if(|run| run.is_cancelled()).is_some() {
            // The turns a cancelled run left waiting go with it.
            sends.waiting.clear();
        }
        sends.waiting.push_back(turn);
        if sends.run.is_some() {
            return;
        }
    }
    let run = shared.submit_send_to_back(|shared, cancel| async move { run_sends(&shared, &cancel).await });
    shared.core.borrow_mut().sending_to_back.sends.run = Some(run);
}

/// Take the waiting turns one at a time, until none is left or a newer operation cancels
/// the run. The send under way then finishes without waiting, and the turns waiting are
/// left alone, as a newer run may have them.
async fn run_sends<P: Platform + 'static>(shared: &Rc<Shared<P>>, cancel: &CancelToken) {
    loop {
        let turn = {
            let mut core = shared.core.borrow_mut();
            let sends = &mut core.sending_to_back.sends;
            if cancel.is_cancelled() {
                return;
            }
            let Some(turn) = sends.waiting.pop_front() else {
                sends.run = None;
                return;
            };
            turn
        };
        match turn {
            Turn::Send => send(shared, cancel).await,
            Turn::PutBack => put_back::put_back(&mut shared.core.borrow_mut()),
        }
    }
}

/// Send the window the last send's `next_target` names behind what it covers, as core
/// plans it, and remember it for the next press and for a hold's put-back.
async fn send<P: Platform + 'static>(shared: &Rc<Shared<P>>, cancel: &CancelToken) {
    let started = Instant::now();
    let last = {
        let state = &mut shared.core.borrow_mut().sending_to_back;
        // A hold let go from now on is this send's, not the one before.
        if let Some(last) = &mut state.last {
            last.can_put_back = false;
        }
        state
            .last
            .filter(|last| last.focus_unreported)
            .map(|last| last.sent)
    };
    let Some((plan, stacked)) = plan(&mut shared.core.borrow_mut(), last) else {
        return;
    };
    let restacking = shared.core.borrow().capabilities.restacking;
    let sent_by = match restacking {
        Restacking::Lower { .. } => lower(&mut shared.core.borrow_mut(), &plan).map(|()| SentBy::Lowering),
        Restacking::RaiseOnly => raising::raise_over(shared, &plan, &stacked, started, cancel).await,
    };
    let mut core = shared.core.borrow_mut();
    let took_ms = millis(started.elapsed().as_secs_f64());
    let (window, focus, covered) = (plan.window().id.0, plan.focus().id.0, plan.covered().len());
    match sent_by {
        Ok(sent_by) => {
            core.sending_to_back.last = Some(LastSend::new(plan.sent()));
            let (steps, reached) = match sent_by {
                SentBy::Lowering => (None, None),
                SentBy::Raising { steps, reached } => (Some(steps), Some(reached)),
            };
            let details = || {
                serde_json::json!({
                    "window": window, "focus": focus, "covered": covered, "raises": steps,
                    "reached": reached, "tookMs": took_ms,
                })
            };
            if reached == Some(false) {
                core.flag(LogEvent::SentToBack, details);
            } else {
                core.trace(LogEvent::SentToBack, details);
            }
        }
        Err(error) => {
            core.sending_to_back.last = None;
            core.flag(
                LogEvent::SentToBack,
                || serde_json::json!({ "window": window, "covered": covered, "error": error.to_string() }),
            );
            core.announce_quietly(&error.to_string());
        }
    }
}

/// What a send does, with the windows it can restack, or `None`, said quietly, when there's
/// nothing to send back. `last` is the last send, while focus hasn't been reported to change
/// since.
fn plan<P: Platform>(
    core: &mut EngineState<P>,
    last: Option<Sent>,
) -> Option<(SendToBack, Vec<StackedWindow>)> {
    let focused = match core.platform.focused_window() {
        Ok(focused) => focused.id,
        Err(error) => {
            core.sending_to_back.last = None;
            core.trace(LogEvent::SentToBack, || serde_json::json!({ "window": null }));
            core.announce_quietly(&error.to_string());
            return None;
        }
    };
    let stacked = stacked(core);
    let target = last.map_or(focused, |last| last.next_target(focused, &stacked));
    let plan = SendToBack::plan(target, &stacked);
    if plan.is_none() {
        core.trace(
            LogEvent::SentToBack,
            || serde_json::json!({ "window": target.0, "covered": 0 }),
        );
        core.announce_quietly(NOTHING_BEHIND);
    }
    plan.map(|plan| (plan, stacked))
}

/// The windows a press can send a window behind, frontmost first: the visible ones of the
/// active workspace. Visible windows are never minimized, hidden tabs, or on another desktop.
fn stacked<P: Platform>(core: &mut EngineState<P>) -> Vec<StackedWindow> {
    let visible = core.platform.visible_windows(None);
    visible
        .into_iter()
        .filter(|window| !core.workspaces.is_inactive(window.id))
        .map(|window| StackedWindow {
            id: window.id,
            pid: window.pid,
            frame: window.frame,
        })
        .collect()
}

/// Lower the window behind every other and focus the window core picks.
fn lower<P: Platform>(core: &mut EngineState<P>, plan: &SendToBack) -> Result<(), WindowError> {
    core.platform.lower(plan.window().id)?;
    core.platform.focus(plan.focus().id);
    Ok(())
}
