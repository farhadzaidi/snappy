//! Switching, stepping, and sending between workspaces, restarting, and quitting.

use std::time::Duration;

use snappy_core::geometry::{Point, Rect, Size};
use snappy_core::settings::WorkspaceStep;
use snappy_core::workspace::{ParkingPolicy, parking_origin};

use super::Outcome;
use super::support::{Context, describe, floating, near, side_by_side, timed};
use crate::display_mode;
use crate::input::pause;

pub(super) fn workspace_switch(context: &mut Context) -> Result<Outcome, String> {
    let start = floating(&context.display, 140.0);
    let before = context.open("ws", start)?;
    let before = context.fixture.frame("ws").unwrap_or(before);
    context.press(&context.switch_key(3)?)?;
    let elapsed = context.wait_parked("ws")?;
    context.wait_active("workspace-3")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_shown("ws", before)?;
    context.wait_active("workspace-1")?;
    Ok(timed(elapsed))
}

pub(super) fn send_to_workspace(context: &mut Context) -> Result<Outcome, String> {
    context.open("sent", floating(&context.display, 200.0))?;
    let before = context.fixture.frame("sent")?;
    context.press(&context.send_key(2)?)?;
    let elapsed = context.wait_parked("sent")?;
    if context.active_workspace()? != "workspace-1" {
        return Err("sending switched workspaces".to_owned());
    }
    context.press(&context.switch_key(2)?)?;
    context.wait_shown("sent", before)?;
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_parked("sent")?;
    context.wait_active("workspace-1")?;
    Ok(timed(elapsed))
}

pub(super) fn send_and_switch(context: &mut Context) -> Result<Outcome, String> {
    context.open("follow", floating(&context.display, 220.0))?;
    let before = context.fixture.frame("follow")?;
    context.press(&context.send_and_switch_key(2)?)?;
    pause(600);
    let after = context.fixture.frame("follow")?;
    if context.is_parked(after) || !near(after, before) {
        return Err(format!("the window moved to {}", describe(after)));
    }
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_parked("follow")?;
    context.wait_active("workspace-1")?;
    Ok(Outcome::default())
}

/// Stopping the daemon the way installing a new build does, then starting it again, moves
/// nothing: the window sent to workspace 2 stays parked, the other stays on screen, the
/// first workspace stays active, and the new daemon switches both as before. Quitting
/// afterwards still brings the parked window back.
pub(super) fn restart_keeps_workspaces(context: &mut Context) -> Result<Outcome, String> {
    context.open("kept", side_by_side(&context.display, 0))?;
    let kept = context.fixture.frame("kept")?;
    context.open("away", side_by_side(&context.display, 2))?;
    let away = context.fixture.frame("away")?;
    context.press(&context.send_key(2)?)?;
    context.wait_parked("away")?;
    let parked = context.fixture.frame("away")?;

    let seen = context.logged("workspacesStarted").len();
    context.begin();
    context.daemon.restart()?;
    let elapsed = context.elapsed_ms();
    context.stayed("kept", kept)?;
    let still = context.fixture.frame("away")?;
    if !context.is_parked(still) || !near(still, parked) {
        return Err(format!(
            "the parked window moved to {} during the restart",
            describe(still)
        ));
    }
    if context.active_workspace()? != "workspace-1" {
        return Err("the restart changed the active workspace".to_owned());
    }
    let started = context.logged("workspacesStarted");
    let rejoined = started
        .get(seen..)
        .and_then(<[_]>::last)
        .map(|record| record["rejoined"]["workspace-2"].clone())
        .unwrap_or_default();
    if rejoined != 1 {
        return Err(format!(
            "Snappy logged {rejoined} windows rejoining workspace 2, not 1"
        ));
    }

    context.press(&context.switch_key(2)?)?;
    context.wait_shown("away", away)?;
    context.wait_parked("kept")?;
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_shown("kept", kept)?;
    context.wait_parked("away")?;
    context.wait_active("workspace-1")?;

    context.daemon.quit()?;
    context.wait_shown("away", away)?;
    context.daemon.start()?;
    Ok(timed(elapsed))
}

pub(super) fn quit_returns_windows(context: &mut Context) -> Result<Outcome, String> {
    context.open("quit", floating(&context.display, 240.0))?;
    let before = context.fixture.frame("quit")?;
    context.press(&context.send_key(2)?)?;
    context.wait_parked("quit")?;
    context.begin();
    context.daemon.quit()?;
    let elapsed = context.wait_shown("quit", before)?;
    context.daemon.start()?;
    Ok(timed(elapsed))
}

