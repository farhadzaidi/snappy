//! Pressing the shortcut: each press sends the window in front behind the windows it
//! covers, and the stack settles in the order expected without the window sent back ever
//! coming forward on the way.

use std::time::Instant;

use snappy_core::geometry::{Point, Rect};
use snappy_core::keys::{Combo, Os};
use snappy_engine::LogEvent;

use super::support::{
    BOTTOM_LEFT, BOTTOM_MIDDLE, FLOATING, STACK, STACK_STEP, StackWindow, TOP_LEFT, on_the_third_workspace,
    open_floating_over_three_zones, open_stack, send_to_back_combo, stack_order, stacked, wait_frontmost,
};
use crate::fixture::Fixture;
use crate::input::pause;
use crate::scenarios::support::{Context, SETTLE_TIMEOUT, timed};
use crate::scenarios::{Outcome, PLATFORM};

/// Three windows stacked a little apart, front to back, stacked-a focused in front. Each press
/// sends the window in front behind the others, and the window that comes to the front takes
/// focus. Nothing moves. Where the window is lowered, three presses go round to where they
/// started. On macOS, which raises the windows that show over it instead, stacked-c, hidden
/// behind stacked-b where stacked-a was, stays behind stacked-a at the first press.
pub(crate) fn send_to_back_stack(context: &mut Context) -> Result<Outcome, String> {
    on_the_third_workspace(context, |context| {
        let frames = open_stack(context)?;
        let mut windows = Vec::new();
        for (id, frame) in STACK.into_iter().zip(frames) {
            windows.push(StackWindow {
                id,
                pid: context.fixture.pid(),
                frame,
            });
        }
        let combo = send_to_back_combo(context)?;
        let seen = context.logged(LogEvent::SentToBack).len();
        let orders = if PLATFORM == Os::MacOs {
            vec![
                vec!["stacked-b", "stacked-a", "stacked-c"],
                vec!["stacked-a", "stacked-c", "stacked-b"],
                vec!["stacked-c", "stacked-b", "stacked-a"],
            ]
        } else {
            rotations(&STACK)
        };
        context.begin();
        let between = press_through(context, &combo, &windows, &orders)?;
        let elapsed = context.elapsed_ms();
        let took_ms = logged_sends(context, seen, &[2, 2, 2])?;
        for (id, before) in STACK.iter().zip(frames) {
            context.stayed(id, before)?;
        }
        Ok(noted(elapsed, &between, &took_ms))
    })
}

/// A window of a second app, between stacked-a and stacked-b in the stack.
const OTHER_APP: &str = "other-app";
/// How long the stack is watched once it's in the order expected, for anything still
/// moving.
const SETTLED_MS: u64 = 300;

/// The stack with a window of a second app, another fixture process, between stacked-a and
/// stacked-b. Where the window is lowered, four presses go round. macOS can't lower another
/// app's window, so there the windows that show over the one sent back are raised over it,
/// which takes care to end up in order: the window focused is the other app's, then stacked-a's
/// app's, whose other windows are raised too, then one of several of its app's.
pub(crate) fn send_to_back_two_apps(context: &mut Context) -> Result<Outcome, String> {
    on_the_third_workspace(context, |context| {
        let mut other_app = context.fixture.another()?;
        let outcome = send_through_two_apps(context, &mut other_app);
        drop(other_app);
        outcome
    })
}

fn send_through_two_apps(context: &mut Context, other_app: &mut Fixture) -> Result<Outcome, String> {
    let display = context.display.clone();
    for index in [2, 1] {
        context.open(STACK[index], stacked(&display, index))?;
    }
    let between = stacked(&display, 1);
    let between = between.with_origin(Point::new(
        between.x + STACK_STEP / 2.0,
        between.y + STACK_STEP / 2.0,
    ));
    other_app.open(OTHER_APP, between, None, None)?;
    other_app.focus(OTHER_APP)?;
    pause(300);
    context.open(STACK[0], stacked(&display, 0))?;
    wait_frontmost(context, "stacked-a")?;

    let mut windows = Vec::new();
    for id in STACK {
        windows.push(StackWindow {
            id,
            pid: context.fixture.pid(),
            frame: context.fixture.frame(id)?,
        });
    }
    windows.push(StackWindow {
        id: OTHER_APP,
        pid: other_app.pid(),
        frame: other_app.frame(OTHER_APP)?,
    });
    let order = ["stacked-a", OTHER_APP, "stacked-b", "stacked-c"];
    let listed = stack_order(&windows)?;
    if listed != order {
        return Err(format!("the stack opened as {listed:?}"));
    }

    let combo = send_to_back_combo(context)?;
    let seen = context.logged(LogEvent::SentToBack).len();
    let orders = if PLATFORM == Os::MacOs {
        vec![
            vec![OTHER_APP, "stacked-a", "stacked-b", "stacked-c"],
            vec!["stacked-a", "stacked-b", OTHER_APP, "stacked-c"],
            vec!["stacked-b", OTHER_APP, "stacked-a", "stacked-c"],
            vec![OTHER_APP, "stacked-a", "stacked-c", "stacked-b"],
        ]
    } else {
        rotations(&order)
    };
    context.begin();
    let between = press_through(context, &combo, &windows, &orders)?;
    let elapsed = context.elapsed_ms();
    let took_ms = logged_sends(context, seen, &[3, 3, 3, 3])?;
    Ok(noted(elapsed, &between, &took_ms))
}

