//! Drag to snap: press, drag, drop, with the overlay shown while the modifier is held. A
//! drop on an occupied zone swaps or stacks as the settings say, and holding the swap key
//! too does the opposite.

use std::rc::Rc;

use snappy_core::drag::{CancelReason, DragIntent};
use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::Mods;
use snappy_core::layout::hit_test;
use snappy_core::swap::{SwapSource, ZoneSlot, source_slot};
use snappy_platform::{Caption, Platform, WindowId};

use crate::core::{Core, DragSession};
use crate::diagnostics::DropRecord;
use crate::ops::{Shared, SnapRequest, SwapMove, run_snap, run_swap};

pub(crate) fn pointer_down<P: Platform + 'static>(
    shared: &Rc<Shared<P>>,
    at: Point,
    mods: Mods,
    window: Option<WindowId>,
) {
    {
        let mut core = shared.core.borrow_mut();
        if !core.live {
            return;
        }
        protect_pressed_windows(&mut core, at, window);
    }
    shared.cancel_current();

    let mut core = shared.core.borrow_mut();
    core.set_preview(None);
    core.end_drag();
    let target = match window {
        Some(id) => core.platform.frame(id).map(|_| (id, String::new())),
        None => core.platform.window_at(at).map(|info| (info.id, info.app)),
    };
    let found = target.and_then(|(id, app)| Ok((id, app, core.platform.frame(id)?)));

    match found {
        Ok((id, app, frame)) => {
            if !core.protected.contains(&id) && core.op_in_flight {
                core.protected.push(id);
            }
            let modifier_held = mods.contains(drag_modifier(&core));
            core.drag = DragSession::Tracking {
                window: id,
                app,
                intent: DragIntent::new(frame, Some(at)),
                pointer: at,
                modifier_held,
                caption: None,
            };
            core.platform.set_drag_monitoring(true);
        }
        Err(error) => {
            core.drag = DragSession::Idle;
            if mods.contains(drag_modifier(&core)) {
                core.diagnostics.record_drop(DropRecord {
                    outcome: "targetLookupFailed".into(),
                    pointer: at,
                    cancellation: Some(error.to_string()),
                    ..DropRecord::default()
                });
            }
        }
    }
}

/// A press during a swap or grouped undo claims the windows under it.
fn protect_pressed_windows<P: Platform>(core: &mut Core<P>, at: Point, pressed: Option<WindowId>) {
    let claimed: Vec<WindowId> = core
        .op_windows
        .clone()
        .into_iter()
        .filter(|id| Some(*id) == pressed || core.platform.frame(*id).is_ok_and(|frame| frame.contains(at)))
        .collect();
    core.protected.extend(claimed);
}

/// The pointer moved or a modifier changed during a press.
pub(crate) fn pointer_moved<P: Platform>(shared: &Shared<P>, at: Point, mods: Mods) {
    let mut core = shared.core.borrow_mut();
    let DragSession::Tracking { window, .. } = &core.drag else {
        return;
    };
    let window = *window;
    let Ok(frame) = core.platform.frame(window) else {
        core.end_drag();
        return;
    };
    let held = mods.contains(drag_modifier(&core));
    if let DragSession::Tracking {
        intent,
        pointer,
        modifier_held,
        ..
    } = &mut core.drag
    {
        intent.observe(frame);
        *pointer = at;
        *modifier_held |= held;
    }
    let caption = show_zones(&mut core, at, mods);
    note_caption(&mut core, caption);
}

/// Shows the overlay for the drag, or hides it. Returns the caption it shows.
fn show_zones<P: Platform>(core: &mut Core<P>, at: Point, mods: Mods) -> Option<Caption> {
    let DragSession::Tracking { intent, .. } = &core.drag else {
        return None;
    };
    let held = mods.contains(drag_modifier(core));
    let original = intent.original();
    if !intent.permits_snap(held) {
        core.platform.hide();
        return None;
    }
    let Some(display) = core.display_at(at).cloned() else {
        core.platform.hide();
        return None;
    };
    let layout = core.settings.layout_for_display(&display.id).clone();
    if !core.settings.is_enabled(&layout) {
        core.platform.hide();
        return None;
    }

    let spacing = core.spacing(&display);
    let zone = hit_test(at, &layout, display.work_area, spacing).map(|zone| zone.id.clone());
    // Occupants are looked up only at the drop, so the setting's own action, which may
    // turn out to be a plain snap, goes unnamed. Only the swap key's reversal is named.
    let caption = reverses(core, mods).then(|| {
        if swaps(core, mods) {
            Caption::Swap
        } else {
            Caption::Stack
        }
    });
    let source = (caption == Some(Caption::Swap))
        .then(|| {
            let slots = zone_slots(core);
            source_slot(original, &slots)
                .filter(|slot| slot.display_id == display.id)
                .map(|slot| slot.zone_id.clone())
        })
        .flatten();
    let scene = core.scene(&layout, &display, zone.as_deref(), caption, source.as_deref());
    core.platform.show(&display.id, &scene);
    caption.filter(|_| zone.is_some())
}

/// Logs the hovered zone's caption when it changes.
fn note_caption<P: Platform>(core: &mut Core<P>, shown: Option<Caption>) {
    let DragSession::Tracking { caption, .. } = &mut core.drag else {
        return;
    };
    if std::mem::replace(caption, shown) != shown {
        core.trace(
            "overlayCaption",
            || serde_json::json!({ "caption": shown.map(Caption::text) }),
        );
    }
}

