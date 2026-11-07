//! Global shortcut actions: zones and moving between zones (swapping with the occupant or
//! stacking on it, as the settings say), layout selection, undo and redo, and workspaces.

use std::rc::Rc;

use snappy_core::bindings::HotKeyAction;
use snappy_core::error::WindowError;
use snappy_core::geometry::Rect;
use snappy_core::keys::Combo;
use snappy_core::layout::{Zone, zone_frame};
use snappy_core::navigate::{Direction, destination};
use snappy_core::swap::ZoneSlot;
use snappy_platform::{Display, Platform, WindowInfo};

use crate::core::{Core, PendingNavigation};
use crate::drag::swap_source;
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
        HotKeyAction::MoveToPreviousWorkspace => workspaces::take_focused(shared, -1),
        HotKeyAction::MoveToNextWorkspace => workspaces::take_focused(shared, 1),
        HotKeyAction::SwitchWorkspace(target) => workspaces::request_switch(shared, &target, false),
        HotKeyAction::SendToWorkspace(target) => workspaces::send_focused(shared, &target, false),
        HotKeyAction::SendAndSwitch(target) => workspaces::send_focused(shared, &target, true),
        _ if !shared.core.borrow().live => {}
        HotKeyAction::UndoMove => shared.submit(run_undo),
        HotKeyAction::RedoMove => shared.submit(run_redo),
        HotKeyAction::SelectLayout(layout) => select_layout_under_pointer(shared, &layout),
        HotKeyAction::Navigate(direction) => move_focused(shared, Target::Direction(direction)),
        HotKeyAction::Zone => move_focused(shared, Target::Zone(combo)),
    }
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
