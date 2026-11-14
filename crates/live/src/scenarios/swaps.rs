//! Moving a window into a zone another window fills.

use super::Outcome;
use super::support::{Context, describe, near, occupied_right_and_floating, timed, two_zoned};
use crate::input::pause;

pub(super) fn keyboard_swap(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = two_zoned(context, "ka", "kb")?;
    context.focus("ka")?;
    context.press(&context.zone_key(1)?)?;
    let elapsed = context.wait_in("ka", right)?;
    context.wait_in("kb", left)?;
    Ok(timed(elapsed))
}

pub(super) fn swap_drag(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = two_zoned(context, "da", "db")?;
    context.drag_window("da", right.center(), context.drag_mods())?;
    let elapsed = context.wait_in("da", right)?;
    context.wait_in("db", left)?;
    Ok(timed(elapsed))
}

/// With swapping the default, the swap key makes the drop cover the occupant instead.
pub(super) fn inverted_drag(context: &mut Context) -> Result<Outcome, String> {
    let (_, right) = two_zoned(context, "sa", "sb")?;
    let inverted = context.inverting_mods()?;
    context.drag_window("sa", right.center(), inverted)?;
    let elapsed = context.wait_in("sa", right)?;
    pause(400);
    let occupant = context.fixture.frame("sb")?;
    if !near(occupant, right) {
        return Err(format!(
            "the occupant moved to {} instead of staying under the dropped window",
            describe(occupant)
        ));
    }
    Ok(timed(elapsed))
}

/// A swap trades zones: the occupant goes to the zone under the floater's center.
pub(super) fn floating_swap(context: &mut Context) -> Result<Outcome, String> {
    let (right, _) = occupied_right_and_floating(context, "occupant", "floater", 0)?;
    context.drag_window("floater", right.center(), context.drag_mods())?;
    let elapsed = context.wait_in("floater", right)?;
    context.wait_in("occupant", context.zone(0)?)?;
    Ok(timed(elapsed))
}

pub(super) fn floating_keyboard_swap(context: &mut Context) -> Result<Outcome, String> {
    let (right, _) = occupied_right_and_floating(context, "kocc", "kfloat", 0)?;
    context.press(&context.zone_key(1)?)?;
    let elapsed = context.wait_in("kfloat", right)?;
    context.wait_in("kocc", context.zone(0)?)?;
    Ok(timed(elapsed))
}

/// When the zone under the floater is the destination itself, the occupant takes the
/// floater's exact frame instead.
pub(super) fn floating_swap_fallback(context: &mut Context) -> Result<Outcome, String> {
    let (right, before) = occupied_right_and_floating(context, "focc", "ffloat", 1)?;
    context.drag_window("ffloat", right.center(), context.drag_mods())?;
    let elapsed = context.wait_in("ffloat", right)?;
    context.wait_in("focc", before)?;
    Ok(timed(elapsed))
}
