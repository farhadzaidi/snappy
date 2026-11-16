//! Choosing another Snappy key.

use snappy_core::keys::{Modifier, Mods, Os};
use snappy_core::placement::navigate::Direction;
use snappy_core::settings::{Settings, SettingsError};

use super::support::{Context, LAYOUT, floating, timed};
use super::{Outcome, PLATFORM};

/// A Snappy key other than the default that each sandbox leaves free with the arrows:
/// Option+Command on macOS, whose run chords all hold Control; Ctrl+Alt on Windows; and
/// Ctrl+Shift on Linux, where Xfwm4 and GNOME Shell switch desktops with Ctrl+Alt and the
/// arrows, and GNOME moves windows between displays with Shift+Super and them.
pub fn free_snappy_key(platform: Os) -> Mods {
    let held: &[Modifier] = match platform {
        Os::MacOs => &[Modifier::Alt, Modifier::Meta],
        Os::Windows => &[Modifier::Ctrl, Modifier::Alt],
        Os::Linux => &[Modifier::Ctrl, Modifier::Shift],
    };
    held.iter().copied().collect()
}

/// The run's settings with workspaces off and the Snappy key changed to `free_snappy_key`.
pub fn with_free_snappy_key(run: &Settings, platform: Os) -> Result<Settings, SettingsError> {
    let mut prepared = run.clone();
    prepared.workspaces.enabled = false;
    prepared.with_snappy_key(free_snappy_key(platform), platform)
}

/// With the Snappy key changed in the run's settings, the Snappy shortcuts move windows with
/// the new modifiers held. Restarts the daemon with the changed key, then with the run's
/// settings again.
pub(super) fn chosen_snappy_key(context: &mut Context) -> Result<Outcome, String> {
    let chosen = with_free_snappy_key(context.settings, PLATFORM)
        .map_err(|error| format!("the run's settings refuse the Snappy key: {error}"))?;
    context.daemon.quit()?;
    context.daemon.write_settings(&chosen)?;
    context.daemon.start()?;
    let outcome = arrows_with_snappy_key(context, &chosen);
    context.close_all();
    context.daemon.quit()?;
    context.daemon.write_settings(context.settings)?;
    context.daemon.start()?;
    outcome
}

fn arrows_with_snappy_key(context: &mut Context, chosen: &Settings) -> Result<Outcome, String> {
    let (left, right) = (context.zone(0)?, context.zone(1)?);
    let arrow = |direction: Direction| {
        chosen
            .navigation
            .get(direction)
            .filter(|chord| chord.mods.has_snappy_key())
            .map(|chord| chord.combo(chosen.snappy_key))
            .ok_or_else(|| format!("moving {direction:?} has no Snappy key shortcut"))
    };
    let (to_right, to_left) = (arrow(Direction::Right)?, arrow(Direction::Left)?);
    if to_right.mods != free_snappy_key(PLATFORM) {
        return Err(format!("Snappy + → would hold {:?}", to_right.mods));
    }

    // The run gives zones concrete chords, which keep their keys whatever the Snappy key.
    let halves = chosen
        .layout(LAYOUT)
        .ok_or("the settings have no halves layout")?;
    let to_zone = halves
        .zones
        .first()
        .and_then(|zone| chosen.options(halves).zone_shortcuts.get(&zone.id).cloned())
        .map(|chord| chord.combo(chosen.snappy_key))
        .ok_or("zone 1 has no shortcut")?;

    context.open("snappy-key", floating(&context.display, 20.0))?;
    context.begin();
    context.input.chord(&to_zone)?;
    context.wait_in("snappy-key", left)?;
    context.begin();
    context.input.chord(&to_right)?;
    let first = context.wait_in("snappy-key", right)?;
    context.begin();
    context.input.chord(&to_left)?;
    let second = context.wait_in("snappy-key", left)?;
    Ok(timed(first.max(second)))
}