/// Shrinking the display while a window is parked, as waking with a display that comes back
/// smaller does, leaves the window parked at the smaller display's corner, and growing it
/// back parks it at the original corner, with no warning from Snappy either time. The
/// system moves windows while the displays change, which is when containment must hold
/// off. Only the macOS VM's display has another size to change to.
pub(super) fn display_change_keeps_windows_parked(context: &mut Context) -> Result<Outcome, String> {
    let (Some(original), Some(smaller)) = (display_mode::size(), display_mode::smaller()) else {
        return Ok(Outcome::skipped(
            "The sandbox's display has no other size to change to.",
        ));
    };
    let display = context.display.clone();
    let outcome = parked_through_display_changes(context, &[smaller, original]);
    let restored = display_mode::set(original);
    context.display = display;
    let outcome = outcome?;
    restored?;
    Ok(outcome)
}

/// How long the system and Snappy may take over a display change.
const DISPLAY_CHANGE_TIMEOUT: Duration = Duration::from_secs(10);

fn parked_through_display_changes(context: &mut Context, sizes: &[Size]) -> Result<Outcome, String> {
    context.open("resized", floating(&context.display, 160.0))?;
    let before = context.fixture.frame("resized")?;
    context.press(&context.send_key(2)?)?;
    context.wait_parked("resized")?;
    let warned = warnings(context).len();
    let updated = context.logged("displaysUpdated").len();

    context.begin();
    let mut slowest: f64 = 0.0;
    for size in sizes {
        display_mode::set(*size)?;
        let display = Rect::from_origin_size(Point::new(0.0, 0.0), *size);
        context.display.frame = display;
        let what = format!("the corner of the {}×{} display", size.width, size.height);
        let (_, elapsed) = context.wait_for_within("resized", &what, DISPLAY_CHANGE_TIMEOUT, |frame| {
            parking_origin(ParkingPolicy::CornerSliver, frame, &[display])
                .is_some_and(|corner| near(frame, frame.with_origin(corner)))
        })?;
        slowest = slowest.max(elapsed);
        // Anything still to come from the change lands meanwhile.
        pause(1500);
        let after = context.fixture.frame("resized")?;
        if !context.is_parked(after) {
            return Err(format!(
                "the window came back to {} after the change",
                describe(after)
            ));
        }
        context.begin();
    }

    let new_warnings = warnings(context).split_off(warned);
    if !new_warnings.is_empty() {
        return Err(format!("Snappy warned: {}", new_warnings.join(" | ")));
    }
    let updates = context.logged("displaysUpdated").split_off(updated);
    if let Some(failed) = updates.iter().find(|record| !record["error"].is_null()) {
        return Err(format!("a display update failed: {}", failed["error"]));
    }
    if updates.len() < sizes.len() {
        return Err(format!(
            "Snappy logged {} display updates for {} changes",
            updates.len(),
            sizes.len()
        ));
    }

    context.press(&context.switch_key(2)?)?;
    context.wait_shown("resized", before)?;
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_parked("resized")?;
    context.wait_active("workspace-1")?;
    Ok(timed(slowest))
}

/// The messages Snappy flagged as warnings for the user so far.
fn warnings(context: &Context) -> Vec<String> {
    context
        .logged("status")
        .iter()
        .filter(|record| record["level"] == "warn")
        .map(|record| record["message"].as_str().unwrap_or_default().to_owned())
        .collect()
}

pub(super) fn step_workspaces(context: &mut Context) -> Result<Outcome, String> {
    context.open("step", floating(&context.display, 280.0))?;
    let before = context.fixture.frame("step")?;
    context.press(&context.step_key(WorkspaceStep::Next)?)?;
    let elapsed = context.wait_parked("step")?;
    context.wait_active("workspace-2")?;
    context.press(&context.step_key(WorkspaceStep::Previous)?)?;
    context.wait_shown("step", before)?;
    context.wait_active("workspace-1")?;
    Ok(timed(elapsed))
}

/// Three windows focused in turn, then sent to the next workspace one press at a time:
/// each press sends the focused window and focus moves to the one used before it.
pub(super) fn send_and_stay_batch(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["batch-a", "batch-b", "batch-c"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    // A window's first focus can come before Snappy can enroll it, which leaves it out of
    // the focus order, so each is focused again once all are open.
    for id in ids {
        context.focus(id)?;
    }
    let send = context.step_key(WorkspaceStep::SendNext)?;
    let mut slowest: f64 = 0.0;
    for id in ids.iter().rev() {
        context.press(&send)?;
        slowest = slowest.max(context.wait_parked(id)?);
        if context.active_workspace()? != "workspace-1" {
            return Err("sending switched workspaces".to_owned());
        }
        pause(300);
    }
    context.press(&context.switch_key(2)?)?;
    for (id, before) in ids.iter().zip(&frames) {
        context.wait_shown(id, *before)?;
    }
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    for id in ids {
        context.wait_parked(id)?;
    }
    context.wait_active("workspace-1")?;
    Ok(timed(slowest))
}
