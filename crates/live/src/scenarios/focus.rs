//! Moving keyboard focus between windows and zones.

use snappy_core::geometry::Rect;
use snappy_core::placement::navigate::Direction;
use snappy_core::settings::NavigationShortcut;
use snappy_engine::LogEvent;

use super::Outcome;
use super::support::{Context, side_by_side, timed, two_zoned};
use crate::input::pause;

/// Three windows side by side and a fourth below the middle one. Focus goes right, down,
/// up, right, and left, each press starting from the window the one before focused, and
/// ends on the middle window, which the right zone's shortcut then moves. Other apps'
/// windows, such as a test VM's terminal, would be in the way, so this runs on the third
/// workspace, which holds none.
pub(super) fn focus_directions(context: &mut Context) -> Result<Outcome, String> {
    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let outcome = focus_around(context);
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    outcome
}

fn focus_around(context: &mut Context) -> Result<Outcome, String> {
    let ids = ["focus-a", "focus-b", "focus-c"];
    let mut frames = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        context.open(id, side_by_side(&context.display, index))?;
        frames.push(context.fixture.frame(id)?);
    }
    let middle = frames[1];
    let area = context.display.work_area;
    let below = Rect::new(middle.x, middle.max_y() + 40.0, middle.width, area.height * 0.25);
    context.open("focus-d", below)?;
    frames.push(context.fixture.frame("focus-d")?);
    context.focus("focus-a")?;

    let seen = context.logged(LogEvent::FocusMove).len();
    let presses = [
        NavigationShortcut::Focus(Direction::Right),
        NavigationShortcut::Focus(Direction::Down),
        NavigationShortcut::Focus(Direction::Up),
        NavigationShortcut::Focus(Direction::Right),
        NavigationShortcut::Focus(Direction::Left),
    ];
    context.begin();
    for shortcut in presses {
        let key = context.navigation_key(shortcut)?;
        context.input.chord(&key.combo(context.settings.snappy_key))?;
        pause(400);
    }
    let elapsed = context.elapsed_ms();
    let moves: Vec<(Option<u64>, Option<u64>)> = context
        .logged(LogEvent::FocusMove)
        .iter()
        .skip(seen)
        .map(|record| (record["from"].as_u64(), record["to"].as_u64()))
        .collect();
    if moves.len() != presses.len() {
        return Err(format!(
            "Snappy logged {} focus moves, not {}",
            moves.len(),
            presses.len()
        ));
    }
    let to: Vec<Option<u64>> = moves.iter().map(|(_, to)| *to).collect();
    if to.iter().any(Option::is_none) {
        return Err(format!("a focus move found no window: {moves:?}"));
    }
    let followed = moves.windows(2).all(|pair| pair[1].0 == pair[0].1);
    let back_to_middle =
        to[0] == to[2] && to[2] == to[4] && to[1] != to[0] && to[3] != to[0] && to[1] != to[3];
    if !followed || !back_to_middle {
        return Err(format!("focus went {moves:?}"));
    }

    context.press(&context.zone_key(1)?)?;
    context.wait_in("focus-b", context.zone(1)?)?;
    for (id, before) in ["focus-a", "focus-c", "focus-d"]
        .iter()
        .zip([frames[0], frames[2], frames[3]])
    {
        context.stayed(id, before)?;
    }
    Ok(timed(elapsed))
}

/// Windows in both zones and a third floating between them, a little lower, so it is the
/// nearest window by center both right of and below the left one. Focus goes right, left,
/// and down from the left window, then right and left from the floating one, and reaches
/// only the zoned windows; the right zone's shortcut then swaps the left window into it,
/// so focus really ended there. Runs on the third workspace, as `focus_directions` does.
pub(super) fn focus_zones(context: &mut Context) -> Result<Outcome, String> {
    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let outcome = focus_between_zones(context);
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    outcome
}

fn focus_between_zones(context: &mut Context) -> Result<Outcome, String> {
    let (left, right) = two_zoned(context, "zone-a", "zone-b")?;
    let area = context.display.work_area;
    let (width, height) = (area.width * 0.3, area.height * 0.3);
    let between = Rect::new(
        area.mid_x() - width / 2.0,
        area.mid_y() + area.height * 0.05 - height / 2.0,
        width,
        height,
    );
    let floating = context.open("zone-f", between)?;
    context.focus("zone-a")?;

    let seen = context.logged(LogEvent::FocusMove).len();
    let press = |context: &mut Context, shortcut| -> Result<(), String> {
        let key = context.navigation_key(shortcut)?;
        context.input.chord(&key.combo(context.settings.snappy_key))?;
        pause(400);
        Ok(())
    };
    context.begin();
    for shortcut in [
        NavigationShortcut::Focus(Direction::Right),
        NavigationShortcut::Focus(Direction::Left),
        NavigationShortcut::Focus(Direction::Down),
    ] {
        press(context, shortcut)?;
    }
    context.focus("zone-f")?;
    press(context, NavigationShortcut::Focus(Direction::Right))?;
    press(context, NavigationShortcut::Focus(Direction::Left))?;
    let elapsed = context.elapsed_ms();

    let moves: Vec<serde_json::Value> = context
        .logged(LogEvent::FocusMove)
        .into_iter()
        .skip(seen)
        .collect();
    let to: Vec<Option<u64>> = moves.iter().map(|record| record["to"].as_u64()).collect();
    let from: Vec<Option<u64>> = moves.iter().map(|record| record["from"].as_u64()).collect();
    let described = || format!("focus went {:?} from {:?}", to, from);
    if moves.len() != 5 {
        return Err(format!(
            "Snappy logged {} focus moves, not 5: {}",
            moves.len(),
            described()
        ));
    }
    if moves.iter().any(|record| record["mode"] != "zones") {
        return Err(format!("not every focus move went by zones: {moves:?}"));
    }
    let (a, b, f) = (from[0], to[0], from[3]);
    let distinct = a.is_some() && b.is_some() && f.is_some() && a != b && f != a && f != b;
    if !distinct || to != [b, a, None, b, a] {
        return Err(described());
    }

    context.press(&context.zone_key(1)?)?;
    context.wait_in("zone-a", right)?;
    context.wait_in("zone-b", left)?;
    context.stayed("zone-f", floating)?;
    Ok(timed(elapsed))
}
