//! The window-operation queue and the operations that run on it. Every window move goes
//! through it, one at a time, so two operations never fight over the same window.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use snappy_core::bounds::{BoundsBackend, CancelToken, Clock, apply_settled};
use snappy_core::error::WindowError;
use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::operations::{self, Replay, SwapRequest, UndoHistory, WindowOps};
use snappy_core::swap::{SwapPlan, SwapSource, ZoneSlot, plan_swap};
use snappy_core::window::WindowSnapshot;
use snappy_platform::{Platform, WindowId};
use tokio::task::JoinHandle;

use crate::core::Core;
use crate::debug_log::Level;
use crate::diagnostics::{DropRecord, Step};

/// Everything the engine and its in-flight operation share on the engine thread.
pub(crate) struct Shared<P> {
    pub core: RefCell<Core<P>>,
    /// Undo histories by scope: one per workspace, or one global.
    pub histories: RefCell<BTreeMap<String, UndoHistory>>,
    queue: RefCell<Queue>,
}

#[derive(Default)]
struct Queue {
    generation: u64,
    cancel: Option<CancelToken>,
    running: Option<JoinHandle<()>>,
}

impl<P: Platform + 'static> Shared<P> {
    pub fn new(core: Core<P>) -> Rc<Self> {
        Rc::new(Self {
            core: RefCell::new(core),
            histories: RefCell::default(),
            queue: RefCell::default(),
        })
    }

    /// Cancel the operation in flight, if any.
    pub fn cancel_current(&self) {
        if let Some(cancel) = &self.queue.borrow().cancel {
            cancel.cancel();
        }
        self.core.borrow_mut().pending_navigation = None;
    }

    /// Run `operation` after the one in flight has stopped. The newest operation wins:
    /// submitting cancels the previous one first.
    pub fn submit<F, Fut>(self: &Rc<Self>, operation: F)
    where
        F: FnOnce(Rc<Self>, CancelToken) -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.cancel_current();
        let cancel = CancelToken::default();
        let (generation, previous) = {
            let mut queue = self.queue.borrow_mut();
            queue.generation += 1;
            queue.cancel = Some(cancel.clone());
            (queue.generation, queue.running.take())
        };
        self.core.borrow_mut().op_in_flight = true;

        let shared = Rc::clone(self);
        let task = tokio::task::spawn_local(async move {
            if let Some(previous) = previous {
                let _ = previous.await;
            }
            if !cancel.is_cancelled() {
                operation(Rc::clone(&shared), cancel).await;
            }
            shared.finish(generation);
        });
        self.queue.borrow_mut().running = Some(task);
    }

    fn finish(&self, generation: u64) {
        if self.queue.borrow().generation != generation {
            return;
        }
        let mut core = self.core.borrow_mut();
        core.op_in_flight = false;
        core.op_windows.clear();
        core.protected.clear();
        core.publish_state();
    }

    /// Wait for the queue to drain. Used at quit and by tests.
    pub async fn idle(&self) {
        loop {
            let running = self.queue.borrow_mut().running.take();
            match running {
                Some(task) => {
                    let _ = task.await;
                }
                None => return,
            }
        }
    }

    /// Take a scope's undo history for the duration of an operation. Operations run one
    /// at a time, so nothing else needs it meanwhile.
    pub fn take_history(&self, scope: &str) -> UndoHistory {
        self.histories.borrow_mut().remove(scope).unwrap_or_default()
    }

    pub fn return_history(&self, scope: String, history: UndoHistory) {
        self.histories.borrow_mut().insert(scope, history);
    }

    pub fn history_scope(&self) -> String {
        let core = self.core.borrow();
        let workspaces = &core.settings.workspaces;
        if workspaces.enabled {
            format!("workspace:{}", workspaces.active_id)
        } else {
            "global".to_owned()
        }
    }
}

/// Real waits on the engine's runtime.
pub(crate) struct TokioClock {
    started: tokio::time::Instant,
}

impl Default for TokioClock {
    fn default() -> Self {
        Self {
            started: tokio::time::Instant::now(),
        }
    }
}

impl Clock for TokioClock {
    async fn sleep(&self, milliseconds: u64) {
        tokio::time::sleep(Duration::from_millis(milliseconds)).await;
    }

    fn now_ms(&self) -> f64 {
        self.started.elapsed().as_secs_f64() * 1000.0
    }
}

/// Window access for operations, borrowing the platform only for each call.
pub(crate) struct Live<P>(pub Rc<Shared<P>>);

