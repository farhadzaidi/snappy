//! Global shortcut actions: zones and moving between zones (swapping with the occupant or
//! stacking on it, as the settings say), moving to the nearest empty zone, layout
//! selection, undo and redo, and workspaces.

use std::rc::Rc;

use snappy_core::bindings::HotKeyAction;
use snappy_core::empty_zone::nearest_empty_zone;
use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_core::keys::Combo;
use snappy_core::layout::{Zone, zone_frame};
use snappy_core::navigate::{Direction, destination};
use snappy_core::swap::ZoneSlot;
use snappy_core::window::WindowSnapshot;
use snappy_platform::{Display, Platform, WindowInfo};

use crate::core::{Core, PendingNavigation};
use crate::drag::{swap_source, zone_slots};
use crate::ops::{Shared, SnapRequest, SwapMove, run_redo, run_snap, run_swap, run_undo};
use crate::workspaces;

pub(crate) fn hotkey<P: Platform + 'static>(shared: &Rc<Shared<P>>, id: u32) {
    let (action, combo) = {
        let mut core = shared.core.borrow_mut();
        if core.recording || !core.settings.snapping.shortcuts_enabled {
            return;
        }
        let Some(hotkey) = core.registered.get(id as usize).cloned() else {
            return;
        };
        let live = core.live;
        core.trace(
            "shortcut",
            || serde_json::json!({ "action": format!("{:?}", hotkey.action), "live": live }),
        );
        (hotkey.action, hotkey.chord.combo())
    };

    match action {
        HotKeyAction::PreviousWorkspace => workspaces::cycle(shared, -1),
        HotKeyAction::NextWorkspace => workspaces::cycle(shared, 1),
        HotKeyAction::SendToPreviousWorkspace => workspaces::send_and_stay(shared, -1),
        HotKeyAction::SendToNextWorkspace => workspaces::send_and_stay(shared, 1),
        HotKeyAction::TakeAlong => workspaces::take_along(&mut shared.core.borrow_mut()),
        HotKeyAction::SwitchWorkspace(target) => workspaces::request_switch(shared, &target, false),
        HotKeyAction::SendToWorkspace(target) => workspaces::send_focused(shared, &target, false),
        HotKeyAction::SendAndSwitch(target) => workspaces::send_focused(shared, &target, true),
        _ if !shared.core.borrow().live => {}
        HotKeyAction::UndoMove => shared.submit(run_undo),
        HotKeyAction::RedoMove => shared.submit(run_redo),
        HotKeyAction::SelectLayout(layout) => select_layout_under_pointer(shared, &layout),
        HotKeyAction::Navigate(direction) => move_focused(shared, Target::Direction(direction)),
        HotKeyAction::Zone => move_focused(shared, Target::Zone(combo)),
        HotKeyAction::NearestEmptyZone => move_to_nearest_empty(shared),
    }
}

/// Snap the focused window into the nearest empty zone. Occupants are looked up once the
/// operation runs, after any move in flight has stopped, as a swap does.
fn move_to_nearest_empty<P: Platform + 'static>(shared: &Rc<Shared<P>>) {
    let focused = shared.core.borrow_mut().platform.focused_window();
    let window = match focused {
        Ok(window) => window,
        Err(error) => {
            shared.core.borrow_mut().announce_error(&error.to_string());
            return;
        }
    };
    shared.submit(move |shared, cancel| async move {
        let planned = nearest_empty_snap(&mut shared.core.borrow_mut(), &window);
        match planned {
            Ok(Some(request)) => run_snap(shared, cancel, request).await,
            Ok(None) => {}
            Err(error) => shared.core.borrow_mut().announce_error(&error.to_string()),
        }
    });
}

/// The snap into the empty zone nearest `window`, or `None` when there is none.
fn nearest_empty_snap<P: Platform>(
    core: &mut Core<P>,
    window: &WindowInfo,
) -> Result<Option<SnapRequest>, WindowError> {
    if core.workspaces.is_inactive(window.id) {
        return Ok(None);
    }
    let frame = core.platform.frame(window.id)?;
    let Some(display) = core.display_of(frame).cloned() else {
        return Ok(None);
    };
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
    let workspaces = &core.workspaces;
    let chosen = nearest_empty_zone(window.id, frame, &display.id, &slots, &windows, |id| {
        workspaces.is_inactive(id)
    });
    let Some(slot) = chosen else {
        core.announce_quietly("No empty zone on this display.");
        return Ok(None);
    };
    Ok(Some(SnapRequest {
        window: window.id,
        frame: slot.frame,
        original: None,
        work_area: slot.work_area,
        drop: None,
    }))
}

