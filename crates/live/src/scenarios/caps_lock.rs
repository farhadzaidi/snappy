//! Caps Lock as the Snappy key.

use std::time::Instant;

use snappy_core::keys::{Mods, Platform};
use snappy_core::navigate::Direction;
use snappy_core::settings::Settings;

use super::support::{Context, SETTLE_TIMEOUT, floating, timed};
use super::{Outcome, PLATFORM};
use crate::input::pause;

/// How long a hand holds a key it taps.
const TAP_MS: u64 = 60;

/// With Caps Lock as the Snappy key, holding it and pressing → moves a window as Snappy + →
/// does, without turning Caps Lock on, and tapping it alone turns Caps Lock on, then off.
/// Restarts the daemon with the setting on and workspaces off, then with the run's settings,
/// after which a tap of Caps Lock turns it on and off as usual again.
pub(super) fn caps_lock_as_snappy_key(context: &mut Context) -> Result<Outcome, String> {
    let mut chosen = context.settings.clone();
    chosen.workspaces.enabled = false;
    chosen.caps_lock_as_snappy_key = true;
    context.daemon.quit()?;
    context.daemon.write_settings(&chosen)?;
    context.daemon.start()?;
    let outcome = hold_and_tap(context, &chosen);
    context.close_all();
    context.daemon.quit()?;
    context.daemon.write_settings(context.settings)?;
    context.daemon.start()?;
    let outcome = outcome?;
    usual_again(context)?;
    Ok(outcome)
}

fn hold_and_tap(context: &mut Context, chosen: &Settings) -> Result<Outcome, String> {
    if context.logged("capsLockHeld").is_empty() {
        let refused = context.logged("capsLockRefused");
        return Err(format!("the daemon didn't take Caps Lock: {refused:?}"));
    }
    if context.input.caps_lock_on()? {
        return Err("Caps Lock was on before the scenario".to_owned());
    }
    let to_right = chosen
        .navigation
        .get(Direction::Right)
        .filter(|chord| chord.mods.has_snappy_key() && chord.mods.others().is_empty())
        .ok_or("moving right has no plain Snappy key shortcut")?
        .code
        .clone();
    let (left, right) = (context.zone(0)?, context.zone(1)?);
    let to_left_zone = context.zone_key(0)?;

    context.open("caps-lock", floating(&context.display, 40.0))?;
    context.press(&to_left_zone)?;
    context.wait_in("caps-lock", left)?;

    context.begin();
    context.input.caps_lock(true)?;
    pause(TAP_MS);
    context.input.key(&to_right, Mods::NONE)?;
    pause(TAP_MS);
    context.input.caps_lock(false)?;
    let elapsed = context.wait_in("caps-lock", right)?;
    pause(200);
    if context.input.caps_lock_on()? {
        return Err("holding Caps Lock turned it on".to_owned());
    }

    tap(context)?;
    wait_caps_lock(context, true, "tapping Caps Lock alone")?;
    tap(context)?;
    wait_caps_lock(context, false, "tapping Caps Lock alone again")?;
    // The daemon logs a tap just after turning Caps Lock on or off.
    let started = Instant::now();
    while context.logged("capsLockTapped").len() != 2 {
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!(
                "the daemon logged {} taps, not 2",
                context.logged("capsLockTapped").len()
            ));
        }
        pause(20);
    }
    Ok(timed(elapsed))
}

/// With Caps Lock back to normal, a tap turns it on and off as the system's own key does.
/// macOS can only be checked for the remap being gone: posted keys join the event stream
/// past the point where macOS remaps keys, so they never pass through it.
fn usual_again(context: &mut Context) -> Result<(), String> {
    if PLATFORM == Platform::MacOs {
        return Ok(());
    }
    tap(context)?;
    wait_caps_lock(context, true, "Caps Lock with the setting off")?;
    tap(context)?;
    wait_caps_lock(context, false, "Caps Lock with the setting off, again")
}

fn tap(context: &mut Context) -> Result<(), String> {
    context.input.caps_lock(true)?;
    pause(TAP_MS);
    context.input.caps_lock(false)
}

fn wait_caps_lock(context: &mut Context, on: bool, after: &str) -> Result<(), String> {
    let started = Instant::now();
    while context.input.caps_lock_on()? != on {
        if started.elapsed() > SETTLE_TIMEOUT {
            let state = if on { "on" } else { "off" };
            return Err(format!("{after} didn't turn Caps Lock {state}"));
        }
        pause(20);
    }
    Ok(())
}