/// A window of a second app over both halves of the screen.
const WIDE: &str = "wide";

/// stacked-a and stacked-b stacked in the left half, front to back, stacked-c in the right
/// half, and a second app's window over both halves, behind them all. A press sends stacked-a
/// behind stacked-b and the wide window, and stacked-c, which it doesn't cover, stays in front
/// of the wide window. macOS raises stacked-c too, since the wide window's raise would put
/// that in front of it.
pub(crate) fn send_to_back_beside_wide(context: &mut Context) -> Result<Outcome, String> {
    on_the_third_workspace(context, |context| {
        let mut other_app = context.fixture.another()?;
        let outcome = send_beside_a_wide_window(context, &mut other_app);
        drop(other_app);
        outcome
    })
}

fn send_beside_a_wide_window(context: &mut Context, other_app: &mut Fixture) -> Result<Outcome, String> {
    let area = context.display.work_area;
    let half = |left: f64| {
        Rect::new(
            area.x + left + 40.0,
            area.y + 100.0,
            area.width * 0.5 - 80.0,
            area.height * 0.5,
        )
    };
    let wide = Rect::new(
        area.x + area.width * 0.15,
        area.y + 60.0,
        area.width * 0.7,
        area.height * 0.7,
    );
    other_app.open(WIDE, wide, None, None)?;
    other_app.focus(WIDE)?;
    pause(300);
    context.open("stacked-c", half(area.width * 0.5))?;
    let back = half(0.0);
    context.open(
        "stacked-b",
        back.with_origin(Point::new(back.x + STACK_STEP, back.y + STACK_STEP)),
    )?;
    context.open("stacked-a", back)?;
    wait_frontmost(context, "stacked-a")?;

    let mut windows = Vec::new();
    for id in STACK {
        windows.push(StackWindow {
            id,
            pid: context.fixture.pid(),
            frame: context.fixture.frame(id)?,
        });
    }
    windows.push(StackWindow {
        id: WIDE,
        pid: other_app.pid(),
        frame: other_app.frame(WIDE)?,
    });
    let listed = stack_order(&windows)?;
    if listed != ["stacked-a", "stacked-b", "stacked-c", WIDE] {
        return Err(format!("the stack opened as {listed:?}"));
    }

    let combo = send_to_back_combo(context)?;
    let seen = context.logged(LogEvent::SentToBack).len();
    context.begin();
    context.input.chord(&combo)?;
    let between = watch_send_to_back(
        &windows,
        "stacked-a",
        &["stacked-b", "stacked-c", WIDE, "stacked-a"],
    )?;
    let elapsed = context.elapsed_ms();
    let took_ms = logged_sends(context, seen, &[2])?;
    Ok(noted(elapsed, &[between], &took_ms))
}

