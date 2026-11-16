//! Snapping by drag and by shortcut, and the windows Snappy leaves alone.

use snappy_core::geometry::{Rect, Size};
use snappy_core::navigate::Direction;
use snappy_core::settings::NavigationShortcut;

use super::Outcome;
use super::support::{
    Context, FRAME_TOLERANCE, UNWAITED_SNAP_MS, describe, floating, near, occupied_right_and_floating, timed,
};
use crate::input::pause;

pub(super) fn drag_snaps(context: &mut Context) -> Result<Outcome, String> {
    let left = context.zone(0)?;
    let start = floating(&context.display, 0.0);
    context.open("drag", start)?;
    context.drag_window("drag", left.center(), context.drag_mods())?;
    context.wait_in("drag", left).map(timed)
}

pub(super) fn zone_shortcut(context: &mut Context) -> Result<Outcome, String> {
    let right = context.zone(1)?;
    context.open("key", floating(&context.display, 20.0))?;
    context.press(&context.zone_key(1)?)?;
    context.wait_in("key", right).map(timed)
}

pub(super) fn arrow_shortcut(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = (context.zone(0)?, context.zone(1)?);
    context.open("arrow", floating(&context.display, 40.0))?;
    context.press(&context.zone_key(0)?)?;
    context.wait_in("arrow", left)?;
    context.press(&context.navigation_key(Direction::Right)?)?;
    context.wait_in("arrow", right).map(timed)
}

/// Terminals size in character cells, so they can't land on the exact zone size. The snap
/// must still finish at once instead of waiting out the settle schedule.
pub(super) fn grid_snap(context: &mut Context) -> Result<Outcome, String> {
    if !context.fixture.supports_size_steps() {
        return Ok(Outcome::skipped(
            "Wayland has no way for an app to ask to size in steps; the Xwayland run covers it",
        ));
    }
    let left = context.zone(0)?;
    let cell = Size::new(7.0, 17.0);
    context.open_with("grid", floating(&context.display, 100.0), None, Some(cell))?;
    let seen = context.snaps().len();
    context.press(&context.zone_key(0)?)?;
    let (frame, elapsed) = context.wait_for("grid", "the left zone's origin", |frame| {
        (frame.x - left.x).abs() <= FRAME_TOLERANCE && (frame.y - left.y).abs() <= FRAME_TOLERANCE
    })?;
    let off_by = Size::new(
        (frame.width - left.width).abs(),
        (frame.height - left.height).abs(),
    );
    if off_by.width > cell.width || off_by.height > cell.height {
        return Err(format!(
            "landed at {}, more than a cell from {}",
            describe(frame),
            describe(left)
        ));
    }
    let (_, settled) = context.finished_snap(seen)?;
    if settled > UNWAITED_SNAP_MS {
        return Err(format!("Snappy took {settled:.0} ms to settle, so it waited"));
    }
    Ok(Outcome {
        elapsed_ms: Some(elapsed),
        note: Some(format!(
            "Snappy settled in {settled:.1} ms at {}",
            describe(frame)
        )),
        ..Outcome::default()
    })
}

pub(super) fn minimum_size(context: &mut Context) -> Result<Outcome, String> {
    let left = context.zone(0)?;
    let min = Size::new(left.width + 120.0, 200.0);
    let start = Rect::new(
        context.display.work_area.x + 40.0,
        context.display.work_area.y + 60.0,
        min.width,
        300.0,
    );
    context.open_with("min", start, Some(min), None)?;
    let seen = context.snaps().len();
    context.press(&context.zone_key(0)?)?;
    let (outcome, _) = context.finished_snap(seen)?;
    if outcome != "appConstrainedBounds" {
        return Err(format!(
            "Snappy reported {outcome}, not a snap the app constrained"
        ));
    }
    let (frame, elapsed) = context.wait_for("min", "the left zone's top left", |frame| {
        (frame.x - left.x).abs() <= FRAME_TOLERANCE && (frame.y - left.y).abs() <= FRAME_TOLERANCE
    })?;
    if frame.width + FRAME_TOLERANCE < min.width || (frame.height - left.height).abs() > FRAME_TOLERANCE {
        return Err(format!(
            "{} should keep the minimum width {:.0} and fill the zone's height",
            describe(frame),
            min.width
        ));
    }
    Ok(timed(elapsed))
}

/// A floater over the occupied right zone goes to the left one, the nearest empty zone,
/// as an ordinary snap.
pub(super) fn nearest_empty_zone(context: &mut Context) -> Result<Outcome, String> {
    let (right, _) = occupied_right_and_floating(context, "nocc", "nfloat", 1)?;
    let left = context.zone(0)?;
    let seen = context.snaps().len();
    context.press(&context.navigation_key(NavigationShortcut::NearestEmptyZone)?)?;
    let elapsed = context.wait_in("nfloat", left)?;
    context.wait_in("nocc", right)?;
    let (outcome, _) = context.finished_snap(seen)?;
    if outcome != "snapped" {
        return Err(format!("Snappy logged the move as {outcome}, not a snap"));
    }
    Ok(timed(elapsed))
}

/// Full-screen windows are refused, not moved. Runs last: it leaves a full-screen Space.
pub(super) fn full_screen_refused(context: &mut Context) -> Result<Outcome, String> {
    context.open("full", floating(&context.display, 300.0))?;
    context.fixture.full_screen("full", true)?;
    pause(1500);
    let before = context.fixture.frame("full")?;
    let seen = context.snaps().len();
    context.press(&context.zone_key(0)?)?;
    pause(800);
    let after = context.fixture.frame("full")?;
    context.fixture.full_screen("full", false)?;
    pause(1500);
    if !near(after, before) {
        return Err(format!("the full-screen window moved to {}", describe(after)));
    }
    if context.snaps().len() > seen {
        return Err("Snappy tried to snap a full-screen window".to_owned());
    }
    Ok(Outcome::default())
}
