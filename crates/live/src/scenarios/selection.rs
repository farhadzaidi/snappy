//! Selecting windows, taking them along, and placing them in empty zones.

use snappy_core::geometry::Rect;
use snappy_core::settings::{FocusShortcut, NavigationShortcut, WorkspaceStep};

use super::Outcome;
use super::support::{Context, floating, side_by_side, timed};
use crate::input::pause;
use crate::screen::screenshot;

/// A selected window is outlined, then comes along, where it was, when the Snappy key and
/// 3 switch to the third workspace.
pub(super) fn select_along_by_number(context: &mut Context) -> Result<Outcome, String> {
    context.open("along", floating(&context.display, 120.0))?;
    let before = context.fixture.frame("along")?;
    context.press(&context.navigation_key(FocusShortcut::SelectWindow)?)?;
    context.wait_outlines(true)?;
    screenshot(&context.out.join("selection_outline.png"));

    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let elapsed = context.elapsed_ms();
    pause(600);
    context.stayed("along", before)?;
    context.wait_outlines(false)?;

    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    context.wait_parked("along")?;
    Ok(timed(elapsed))
}

/// Two of three windows selected go along to the next workspace; the third stays behind.
pub(super) fn select_two_along_by_step(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["pair-a", "pair-b", "pair-left"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    context.select(&ids[..2])?;

    context.press(&context.step_key(WorkspaceStep::Next)?)?;
    context.wait_active("workspace-2")?;
    let elapsed = context.wait_parked("pair-left")?;
    pause(300);
    for (id, before) in ids.iter().zip(&frames).take(2) {
        context.stayed(id, *before)?;
    }
    context.wait_outlines(false)?;

    context.press(&context.step_key(WorkspaceStep::Previous)?)?;
    context.wait_active("workspace-1")?;
    context.wait_shown("pair-left", frames[2])?;
    for id in &ids[..2] {
        context.wait_parked(id)?;
    }
    Ok(timed(elapsed))
}

/// Pressing the select key again unselects the window: its outline goes, and it stays
/// behind at the next switch.
pub(super) fn unselect(context: &mut Context) -> Result<Outcome, String> {
    context.open("down", floating(&context.display, 160.0))?;
    let before = context.fixture.frame("down")?;
    let select = context.navigation_key(FocusShortcut::SelectWindow)?;
    context.press(&select)?;
    context.wait_outlines(true)?;
    context.press(&select)?;
    context.wait_outlines(false)?;

    context.press(&context.switch_key(2)?)?;
    let elapsed = context.wait_parked("down")?;
    context.wait_active("workspace-2")?;
    context.press(&context.switch_key(1)?)?;
    context.wait_shown("down", before)?;
    context.wait_active("workspace-1")?;
    Ok(timed(elapsed))
}

/// Two floating windows, selected, go into the two empty zones nearest them with one press
/// of the nearest-empty-zone key, and leave the selection.
pub(super) fn place_selection(context: &mut Context) -> Result<Outcome, String> {
    let area = context.display.work_area;
    let size = |x: f64, y: f64| {
        Rect::new(
            area.x + area.width * x,
            area.y + y,
            area.width * 0.3,
            area.height * 0.4,
        )
    };
    context.open("place-left", size(0.08, 100.0))?;
    context.open("place-right", size(0.6, 160.0))?;
    context.select(&["place-left", "place-right"])?;

    let seen = context.snaps().len();
    context.press(&context.navigation_key(NavigationShortcut::NearestEmpty)?)?;
    let left = context.wait_in("place-left", context.zone(0)?)?;
    let right = context.wait_in("place-right", context.zone(1)?)?;
    context.wait_outlines(false)?;
    pause(200);
    let outcomes: Vec<String> = context
        .snaps()
        .iter()
        .skip(seen)
        .map(|record| record["outcome"].as_str().unwrap_or_default().to_owned())
        .collect();
    if outcomes != ["snapped", "snapped"] {
        return Err(format!("Snappy logged the moves as {outcomes:?}"));
    }
    Ok(timed(left.max(right)))
}

/// With workspaces off, the select key still outlines windows, and the clear key takes
/// every outline away without moving anything. Restarts the daemon without workspaces,
/// then with the run's settings again.
pub(super) fn select_without_workspaces(context: &mut Context) -> Result<Outcome, String> {
    let mut without = context.settings.clone();
    without.workspaces.enabled = false;
    context.daemon.quit()?;
    context.daemon.write_settings(&without)?;
    context.daemon.start()?;
    let outcome = select_and_clear(context);
    context.close_all();
    context.daemon.quit()?;
    context.daemon.write_settings(context.settings)?;
    context.daemon.start()?;
    outcome
}

fn select_and_clear(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["solo-a", "solo-b"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    context.select(&ids)?;
    context.press(&context.navigation_key(FocusShortcut::ClearSelection)?)?;
    context.wait_outlines(false)?;
    let elapsed = context.elapsed_ms();
    for (id, before) in ids.iter().zip(frames) {
        context.stayed(id, before)?;
    }
    Ok(timed(elapsed))
}