/// `order`, the stack front to back, as each press leaves it where presses send the window in
/// front to the very back.
fn rotations(order: &[&'static str]) -> Vec<Vec<&'static str>> {
    (1..=order.len())
        .map(|turn| {
            let mut rotated = order.to_vec();
            rotated.rotate_left(turn);
            rotated
        })
        .collect()
}

/// Press once for each of `orders`, checking after each press that the stack settles as that
/// order, front to back, as `watch_send_to_back` says. Returns how many stacks showed
/// between the one before each press and the one after.
fn press_through(
    context: &mut Context,
    combo: &Combo,
    windows: &[StackWindow],
    orders: &[Vec<&'static str>],
) -> Result<Vec<usize>, String> {
    let mut between = Vec::new();
    let mut sent = stack_order(windows)?.first().copied().unwrap_or_default();
    for order in orders {
        context.input.chord(combo)?;
        between.push(watch_send_to_back(windows, sent, order)?);
        sent = order[0];
    }
    Ok(between)
}

/// Watch the window list until it shows `expected`, front to back, for `SETTLED_MS`. The
/// window `sent` back is never raised, so it never comes forward on the way. Returns how many
/// other stacks showed between the first look and the last.
fn watch_send_to_back(
    windows: &[StackWindow],
    sent: &str,
    expected: &[&'static str],
) -> Result<usize, String> {
    let started = Instant::now();
    let mut seen: Vec<Vec<&'static str>> = Vec::new();
    let mut settled_since: Option<Instant> = None;
    loop {
        let listed = stack_order(windows)?;
        let place = listed.iter().position(|id| *id == sent);
        let before = seen
            .last()
            .and_then(|last| last.iter().position(|id| *id == sent));
        if let (Some(place), Some(before)) = (place, before)
            && place < before
        {
            return Err(format!(
                "{sent} came forward on the way to the back: {seen:?}, then {listed:?}"
            ));
        }
        if seen.last() != Some(&listed) {
            seen.push(listed.clone());
        }
        if listed == expected {
            let since = *settled_since.get_or_insert_with(Instant::now);
            if since.elapsed().as_millis() >= u128::from(SETTLED_MS) {
                return Ok(seen.len().saturating_sub(2));
            }
        } else {
            settled_since = None;
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!(
                "the stack never settled as {expected:?}; it went {seen:?}"
            ));
        }
        pause(2);
    }
}

/// The debug log's sends to the back since the first `seen`, each with how many windows it
/// went behind, are `covered`. Returns how long each send took, in milliseconds.
fn logged_sends(context: &Context, seen: usize, covered: &[u64]) -> Result<Vec<u64>, String> {
    let records: Vec<serde_json::Value> = context
        .logged(LogEvent::SentToBack)
        .into_iter()
        .skip(seen)
        .collect();
    let logged: Vec<Option<u64>> = records.iter().map(|record| record["covered"].as_u64()).collect();
    let expected: Vec<Option<u64>> = covered.iter().copied().map(Some).collect();
    if logged != expected {
        return Err(format!(
            "Snappy logged the sends to the back as covering {logged:?}"
        ));
    }
    Ok(records
        .iter()
        .map(|record| record["tookMs"].as_f64().unwrap_or_default().round() as u64)
        .collect())
}

/// The outcome of presses that took `elapsed` milliseconds, noting for each press how many stacks
/// showed between the one before and the one after, and how long each send took.
fn noted(elapsed: f64, between: &[usize], took_ms: &[u64]) -> Outcome {
    Outcome {
        note: Some(format!(
            "stacks shown on the way, per press: {between:?}; milliseconds each took: {took_ms:?}"
        )),
        ..timed(elapsed)
    }
}

/// The window floating over three zones goes behind all three, and the bottom middle one,
/// the frontmost of them, takes focus, which brings it to the front. Pressed again, that one
/// goes behind the floating window, which stays behind the other two: focusing it would
/// raise it over them, so the
/// bottom left one, the front of those two, takes focus instead. No window ever ends in
/// front of one it was behind but the one sent back.
pub(crate) fn send_to_back_floating(context: &mut Context) -> Result<Outcome, String> {
    on_the_third_workspace(context, |context| {
        let mut other_app = context.fixture.another()?;
        let outcome = press_twice_over_three_zones(context, &mut other_app);
        drop(other_app);
        outcome
    })
}

fn press_twice_over_three_zones(context: &mut Context, other_app: &mut Fixture) -> Result<Outcome, String> {
    let windows = open_floating_over_three_zones(context, other_app)?;
    let combo = send_to_back_combo(context)?;
    let seen = context.logged(LogEvent::SentToBack).len();
    context.begin();
    let orders = [
        vec![BOTTOM_MIDDLE, BOTTOM_LEFT, TOP_LEFT, FLOATING],
        vec![BOTTOM_LEFT, TOP_LEFT, FLOATING, BOTTOM_MIDDLE],
    ];
    let between = press_through(context, &combo, &windows, &orders)?;
    let elapsed = context.elapsed_ms();
    let took_ms = logged_sends(context, seen, &[3, 1])?;
    Ok(noted(elapsed, &between, &took_ms))
}
