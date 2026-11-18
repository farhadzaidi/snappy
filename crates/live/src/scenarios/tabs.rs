//! Native tab groups taken along to another workspace, and told apart from windows left on
//! another desktop.

use snappy_core::geometry::{Point, Rect};
use snappy_core::placement::navigate::Direction;
use snappy_core::settings::{NavigationShortcut, WorkspaceStep};
use snappy_engine::LogEvent;
use snappy_platform::Display;

use super::Outcome;
use super::support::{Context, describe, floating, near, parked, shown, timed};
use crate::input::pause;
use crate::spaces;

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

/// Windows of one app, all the same size in a grid, as a user lays out terminals. One is
/// left on another desktop, and a group of native tabs then opens in its place. macOS lists
/// neither that window nor the group's hidden tab, but only the hidden tab joins the group:
/// focus reaches every window showing, and taken along to another workspace, the group
/// keeps its hidden tab where it is while the window on the other desktop parks with its
/// own workspace. Runs on the third workspace, which holds no other app's windows.
pub(super) fn tabs_beside_another_desktop(context: &mut Context) -> Result<Outcome, String> {
    if !cfg!(target_os = "macos") {
        return Ok(Outcome::skipped(
            "Only macOS makes each native tab a window of its own.",
        ));
    }
    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let outcome = grid_beside_another_desktop(context);
    let removed = spaces::remove_added();
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    let outcome = outcome?;
    removed?;
    Ok(outcome)
}

fn grid_beside_another_desktop(context: &mut Context) -> Result<Outcome, String> {
    let cells = grid_cells(&context.display);
    // Where a group of tabs opened from a window in the top left cell shows, which an app
    // may make taller than the window was to fit the tab bar.
    let single = context.open("measure-a", cells[0])?;
    context.open_tab("measure-b", "measure-a")?;
    pause(500);
    let group = context.fixture.frame("measure-b")?;
    context.fixture.close("measure-b")?;
    context.fixture.close("measure-a")?;
    pause(500);

    // Plain windows of the group's size: one exactly where the group will show, the
    // others in the next cells.
    let title_bar = single.height - cells[0].height;
    let sized = |at: Point| Rect::new(at.x, at.y, group.width, group.height - title_bar);
    let away = context.open("away", sized(group.origin()))?;
    if !near(away, group) {
        return Err(format!(
            "couldn't open a window where the group shows: {} for {}",
            describe(away),
            describe(group)
        ));
    }
    for (id, cell) in [("grid-a", cells[1]), ("grid-b", cells[2])] {
        context.open(id, sized(cell.origin()))?;
    }

    // Leave it on another desktop, as an app used on several desktops has windows there.
    spaces::open_new()?;
    context.fixture.move_to_active_space("away")?;
    spaces::go_to_first()?;
    pause(1000);
    if shown(context.fixture.pid(), group) {
        return Err("the window didn't stay on the other desktop".to_owned());
    }

    // A group of tabs opens in its place, where the window last showed.
    context.open("tabs-a", cells[0])?;
    context.open_tab("tabs-b", "tabs-a")?;
    pause(600);
    let tabbed = context.fixture.frame("tabs-b")?;
    if !near(tabbed, group) {
        return Err(format!(
            "the group shows at {}, not {}",
            describe(tabbed),
            describe(group)
        ));
    }
    let hidden = context.fixture.frame("tabs-a")?;

    // Focus goes right to one grid window, back, and down to the other.
    context.focus("tabs-b")?;
    let seen = context.logged(LogEvent::FocusMove).len();
    for direction in [Direction::Right, Direction::Left, Direction::Down] {
        let key = context.navigation_key(NavigationShortcut::Focus(direction))?;
        context.input.chord(&key.combo(context.settings.snappy_key))?;
        pause(400);
    }
    let to: Vec<Option<u64>> = context
        .logged(LogEvent::FocusMove)
        .iter()
        .skip(seen)
        .map(|record| record["to"].as_u64())
        .collect();
    let reached_all = matches!(to[..], [Some(right), Some(back), Some(down)]
        if right != back && down != back && down != right);
    if !reached_all {
        return Err(format!("focus went to {to:?}"));
    }

    context.select(&["tabs-b"])?;
    context.press(&context.switch_key(2)?)?;
    context.wait_active("workspace-2")?;
    let elapsed = context.elapsed_ms();
    pause(600);
    let taken_along = context.logged(LogEvent::SelectionTakenAlong);
    let tabs = taken_along
        .last()
        .map(|record| record["tabs"].clone())
        .unwrap_or_default();
    let along: Vec<usize> = tabs
        .as_object()
        .map(|groups| {
            groups
                .values()
                .map(|hidden| hidden.as_array().map_or(0, Vec::len))
                .collect()
        })
        .unwrap_or_default();
    if along != [1] {
        return Err(format!("the group didn't go along with one hidden tab: {tabs}"));
    }
    context.stayed("tabs-b", tabbed)?;
    let still = context.fixture.frame("tabs-a")?;
    if !near(still, hidden) {
        return Err(format!("the hidden tab moved to {}", describe(still)));
    }
    let display = context.display.frame;
    context.wait_for("away", "a parked position", |frame| parked(display, frame))?;
    for id in ["grid-a", "grid-b"] {
        context.wait_parked(id)?;
    }
    Ok(timed(elapsed))
}

/// Three cells of a two-by-two grid on the display, each a window's top left and content
/// size: top left, top right, and bottom left. A title bar and a tab bar fit in each cell
/// below the content.
fn grid_cells(display: &Display) -> [Rect; 3] {
    const GAP: f64 = 30.0;
    const BARS: f64 = 60.0;
    let area = display.work_area;
    let width = (area.width - 3.0 * GAP) / 2.0;
    let height = (area.height - 3.0 * GAP) / 2.0 - BARS;
    let cell = |column: f64, row: f64| {
        Rect::new(
            area.x + GAP + column * (width + GAP),
            area.y + GAP + row * (height + BARS + GAP),
            width,
            height,
        )
    };
    [cell(0.0, 0.0), cell(1.0, 0.0), cell(0.0, 1.0)]
}
