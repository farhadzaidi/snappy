//! Keyboard focus and the selection: moving focus to the next window in a direction,
//! selecting windows, and placing the selected windows into empty zones. Workspace
//! switches and sends take the selection along (`workspaces`).

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use snappy_core::bounds::{BoundsStep, CancelToken};
use snappy_core::empty_zone::place_selection;
use snappy_core::error::WindowError;
use snappy_core::focus::focus_target;
use snappy_core::geometry::{Point, Rect};
use snappy_core::navigate::Direction;
use snappy_core::operations::{self, GroupMove, Placement, WindowOps};
use snappy_core::window::WindowSnapshot;
use snappy_platform::{Platform, WindowId};

use crate::core::Core;
use crate::drag::zone_slots;
use crate::ops::{Live, Shared, debug_steps};
use crate::workspaces;

/// How long a window that just got focus from a focus shortcut stays outlined.
const FOCUS_FLASH: Duration = Duration::from_millis(400);

/// The selected windows, and the window briefly outlined after a focus shortcut. Kept in
/// memory only: quitting forgets them and moves nothing.
#[derive(Default)]
pub(crate) struct Selection {
    /// In the order they were selected.
    pub windows: Vec<WindowId>,
    /// Outlined in the focus style until its flash ends, over its selection outline.
    pub flash: Option<WindowId>,
    /// Only the latest flash's timer takes it away.
    flash_generation: u64,
}

impl Selection {
    pub fn contains(&self, window: WindowId) -> bool {
        self.windows.contains(&window)
    }

    pub fn is_empty(&self) -> bool {
        self.windows.is_empty()
    }
}

// MARK: Focus

/// Focus and raise the next window in `direction` from the focused window, or from the
/// middle of the display under the pointer when nothing is focused, and outline it briefly.
/// While any window is in a zone, focus moves between zones (`focus_target`).
pub(crate) fn focus_toward<P: Platform + 'static>(shared: &Rc<Shared<P>>, direction: Direction) {
    let target = {
        let mut core = shared.core.borrow_mut();
        let focused = core.platform.focused_window().ok();
        let focused_frame = focused
            .as_ref()
            .map(|window| core.platform.frame(window.id).unwrap_or(window.frame));
        let from = match focused_frame {
            Some(frame) => frame.center(),
            None => pointer_display_center(&mut core),
        };
        // A window of a workspace that isn't showing never counts as in a zone.
        let zoned_frame = focused
            .as_ref()
            .filter(|window| !core.workspaces.is_inactive(window.id))
            .and(focused_frame);
        let candidates = focus_candidates(&mut core, focused.as_ref().map(|window| window.id));
        let slots = zone_slots(&core);
        let choice = focus_target(zoned_frame, from, direction, &slots, &candidates);
        let target = choice.window;
        let from_window = focused.as_ref().map(|window| window.id.0);
        let to_window = target.map(|window| window.0);
        let considered = candidates.len();
        let mode = choice.mode.name();
        let zone = choice
            .zone
            .map(|slot| serde_json::json!({ "display": slot.display_id, "zone": slot.zone_id }));
        core.trace("focusMove", || {
            serde_json::json!({
                "direction": direction.name(),
                "from": from_window,
                "to": to_window,
                "candidates": considered,
                "mode": mode,
                "zone": zone,
            })
        });
        match target {
            Some(window) => core.platform.focus(window),
            None => core.announce_quietly(nothing_toward(direction)),
        }
        target
    };
    if let Some(window) = target {
        flash(shared, window);
    }
}

fn nothing_toward(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "No window above.",
        Direction::Down => "No window below.",
        Direction::Left => "No window to the left.",
        Direction::Right => "No window to the right.",
    }
}

fn pointer_display_center<P: Platform>(core: &mut Core<P>) -> Point {
    let pointer = core.platform.pointer();
    core.display_at(pointer)
        .or_else(|| core.displays.first())
        .map_or(pointer, |display| display.frame.center())
}