fn select_layout_under_pointer<P: Platform + 'static>(shared: &Rc<Shared<P>>, layout: &str) {
    let mut core = shared.core.borrow_mut();
    let pointer = core.platform.pointer();
    let display = core
        .display_at(pointer)
        .or_else(|| core.displays.first())
        .map(|display| display.id.clone());
    if let Some(display) = display {
        crate::commands::apply_layout(&mut core, &display, layout);
    }
}

/// Where a keyboard move goes, relative to the focused window.
enum Target {
    /// The zone whose shortcut is this combo.
    Zone(Combo),
    Direction(Direction),
}

/// A destination zone chosen for the focused window.
struct Planned {
    window: WindowInfo,
    frame: Rect,
    display: Display,
    zone: Zone,
    pending: Option<PendingNavigation>,
}

/// Move the focused window to the target zone, swapping with its occupant or stacking on
/// it as the settings say.
fn move_focused<P: Platform + 'static>(shared: &Rc<Shared<P>>, target: Target) {
    let planned = {
        let mut core = shared.core.borrow_mut();
        let planned = core
            .platform
            .focused_window()
            .and_then(|window| plan(&mut core, window, &target));
        match planned {
            Ok(Some(planned)) => planned,
            Ok(None) => return,
            Err(error) => {
                core.announce_error(&error.to_string());
                return;
            }
        }
    };

    let core = shared.core.borrow();
    let spacing = core.spacing(&planned.display);
    let work_area = planned.display.work_area;
    let zone_frame = zone_frame(planned.zone.rect, work_area, spacing);
    let pending = planned.pending;
    if core.settings.snapping.when_taken.swaps(false) {
        let swap = SwapMove {
            window: planned.window.id,
            original: planned.frame,
            source: swap_source(&core, planned.frame),
            destination: ZoneSlot {
                display_id: planned.display.id.clone(),
                zone_id: planned.zone.id.clone(),
                frame: zone_frame,
                work_area,
            },
            record: None,
        };
        drop(core);
        shared.submit(move |shared, cancel| async move {
            shared.core.borrow_mut().pending_navigation = pending;
            run_swap(shared, cancel, swap).await;
        });
    } else {
        let request = SnapRequest {
            window: planned.window.id,
            frame: zone_frame,
            original: None,
            work_area,
            drop: None,
        };
        drop(core);
        shared.submit(move |shared, cancel| async move {
            // A rapid repeat continues from here while this snap settles.
            shared.core.borrow_mut().pending_navigation = pending;
            run_snap(shared, cancel, request).await;
        });
    }
}

fn plan<P: Platform>(
    core: &mut Core<P>,
    window: WindowInfo,
    target: &Target,
) -> Result<Option<Planned>, WindowError> {
    let frame = core.platform.frame(window.id)?;
    let Some(display) = core.display_of(frame).cloned() else {
        return Ok(None);
    };
    let found = match target {
        Target::Zone(combo) => zone_by_chord(core, &display, combo).map(|zone| (zone, None)),
        Target::Direction(direction) => navigate(core, &window, frame, &display, *direction),
    };
    Ok(found.map(|(zone, pending)| Planned {
        window,
        frame,
        display,
        zone,
        pending,
    }))
}

/// The zone whose shortcut was pressed in the display's layout.
fn zone_by_chord<P: Platform>(core: &Core<P>, display: &Display, combo: &Combo) -> Option<Zone> {
    let layout = core.settings.layout_for_display(&display.id);
    let options = core.settings.options(layout);
    if !options.enabled {
        return None;
    }
    let pressed = |chord: &snappy_core::keys::KeyChord| &chord.combo() == combo;
    layout
        .zones
        .iter()
        .find(|zone| options.zone_shortcuts.get(&zone.id).is_some_and(pressed))
        .cloned()
}

/// The next zone in `direction`.
fn navigate<P: Platform>(
    core: &mut Core<P>,
    window: &WindowInfo,
    frame: Rect,
    display: &Display,
    direction: Direction,
) -> Option<(Zone, Option<PendingNavigation>)> {
    if core.workspaces.is_inactive(window.id) {
        return None;
    }
    let layout = core.settings.layout_for_display(&display.id).clone();
    if !core.settings.is_enabled(&layout) {
        return None;
    }
    let continuing = core
        .pending_navigation
        .as_ref()
        .filter(|pending| {
            pending.window == window.id && pending.layout == layout.id && pending.display == display.id
        })
        .map(|pending| pending.zone.clone());
    let spacing = core.spacing(display);
    let Some(zone) = destination(
        frame,
        &layout,
        display.work_area,
        spacing,
        direction,
        continuing.as_deref(),
    ) else {
        core.announce_quietly(&format!("No zone {} of this window.", direction.name()));
        return None;
    };
    let pending = PendingNavigation {
        window: window.id,
        layout: layout.id.clone(),
        display: display.id.clone(),
        zone: zone.id.clone(),
    };
    Some((zone.clone(), Some(pending)))
}
