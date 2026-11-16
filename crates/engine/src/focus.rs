//! The shortcuts that move keyboard focus to the next window in a direction, and the
//! brief outline on the window they focus.

use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use snappy_core::geometry::Point;
use snappy_core::placement::focus::focus_target;
use snappy_core::placement::navigate::Direction;
use snappy_core::window::WindowSnapshot;
use snappy_platform::{Platform, WindowId};

use crate::drag::zone_slots;
use crate::log_event::LogEvent;
use crate::ops::Shared;
use crate::state::EngineState;

/// How long a window that just got focus from a focus shortcut stays outlined.
const FOCUS_FLASH: Duration = Duration::from_millis(400);

/// The window a focus shortcut just focused, outlined in the focus style until its flash
/// ends, over any selection outline.
#[derive(Default)]
pub(crate) struct FocusFlash {
    pub window: Option<WindowId>,
    /// Only the latest flash's timer takes it away.
    generation: u64,
}

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
        core.trace(LogEvent::FocusMove, || {
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

fn pointer_display_center<P: Platform>(core: &mut EngineState<P>) -> Point {
    match core.display_under_pointer() {
        Some(display) => display.frame.center(),
        None => core.platform.pointer(),
    }
}

/// The windows focus can move to: visible, not minimized, on the active workspace, and not
/// a hidden tab, other than `focused`. They come in the order ties go: the most recently
/// focused first, then the rest in the order the platform lists them.
fn focus_candidates<P: Platform>(
    core: &mut EngineState<P>,
    focused: Option<WindowId>,
) -> Vec<WindowSnapshot> {
    let visible = core.platform.visible_windows(None);
    let mut candidates: Vec<WindowSnapshot> = Vec::new();
    for info in visible {
        if Some(info.id) == focused || core.workspaces.is_hidden_tab(info.id) {
            continue;
        }
        if core.is_on_screen(info.id) {
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
        core.focus_flash.window = Some(window);
        core.focus_flash.generation += 1;
        core.refresh_marks();
        core.focus_flash.generation
    };
    let shared = Rc::clone(shared);
    tokio::task::spawn_local(async move {
        tokio::time::sleep(FOCUS_FLASH).await;
        let mut core = shared.core.borrow_mut();
        if core.focus_flash.generation == generation {
            core.focus_flash.window = None;
            core.refresh_marks();
        }
    });
}

/// A flashed window that closed loses its outline at once.
pub(crate) fn window_closed<P: Platform>(core: &mut EngineState<P>, window: WindowId) {
    if core.focus_flash.window == Some(window) {
        core.focus_flash.window = None;
        core.refresh_marks();
    }
}
