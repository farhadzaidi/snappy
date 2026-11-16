//! Native tab groups taken along to another workspace.

use snappy_core::geometry::Point;
use snappy_core::settings::{NavigationShortcut, WorkspaceStep};
use snappy_engine::LogEvent;

use super::Outcome;
use super::support::{Context, floating, timed};
use crate::input::pause;

/// A window with three native tabs is selected and taken along to the next workspace, then
/// two tabs are dragged out into windows of their own. All three windows stay with that
/// workspace: they park together on switching back and return together. Only macOS makes
/// each tab a window of its own, and so the bug this guards against is macOS's.
pub(super) fn tabs_along(context: &mut Context) -> Result<Outcome, String> {
    if !cfg!(target_os = "macos") {
        return Ok(Outcome::skipped(
            "Only macOS makes each native tab a window of its own.",
        ));
    }
    let outcome = tabs_along_and_out(context);
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    outcome
}

fn tabs_along_and_out(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["tab-a", "tab-b", "tab-c"];
    context.open(ids[0], floating(&context.display, 60.0))?;
    // Each new tab shows and takes focus, so Snappy meets every tab.
    for id in &ids[1..] {
        context.open_tab(id, ids[0])?;
        pause(500);
    }
    let group = context.fixture.frame("tab-c")?;
    context.press(&context.navigation_key(NavigationShortcut::SelectWindow)?)?;
    context.wait_outlines(true)?;

    context.press(&context.step_key(WorkspaceStep::Next)?)?;
    context.wait_active("workspace-2")?;
    let elapsed = context.elapsed_ms();
    pause(600);
    context.stayed("tab-c", group)?;
    let taken_along = context.logged(LogEvent::SelectionTakenAlong);
    let tabs = taken_along
        .last()
        .map(|record| record["tabs"].clone())
        .unwrap_or_default();
    let along = tabs
        .as_object()
        .and_then(|groups| groups.values().next())
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    if along != 2 {
        return Err(format!("the hidden tabs didn't go along: {tabs}"));
    }

    let area = context.display.work_area;
    let spots = [
        Point::new(area.x + 40.0, area.max_y() - group.height - 40.0),
        Point::new(
            area.max_x() - group.width - 40.0,
            area.max_y() - group.height - 40.0,
        ),
    ];
    for (id, at) in ids.iter().zip(spots) {
        context.fixture.tear_out(id, at)?;
        pause(600);
    }
    let mut frames = Vec::new();
    for id in ids {
        let frame = context.fixture.frame(id)?;
        if context.is_parked(frame) {
            return Err(format!("{id} isn't on screen after the tabs came apart"));
        }
        frames.push(frame);
    }

    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    for id in ids {
        context.wait_parked(id)?;
    }
    context.press(&context.switch_key(2)?)?;
    context.wait_active("workspace-2")?;
    for (id, frame) in ids.iter().zip(frames) {
        context.wait_shown(id, frame)?;
    }
    Ok(timed(elapsed))
}