impl<P: Platform + 'static> WindowOps for Live<P> {
    fn read(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        self.0.core.borrow_mut().platform.frame(window)
    }

    async fn settle(
        &mut self,
        window: WindowId,
        frame: Rect,
        work_area: Option<Rect>,
        cancel: &CancelToken,
    ) -> Result<Rect, WindowError> {
        self.0.core.borrow_mut().platform.begin_bounds_change(window);
        let mut backend = Backend {
            shared: Rc::clone(&self.0),
            window,
        };
        let mut steps = Vec::new();
        let result = apply_settled(
            frame,
            work_area,
            &mut backend,
            &TokioClock::default(),
            cancel,
            &mut steps,
        )
        .await;
        let restore_delay = self.0.core.borrow_mut().platform.end_bounds_change(window);
        self.0.core.borrow_mut().diagnostics.last_steps = steps;
        if result.is_ok() && !restore_delay.is_zero() {
            // The app may move the window once it's restored; report where it ended up.
            tokio::time::sleep(restore_delay).await;
            return self.read(window);
        }
        result
    }
}

struct Backend<P> {
    shared: Rc<Shared<P>>,
    window: WindowId,
}

impl<P: Platform> BoundsBackend for Backend<P> {
    fn read_frame(&mut self) -> Result<Rect, WindowError> {
        self.shared.core.borrow_mut().platform.frame(self.window)
    }

    fn set_size(&mut self, size: Size) -> Result<(), WindowError> {
        self.shared.core.borrow_mut().platform.set_size(self.window, size)
    }

    fn set_position(&mut self, origin: Point) -> Result<(), WindowError> {
        self.shared
            .core
            .borrow_mut()
            .platform
            .set_position(self.window, origin)
    }
}

/// A snap to a zone, with what's needed to report it.
pub(crate) struct SnapRequest {
    pub window: WindowId,
    pub frame: Rect,
    /// Where undo returns: the frame before a drag started.
    pub original: Option<Rect>,
    pub work_area: Rect,
    pub drop: Option<DropRecord>,
}

pub(crate) async fn run_snap<P: Platform + 'static>(
    shared: Rc<Shared<P>>,
    cancel: CancelToken,
    request: SnapRequest,
) {
    let scope = shared.history_scope();
    let mut history = shared.take_history(&scope);
    let mut live = Live(Rc::clone(&shared));
    let result = operations::snap(
        &mut live,
        &mut history,
        request.window,
        request.frame,
        request.original,
        Some(request.work_area),
        &cancel,
    )
    .await;
    shared.return_history(scope, history);

    let mut core = shared.core.borrow_mut();
    let outcome = match &result {
        Ok(placement) if placement.constrained() => {
            core.announce_quietly("Snapped, limited by the app's window size.");
            "appConstrainedBounds"
        }
        Ok(_) => {
            core.announce_quietly("Snapped.");
            "snapped"
        }
        Err(WindowError::Cancelled) => "interruptedByNewInteraction",
        Err(error) => {
            core.announce_error(&error.to_string());
            "windowWriteFailed"
        }
    };
    let actual = result.as_ref().ok().map(|placement| placement.actual);
    let steps = debug_steps(&core.diagnostics.last_steps);
    core.log(level_of(outcome == "windowWriteFailed"), "snap", || {
        serde_json::json!({
            "window": request.window.0,
            "requested": request.frame,
            "actual": actual,
            "outcome": outcome,
            "steps": steps,
        })
    });
    if let Some(mut record) = request.drop {
        record.outcome = outcome.to_owned();
        record.requested = Some(request.frame);
        record.actual = result.as_ref().ok().map(|placement| placement.actual);
        record.steps = core.diagnostics.last_steps.iter().map(Step::from).collect();
        core.diagnostics.record_drop(record);
    }
}

/// A swap from a drag drop or a keyboard swap.
pub(crate) struct SwapMove {
    pub window: WindowId,
    /// The window's frame before the gesture.
    pub original: Rect,
    /// Where the occupant goes, before `plan_swap` settles a floating window's fallback.
    pub source: SwapSource,
    pub destination: ZoneSlot,
    /// Drag drops record diagnostics.
    pub record: Option<DropRecord>,
}

