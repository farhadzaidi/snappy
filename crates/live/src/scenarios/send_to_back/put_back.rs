//! Holding the shortcut: the window sent back stays there while it's held, and letting go
//! puts every window back in its order, the window in front focused, however the keys come
//! up and with Caps Lock as the Snappy key.

use std::time::Instant;

use snappy_core::keys::Mods;
use snappy_engine::LogEvent;

use super::support::{
    BOTTOM_LEFT, BOTTOM_MIDDLE, FLOATING, STACK, StackWindow, TOP_LEFT, on_the_third_workspace,
    open_floating_over_three_zones, open_stack, send_to_back_combo, stack_order, wait_frontmost, wait_order,
};
use crate::fixture::Fixture;
use crate::input::pause;
use crate::scenarios::Outcome;
use crate::scenarios::support::{Context, SETTLE_TIMEOUT, timed};

/// Longer than a quick press, so the shortcut is held.
const HOLD_MS: u64 = 700;
/// How long a hand holds a key it presses and lets go at once, as in the Caps Lock
/// scenario.
const PRESS_MS: u64 = 60;

/// Holding the shortcut sends the focused window to the back as it's pressed, and it stays
/// there while held. Letting go puts every window back in the order it was, the window in
/// front, focused.
pub(crate) fn put_back_stack(context: &mut Context) -> Result<Outcome, String> {
    on_the_third_workspace(context, |context| {
        open_stack(context)?;
        let combo = send_to_back_combo(context)?;
        peek(
            context,
            |context| context.input.hold_chord(&combo),
            |context| context.input.let_go_of_chord(&combo),
        )
    })
}

/// Holding the shortcut, then letting go of the Snappy key before the key, still brings the
/// window back, and the next press is a quick one. Some desktops report the shortcut let go as the
/// Snappy key comes up and others as its key does; neither may leave the shortcut down, so
/// that the next press is taken for its key repeating.
pub(crate) fn put_back_snappy_key_first(context: &mut Context) -> Result<Outcome, String> {
    on_the_third_workspace(context, |context| {
        open_stack(context)?;
        let combo = send_to_back_combo(context)?;
        let let_go = |context: &mut Context| -> Result<(), String> {
            context.input.modifiers(combo.mods, false)?;
            pause(PRESS_MS);
            context.input.send_key(&combo.code, Mods::NONE, false)
        };
        let outcome = peek(context, |context| context.input.hold_chord(&combo), let_go)?;
        context.input.chord(&combo)?;
        wait_frontmost(context, "stacked-b")?;
        Ok(outcome)
    })
}

/// With Caps Lock as the Snappy key, Caps Lock and B press and hold as the shortcut does.
/// Restarts the daemon with the setting on, then with the run's settings.
pub(crate) fn caps_lock_send_to_back(context: &mut Context) -> Result<Outcome, String> {
    let mut chosen = context.settings.clone();
    chosen.caps_lock_as_snappy_key = true;
    context.daemon.quit()?;
    context.daemon.write_settings(&chosen)?;
    context.daemon.start()?;
    let outcome = on_the_third_workspace(context, caps_lock_press_and_hold);
    context.close_all();
    context.daemon.quit()?;
    context.daemon.write_settings(context.settings)?;
    context.daemon.start()?;
    outcome
}

fn caps_lock_press_and_hold(context: &mut Context) -> Result<Outcome, String> {
    if context.logged(LogEvent::CapsLockHeld).is_empty() {
        let refused = context.logged(LogEvent::CapsLockRefused);
        return Err(format!("the daemon didn't take Caps Lock: {refused:?}"));
    }
    open_stack(context)?;
    let key = send_to_back_combo(context)?.code;
    context.begin();
    context.input.caps_lock(true)?;
    pause(PRESS_MS);
    context.input.key(&key, Mods::NONE)?;
    pause(PRESS_MS);
    context.input.caps_lock(false)?;
    wait_frontmost(context, "stacked-b")?;
    let elapsed = context.elapsed_ms();
    context.focus("stacked-a")?;
    wait_frontmost(context, "stacked-a")?;

    let hold = |context: &mut Context| -> Result<(), String> {
        context.input.caps_lock(true)?;
        pause(PRESS_MS);
        context.input.send_key(&key, Mods::NONE, true)
    };
    let let_go = |context: &mut Context| -> Result<(), String> {
        context.input.send_key(&key, Mods::NONE, false)?;
        pause(PRESS_MS);
        context.input.caps_lock(false)
    };
    peek(context, hold, let_go)?;
    pause(200);
    if context.input.caps_lock_on()? {
        return Err("holding Caps Lock turned it on".to_owned());
    }

    // Caps Lock let go before B: the window still goes back, and the next press is still
    // a quick one.
    let caps_lock_first = |context: &mut Context| -> Result<(), String> {
        context.input.caps_lock(false)?;
        pause(PRESS_MS);
        context.input.send_key(&key, Mods::NONE, false)
    };
    peek(context, hold, caps_lock_first)?;
    context.input.caps_lock(true)?;
    pause(PRESS_MS);
    context.input.key(&key, Mods::NONE)?;
    pause(PRESS_MS);
    context.input.caps_lock(false)?;
    wait_frontmost(context, "stacked-b")?;
    Ok(timed(elapsed))
}