/// The button was released.
pub(crate) fn pointer_up<P: Platform + 'static>(shared: &Rc<Shared<P>>, at: Point, mods: Mods) {
    let mut core = shared.core.borrow_mut();
    let DragSession::Tracking {
        window,
        app,
        intent,
        modifier_held,
        ..
    } = std::mem::replace(&mut core.drag, DragSession::Idle)
    else {
        return;
    };
    let mut intent = intent;
    let held = mods.contains(drag_modifier(&core));
    let diagnosed = held || modifier_held;
    let mut record = DropRecord {
        timestamp: core.platform.now(),
        app: Some(app),
        original: Some(intent.original()),
        pointer: at,
        started_in_move_area: intent.started_in_move_area(),
        ..DropRecord::default()
    };

    let submission = plan_drop(&mut core, window, &mut intent, at, mods, &mut record);
    core.end_drag();
    match submission {
        Some(DropAction::Snap(request)) => {
            drop(core);
            shared.submit(move |shared, cancel| run_snap(shared, cancel, request));
        }
        Some(DropAction::Swap(swap)) => {
            drop(core);
            shared.submit(move |shared, cancel| run_swap(shared, cancel, swap));
        }
        None if diagnosed => core.diagnostics.record_drop(record),
        None => {}
    }
}

enum DropAction {
    Snap(SnapRequest),
    Swap(SwapMove),
}

/// Decide what the drop does, filling in the record's outcome when it does nothing.
fn plan_drop<P: Platform>(
    core: &mut Core<P>,
    window: WindowId,
    intent: &mut DragIntent,
    at: Point,
    mods: Mods,
    record: &mut DropRecord,
) -> Option<DropAction> {
    let held = mods.contains(drag_modifier(core));
    let Ok(current) = core.platform.frame(window) else {
        record.outcome = "windowUnavailable".into();
        return None;
    };
    record.before_drop = Some(current);
    intent.observe(current);
    if !intent.permits_snap(held) {
        record.cancellation = intent.cancellation().map(|reason| reason.as_str().to_owned());
        record.outcome = match (intent.cancellation(), held) {
            (Some(reason), _) => reason.as_str().to_owned(),
            (None, true) => "noWindowMovement".into(),
            (None, false) => "modifierReleased".into(),
        };
        return None;
    }
    let Some(display) = core.display_at(at).cloned() else {
        record.outcome = "outsideDisplay".into();
        return None;
    };
    let layout = core.settings.layout_for_display(&display.id).clone();
    if !core.settings.is_enabled(&layout) {
        record.outcome = "layoutDisabled".into();
        return None;
    }
    let spacing = core.spacing(&display);
    let Some(zone) = hit_test(at, &layout, display.work_area, spacing) else {
        record.outcome = "outsideZone".into();
        return None;
    };
    let frame = snappy_core::layout::zone_frame(zone.rect, display.work_area, spacing);

    if swaps(core, mods) {
        let destination = ZoneSlot {
            display_id: display.id.clone(),
            zone_id: zone.id.clone(),
            frame,
            work_area: display.work_area,
        };
        let original = intent.original();
        return Some(DropAction::Swap(SwapMove {
            window,
            original,
            source: swap_source(core, original),
            destination,
            record: Some(std::mem::take(record)),
        }));
    }
    Some(DropAction::Snap(SnapRequest {
        window,
        frame,
        original: Some(intent.original()),
        work_area: display.work_area,
        drop: Some(std::mem::take(record)),
    }))
}

/// Escape cancels the drag until the next press and the operation in flight.
pub(crate) fn escape<P: Platform + 'static>(shared: &Rc<Shared<P>>) {
    shared.cancel_current();
    let mut core = shared.core.borrow_mut();
    if let DragSession::Tracking { intent, .. } = &mut core.drag {
        intent.cancel(CancelReason::Escape);
    }
    core.platform.hide();
}

/// Where a window at `frame` swaps from: its zone, or its own frame.
pub(crate) fn swap_source<P: Platform>(core: &Core<P>, frame: Rect) -> SwapSource {
    let work_area = core.display_of(frame).map_or(frame, |display| display.work_area);
    SwapSource::find(frame, work_area, &zone_slots(core))
}

/// Every zone on every display, by each display's current enabled layout.
pub(crate) fn zone_slots<P: Platform>(core: &Core<P>) -> Vec<ZoneSlot> {
    core.displays
        .iter()
        .flat_map(|display| {
            let layout = core.settings.layout_for_display(&display.id);
            let frames = if core.settings.is_enabled(layout) {
                core.zone_frames(layout, display)
            } else {
                Vec::new()
            };
            layout.zones.iter().zip(frames).map(|(zone, frame)| ZoneSlot {
                display_id: display.id.clone(),
                zone_id: zone.id.clone(),
                frame,
                work_area: display.work_area,
            })
        })
        .collect()
}

fn drag_modifier<P>(core: &Core<P>) -> snappy_core::keys::Modifier {
    core.settings.snapping.drag_modifier.modifier()
}

/// True when `mods` hold the swap key, so the drop does the opposite of the setting.
fn reverses<P>(core: &Core<P>, mods: Mods) -> bool {
    core.settings
        .snapping
        .swap_modifier
        .is_held(mods, drag_modifier(core))
}

/// True when a drop with `mods` held swaps with the zone's occupant rather than stacking.
fn swaps<P>(core: &Core<P>, mods: Mods) -> bool {
    core.settings.snapping.when_taken.swaps(reverses(core, mods))
}