pub(crate) async fn run_swap<P: Platform + 'static>(
    shared: Rc<Shared<P>>,
    cancel: CancelToken,
    swap: SwapMove,
) {
    let occupant = match discover_and_plan(&shared, &swap) {
        Ok(SwapPlan::Snap) => {
            // Nothing to trade places with: an ordinary snap, reported like one.
            let request = SnapRequest {
                window: swap.window,
                frame: swap.destination.frame,
                original: Some(swap.original),
                work_area: swap.destination.work_area,
                drop: swap.record,
            };
            return run_snap(shared, cancel, request).await;
        }
        Ok(SwapPlan::Exchange { occupant, source }) => Ok((occupant, source)),
        Ok(SwapPlan::Blocked(block)) => Err(("swapBlocked", block.message().to_owned())),
        Err(error) => Err(("swapFailed", error.to_string())),
    };
    let scope = shared.history_scope();
    let mut history = shared.take_history(&scope);
    let protected = |window: WindowId| shared.core.borrow().protected.contains(&window);

    let mut source_kind = swap.source.kind();
    let (outcome, failure): (&str, Option<String>) = match occupant {
        Err((outcome, message)) => (outcome, Some(message)),
        Ok((occupant, source)) => {
            source_kind = source.kind();
            shared.core.borrow_mut().op_windows = vec![swap.window, occupant];
            let request = SwapRequest {
                dragged: swap.window,
                occupant,
                source: &source,
                destination: &swap.destination,
                original: swap.original,
            };
            let mut live = Live(Rc::clone(&shared));
            let result = operations::swap(&mut live, &mut history, request, &cancel, &protected).await;
            match result {
                Ok(_) => {
                    shared.core.borrow_mut().announce_quietly("Windows swapped.");
                    ("swapped", None)
                }
                Err(WindowError::Cancelled) => ("swapInterrupted", None),
                Err(error) => ("swapFailed", Some(error.to_string())),
            }
        }
    };
    shared.return_history(scope, history);

    // A rejected drag goes back where it started, unless a new gesture holds it.
    let rejected = outcome == "swapBlocked" || outcome == "swapFailed";
    let moved = shared
        .core
        .borrow_mut()
        .platform
        .frame(swap.window)
        .is_ok_and(|frame| frame != swap.original);
    if rejected && moved && !protected(swap.window) && !cancel.is_cancelled() {
        let mut live = Live(Rc::clone(&shared));
        let area = Some(swap.source.work_area());
        let _ = live
            .settle(swap.window, swap.original, area, &CancelToken::default())
            .await;
    }

    let mut core = shared.core.borrow_mut();
    core.log(level_of(outcome == "swapFailed"), "swap", || {
        serde_json::json!({
            "window": swap.window.0,
            "destination": swap.destination.frame,
            "source": source_kind,
            "outcome": outcome,
            "reason": failure,
        })
    });
    if let Some(message) = &failure {
        core.announce_error(message);
    }
    if let Some(mut record) = swap.record {
        record.outcome = outcome.to_owned();
        record.cancellation = failure;
        record.requested = Some(swap.destination.frame);
        record.actual = core.platform.frame(swap.window).ok();
        core.diagnostics.record_drop(record);
    }
}

/// Find the windows filling the places involved, at the moment of the swap only.
fn discover_and_plan<P: Platform>(shared: &Shared<P>, swap: &SwapMove) -> Result<SwapPlan, WindowError> {
    let mut core = shared.core.borrow_mut();
    let mut frames = vec![swap.destination.frame];
    frames.extend(swap.source.zone().map(|slot| slot.frame));
    let found = core.platform.windows_occupying(&frames)?;
    let windows: Vec<WindowSnapshot> = found
        .iter()
        .filter(|window| !core.workspaces.is_inactive(window.id))
        .map(|window| WindowSnapshot {
            id: window.id,
            frame: window.frame,
        })
        .collect();
    Ok(plan_swap(swap.window, &swap.source, &swap.destination, &windows))
}

/// Undo the newest move in the current scope.
pub(crate) async fn run_undo<P: Platform + 'static>(shared: Rc<Shared<P>>, cancel: CancelToken) {
    run_replay(shared, Replay::Undo, cancel).await;
}

/// Redo the newest undone move in the current scope.
pub(crate) async fn run_redo<P: Platform + 'static>(shared: Rc<Shared<P>>, cancel: CancelToken) {
    run_replay(shared, Replay::Redo, cancel).await;
}

async fn run_replay<P: Platform + 'static>(shared: Rc<Shared<P>>, which: Replay, cancel: CancelToken) {
    let scope = shared.history_scope();
    let protected = |window: WindowId| shared.core.borrow().protected.contains(&window);
    let mut history = shared.take_history(&scope);
    shared.core.borrow_mut().op_windows = history.next_windows(which);
    let mut live = Live(Rc::clone(&shared));
    let result = match which {
        Replay::Undo => operations::undo(&mut live, &mut history, &cancel, &protected).await,
        Replay::Redo => operations::redo(&mut live, &mut history, &cancel, &protected).await,
    };
    shared.return_history(scope, history);
    let (event, done, message) = match which {
        Replay::Undo => ("undo", "undone", "Last live move undone."),
        Replay::Redo => ("redo", "redone", "Last live move redone."),
    };
    let mut core = shared.core.borrow_mut();
    match result {
        Ok(()) => {
            core.trace(event, || serde_json::json!({ "outcome": done }));
            core.announce(message, false, true);
        }
        Err(WindowError::Cancelled) => {}
        Err(error) => {
            core.flag(event, || serde_json::json!({ "error": error.to_string() }));
            core.announce_error(&error.to_string());
        }
    }
}

fn level_of(failed: bool) -> Level {
    if failed { Level::Warn } else { Level::Info }
}

/// Settle steps for the debug log: stage, frame, and elapsed milliseconds.
fn debug_steps(steps: &[snappy_core::bounds::BoundsStep]) -> serde_json::Value {
    steps
        .iter()
        .map(|step| serde_json::json!([step.stage, step.frame, (step.elapsed_ms * 10.0).round() / 10.0]))
        .collect()
}
