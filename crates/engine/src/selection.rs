//! The selection: selecting windows, and placing the selected windows into empty zones.
//! Workspace switches and sends take the selection along (`workspaces`).

use std::collections::BTreeMap;
use std::rc::Rc;

use snappy_core::bounds::{BoundsStep, CancelToken};
use snappy_core::empty_zone::place_selection;
use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_core::operations::{self, GroupMove, WindowOps};
use snappy_core::window::WindowSnapshot;
use snappy_platform::{Platform, WindowId};

use crate::core::Core;
use crate::drag::zone_slots;
use crate::ops::{Live, Shared, log_snap};
use crate::workspaces;

/// The selected windows, in the order they were selected. Kept in memory only: quitting
/// forgets them and moves nothing.
#[derive(Default)]
pub(crate) struct Selection {
    pub windows: Vec<WindowId>,
}

impl Selection {
    pub fn contains(&self, window: WindowId) -> bool {
        self.windows.contains(&window)
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

// MARK: Selecting

/// Add the focused window to the selection, or take it out when it is already in it. A
/// tab group is selected as one: a tab of a selected group counts as selected.
pub(crate) fn toggle<P: Platform>(core: &mut Core<P>) {
    prune(core);
    let focused = match core.platform.focused_window() {
        Ok(info) => info,
        Err(error) => {
            let reason = error.to_string();
            core.trace("selectionSkipped", || serde_json::json!({ "reason": reason }));
            core.announce_quietly("Focus a window to select it.");
            return;
        }
    };
    let group: Vec<WindowId> = core
        .selection
        .windows
        .iter()
        .copied()
        .filter(|window| *window == focused.id || core.workspaces.tab_root(*window) == focused.id)
        .collect();
    if !group.is_empty() {
        for window in group {
            remove(core, window, "pressed again");
        }
        return;
    }
    if !workspaces::shown_on_active_workspace(core, &focused) {
        core.trace(
            "selectionSkipped",
            || serde_json::json!({ "window": focused.id.0, "app": focused.app, "reason": "not on the current workspace" }),
        );
        core.announce_quietly("Focus a window on the current workspace first.");
        return;
    }
    core.selection.windows.push(focused.id);
    let selected = core.selection.windows.len();
    core.trace(
        "selectionAdded",
        || serde_json::json!({ "window": focused.id.0, "app": focused.app, "selected": selected }),
    );
    core.refresh_marks();
}

/// Take `window` out of the selection.
pub(crate) fn remove<P: Platform>(core: &mut Core<P>, window: WindowId, reason: &str) {
    if !core.selection.contains(window) {
        return;
    }
    core.selection.windows.retain(|selected| *selected != window);
    let selected = core.selection.windows.len();
    core.trace(
        "selectionRemoved",
        || serde_json::json!({ "window": window.0, "reason": reason, "selected": selected }),
    );
    core.refresh_marks();
}

/// Empty the selection.
pub(crate) fn clear<P: Platform>(core: &mut Core<P>, reason: &str) {
    if core.selection.is_empty() {
        return;
    }
    let windows: Vec<u64> = core.selection.windows.drain(..).map(|window| window.0).collect();
    core.trace(
        "selectionCleared",
        || serde_json::json!({ "windows": windows, "reason": reason }),
    );
    core.refresh_marks();
}

/// Forget selected windows that closed without the platform saying so, as when their app
/// quit.
pub(crate) fn prune<P: Platform>(core: &mut Core<P>) {
    let closed: Vec<WindowId> = core
        .selection
        .windows
        .iter()
        .copied()
        .filter(|window| core.platform.is_closed(*window))
        .collect();
    for window in closed {
        remove(core, window, "closed");
    }
}

/// Prune closed windows, then say whether any are selected.
pub(crate) fn has_windows<P: Platform>(core: &mut Core<P>) -> bool {
    prune(core);
    !core.selection.is_empty()
}

/// Take out every selected window that stands for one of `groups`, by its tab group's
/// shown tab.
pub(crate) fn remove_groups<P: Platform>(core: &mut Core<P>, groups: &[WindowId], reason: &str) {
    let gone: Vec<WindowId> = core
        .selection
        .windows
        .iter()
        .copied()
        .filter(|window| groups.contains(&core.workspaces.tab_root(*window)))
        .collect();
    for window in gone {
        remove(core, window, reason);
    }
}

// MARK: Placing

/// Moves for the selected windows that aren't in a zone yet, and what can't move.
struct PlacementPlan {
    moves: Vec<GroupMove>,
    in_zones: Vec<WindowId>,
    left_over: Vec<WindowId>,
}

/// Move the selected windows that aren't in a zone into empty zones, as one step of undo.
/// Occupants are looked up once the operation runs, after any move in flight has stopped.
pub(crate) fn place<P: Platform + 'static>(shared: &Rc<Shared<P>>) {
    shared.submit(move |shared, cancel| async move {
        let planned = plan_placement(&mut shared.core.borrow_mut());
        match planned {
            Ok(Some(plan)) => run_placement(shared, cancel, plan).await,
            Ok(None) => {}
            Err(error) => shared.core.borrow_mut().announce_error(&error.to_string()),
        }
    });
}

fn plan_placement<P: Platform>(core: &mut Core<P>) -> Result<Option<PlacementPlan>, WindowError> {
    prune(core);
    let mut selected: Vec<WindowSnapshot> = Vec::new();
    // A selected tab that its app has since hidden moves its group's shown tab.
    for window in core.workspaces.tab_roots(&core.selection.windows) {
        if let (true, Ok(frame)) = (core.is_on_screen(window), core.platform.frame(window)) {
            selected.push(WindowSnapshot { id: window, frame });
        }
    }
    let slots = zone_slots(core);
    let frames: Vec<Rect> = slots.iter().map(|slot| slot.frame).collect();
    let windows = core.occupants(&frames)?;
    let first_display = core
        .display_under_pointer()
        .map(|display| display.id.clone())
        .unwrap_or_default();
    let plan = place_selection(&selected, &first_display, &slots, &windows);

    if plan.moves.is_empty() {
        let message = if plan.left_over.is_empty() {
            "The selected windows are already in zones."
        } else {
            "No empty zone for the selected windows."
        };
        core.announce_quietly(message);
        return Ok(None);
    }
    Ok(Some(PlacementPlan {
        moves: plan
            .moves
            .iter()
            .map(|(window, slot)| GroupMove {
                window: *window,
                frame: slot.frame,
                work_area: slot.work_area,
            })
            .collect(),
        in_zones: plan.in_zones,
        left_over: plan.left_over,
    }))
}

async fn run_placement<P: Platform + 'static>(
    shared: Rc<Shared<P>>,
    cancel: CancelToken,
    plan: PlacementPlan,
) {
    let scope = shared.history_scope();
    let mut history = shared.take_history(&scope);
    shared.core.borrow_mut().op_windows = plan.moves.iter().map(|planned| planned.window).collect();
    let mut stepped = Stepped {
        live: Live(Rc::clone(&shared)),
        steps: BTreeMap::new(),
    };
    let outcomes = operations::snap_group(&mut stepped, &mut history, &plan.moves, &cancel).await;
    shared.return_history(scope, history);

    let mut core = shared.core.borrow_mut();
    let mut placed = Vec::new();
    let mut failure = None;
    for (planned, outcome) in plan.moves.iter().zip(&outcomes) {
        let steps = stepped.steps.remove(&planned.window).unwrap_or_default();
        log_snap(&mut core, planned.window, planned.frame, outcome, &steps);
        match outcome {
            Ok(_) => placed.push(planned.window),
            Err(WindowError::Cancelled) => {}
            Err(error) => failure = failure.or_else(|| Some(error.to_string())),
        }
    }
    remove_groups(&mut core, &placed, "placed");
    let (placed_ids, left_over, in_zones) = (
        placed.iter().map(|window| window.0).collect::<Vec<_>>(),
        plan.left_over.iter().map(|window| window.0).collect::<Vec<_>>(),
        plan.in_zones.iter().map(|window| window.0).collect::<Vec<_>>(),
    );
    core.trace(
        "selectionPlaced",
        || serde_json::json!({ "placed": placed_ids, "leftOver": left_over, "inZones": in_zones }),
    );
    match (failure, plan.left_over.len(), placed.len()) {
        (Some(message), _, _) => core.announce_error(&message),
        (None, 0, 0) => {}
        (None, 0, 1) => core.announce_quietly("Snapped."),
        (None, 0, count) => core.announce_quietly(&format!("Snapped {count} windows.")),
        (None, 1, _) => core.announce_quietly("1 window didn't fit."),
        (None, count, _) => core.announce_quietly(&format!("{count} windows didn't fit.")),
    }
}

/// Window access that keeps the settle steps of each window's last move, so every snap of
/// a group is logged with its own.
struct Stepped<P> {
    live: Live<P>,
    steps: BTreeMap<WindowId, Vec<BoundsStep>>,
}

impl<P: Platform + 'static> WindowOps for Stepped<P> {
    fn read(&mut self, window: WindowId) -> Result<Rect, WindowError> {
        self.live.read(window)
    }

    async fn settle(
        &mut self,
        window: WindowId,
        frame: Rect,
        work_area: Option<Rect>,
        cancel: &CancelToken,
    ) -> Result<Rect, WindowError> {
        let result = self.live.settle(window, frame, work_area, cancel).await;
        let steps = self.live.0.core.borrow().diagnostics.last_steps.clone();
        self.steps.insert(window, steps);
        result
    }
}