/// The windows focus can move to: visible, not minimized, on the active workspace, and not
/// a hidden tab, other than `focused`. They come in the order ties go: the most recently
/// focused first, then the rest in the order the platform lists them.
fn focus_candidates<P: Platform>(core: &mut Core<P>, focused: Option<WindowId>) -> Vec<WindowSnapshot> {
    let visible = core.platform.visible_windows(None);
    let mut candidates: Vec<WindowSnapshot> = Vec::new();
    for info in visible {
        let hidden_tab = core.workspaces.is_hidden_tab(info.id);
        if Some(info.id) == focused || core.workspaces.is_inactive(info.id) || hidden_tab {
            continue;
        }
        let shown = core
            .platform
            .state(info.id)
            .is_ok_and(|state| !state.minimized && !state.on_parking_desktop);
        if shown {
            candidates.push(WindowSnapshot {
                id: info.id,
                frame: info.frame,
            });
        }
    }
    let recency: BTreeMap<WindowId, usize> = core
        .workspaces
        .session
        .as_ref()
        .map(|session| {
            session
                .recently_focused()
                .enumerate()
                .map(|(rank, window)| (window, rank))
                .collect()
        })
        .unwrap_or_default();
    candidates.sort_by_key(|candidate| recency.get(&candidate.id).copied().unwrap_or(usize::MAX));
    candidates
}

/// Outline `window` in the focus style until `FOCUS_FLASH` has passed, unless a newer
/// flash replaces it first.
fn flash<P: Platform + 'static>(shared: &Rc<Shared<P>>, window: WindowId) {
    let generation = {
        let mut core = shared.core.borrow_mut();
        core.selection.flash = Some(window);
        core.selection.flash_generation += 1;
        core.refresh_marks();
        core.selection.flash_generation
    };
    let shared = Rc::clone(shared);
    tokio::task::spawn_local(async move {
        tokio::time::sleep(FOCUS_FLASH).await;
        let mut core = shared.core.borrow_mut();
        if core.selection.flash_generation == generation {
            core.selection.flash = None;
            core.refresh_marks();
        }
    });
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
            core.trace("selectSkipped", || serde_json::json!({ "reason": reason }));
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
            "selectSkipped",
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

pub(crate) fn window_closed<P: Platform>(core: &mut Core<P>, window: WindowId) {
    remove(core, window, "closed");
    if core.selection.flash == Some(window) {
        core.selection.flash = None;
        core.refresh_marks();
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
    for window in core.selection.windows.clone() {
        // A selected tab that its app has since hidden moves its group's shown tab.
        let window = core.workspaces.tab_root(window);
        if selected.iter().any(|snapshot| snapshot.id == window) {
            continue;
        }
        let shown = !core.workspaces.is_inactive(window)
            && core
                .platform
                .state(window)
                .is_ok_and(|state| !state.minimized && !state.on_parking_desktop);
        if let (true, Ok(frame)) = (shown, core.platform.frame(window)) {
            selected.push(WindowSnapshot { id: window, frame });
        }
    }
    let slots = zone_slots(core);
    let frames: Vec<Rect> = slots.iter().map(|slot| slot.frame).collect();
    let windows: Vec<WindowSnapshot> = core
        .platform
        .windows_occupying(&frames)?
        .iter()
        .map(|found| WindowSnapshot {
            id: found.id,
            frame: found.frame,
        })
        .collect();
    let pointer = core.platform.pointer();
    let first_display = core
        .display_at(pointer)
        .or_else(|| core.displays.first())
        .map(|display| display.id.clone())
        .unwrap_or_default();
    let workspaces = &core.workspaces;
    let plan = place_selection(&selected, &first_display, &slots, &windows, |id| {
        workspaces.is_inactive(id)
    });

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
        log_snap(&mut core, planned, outcome, &steps);
        match outcome {
            Ok(_) => placed.push(planned.window),
            Err(WindowError::Cancelled) => {}
            Err(error) => failure = failure.or_else(|| Some(error.to_string())),
        }
    }
    let gone: Vec<WindowId> = core
        .selection
        .windows
        .iter()
        .copied()
        .filter(|window| placed.contains(&core.workspaces.tab_root(*window)))
        .collect();
    for window in &gone {
        remove(&mut core, *window, "placed");
    }
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

/// A snap of a group, recorded like any other snap.
fn log_snap<P: Platform>(
    core: &mut Core<P>,
    planned: &GroupMove,
    outcome: &Result<Placement, WindowError>,
    steps: &[BoundsStep],
) {
    let (label, failed) = match outcome {
        Ok(placement) if placement.constrained() => ("appConstrainedBounds", false),
        Ok(_) => ("snapped", false),
        Err(WindowError::Cancelled) => ("interruptedByNewInteraction", false),
        Err(_) => ("windowWriteFailed", true),
    };
    let actual = outcome.as_ref().ok().map(|placement| placement.actual);
    let steps = debug_steps(steps);
    let details = || {
        serde_json::json!({
            "window": planned.window.0,
            "requested": planned.frame,
            "actual": actual,
            "outcome": label,
            "steps": steps,
        })
    };
    if failed {
        core.flag("snap", details);
    } else {
        core.trace("snap", details);
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