/// Hold the shortcut down with `hold`, check stacked-a goes back and stays there while held,
/// let go with `let_go`, and check the stack is back in its order, stacked-a in front and
/// focused. Returns how long stacked-a took to go back.
fn peek(
    context: &mut Context,
    hold: impl Fn(&mut Context) -> Result<(), String>,
    let_go: impl Fn(&mut Context) -> Result<(), String>,
) -> Result<Outcome, String> {
    let windows = stack_windows(context)?;
    let before = stack_order(&windows)?;
    let seen = context.logged(LogEvent::PutBack).len();
    context.begin();
    let held = hold(context);
    let went_back = held.and_then(|()| wait_behind(context, &windows));
    pause(HOLD_MS);
    let still = went_back.and_then(|elapsed| is_behind(&windows).map(|()| elapsed));
    let_go(context)?;
    let elapsed = still?;
    wait_order(&windows, &before)?;
    wait_focused(context, "stacked-a")?;
    wait_logged_put_back(context, seen)?;
    Ok(timed(elapsed))
}

/// Wait until the fixture's window `expected` has keyboard focus. Letting go of the shortcut
/// is input to the window focused while it was held, which Windows lets keep the foreground
/// unless Snappy may take it.
fn wait_focused(context: &mut Context, expected: &str) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let focused = context.fixture.focused()?;
        if focused.as_deref() == Some(expected) {
            return Ok(());
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!("{expected} never took focus; {focused:?} has it"));
        }
        pause(20);
    }
}

/// Wait until the debug log, past its first `seen` put-back records, has the window put back.
/// Snappy logs it after asking, so it can show up a moment later.
fn wait_logged_put_back(context: &Context, seen: usize) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let put_back = context.logged(LogEvent::PutBack).len() - seen;
        if put_back == 1 {
            return Ok(());
        }
        if put_back > 1 || started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!("Snappy logged {put_back} windows put back"));
        }
        pause(20);
    }
}

/// The stack's windows.
fn stack_windows(context: &mut Context) -> Result<Vec<StackWindow>, String> {
    let pid = context.fixture.pid();
    STACK
        .iter()
        .map(|&id| {
            Ok(StackWindow {
                id,
                pid,
                frame: context.fixture.frame(id)?,
            })
        })
        .collect()
}

/// Wait until stacked-a has gone back (`is_behind`). Returns the time it took.
fn wait_behind(context: &mut Context, windows: &[StackWindow]) -> Result<f64, String> {
    let started = Instant::now();
    loop {
        if is_behind(windows).is_ok() {
            return Ok(context.elapsed_ms());
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return is_behind(windows).map(|()| context.elapsed_ms());
        }
        pause(10);
    }
}

/// Whether stacked-a is behind stacked-b, the window it covered that shows where it was, as a
/// press puts it. On macOS stacked-c, hidden behind stacked-b there, stays behind it.
fn is_behind(windows: &[StackWindow]) -> Result<(), String> {
    let listed = stack_order(windows)?;
    let place = |id: &str| listed.iter().position(|listed| *listed == id);
    match (place("stacked-b"), place("stacked-a")) {
        (Some(front), Some(sent)) if front < sent => Ok(()),
        _ => Err(format!("stacked-a isn't behind stacked-b: {listed:?}")),
    }
}

/// Holding the shortcut on the floating window shows the three zones' windows while it's
/// held, and letting go puts every window back in the order it was.
pub(crate) fn put_back_floating(context: &mut Context) -> Result<Outcome, String> {
    on_the_third_workspace(context, |context| {
        let mut other_app = context.fixture.another()?;
        let outcome = hold_over_three_zones(context, &mut other_app);
        drop(other_app);
        outcome
    })
}

fn hold_over_three_zones(context: &mut Context, other_app: &mut Fixture) -> Result<Outcome, String> {
    let windows = open_floating_over_three_zones(context, other_app)?;
    let before = stack_order(&windows)?;
    let combo = send_to_back_combo(context)?;
    context.begin();
    context.input.hold_chord(&combo)?;
    let held = wait_order(&windows, &[BOTTOM_MIDDLE, BOTTOM_LEFT, TOP_LEFT, FLOATING]);
    let elapsed = context.elapsed_ms();
    pause(HOLD_MS);
    context.input.let_go_of_chord(&combo)?;
    held?;
    wait_order(&windows, &before)?;
    wait_focused(context, FLOATING)?;
    Ok(timed(elapsed))
}
