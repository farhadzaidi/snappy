//! What the send-to-back scenarios share: the stack of fixture windows and the floating
//! window over three zones they open, reading their order in the window server's list, and
//! the third workspace they run on.

use std::time::Instant;

use snappy_core::geometry::Rect;
use snappy_core::keys::Combo;
use snappy_core::settings::NavigationShortcut;
use snappy_platform::Display;

use crate::fixture::Fixture;
use crate::input::pause;
use crate::scenarios::Outcome;
use crate::scenarios::support::{Context, SETTLE_TIMEOUT, near};
use crate::screen::{NORMAL_LAYER, on_screen_windows};

/// A window a scenario follows in the window server's list, by its process and frame.
#[derive(Debug, Clone, Copy)]
pub(super) struct StackWindow {
    pub(super) id: &'static str,
    pub(super) pid: u32,
    pub(super) frame: Rect,
}

/// The stack's windows, front to back once they're open.
pub(super) const STACK: [&str; 3] = ["stacked-a", "stacked-b", "stacked-c"];
/// How far each window of the stack sits from the one in front, so their frames differ.
pub(super) const STACK_STEP: f64 = 40.0;

/// `windows` in the order the window server shows them, front to back.
pub(super) fn stack_order(windows: &[StackWindow]) -> Result<Vec<&'static str>, String> {
    Ok(on_screen_windows()?
        .into_iter()
        .filter(|window| window.layer == NORMAL_LAYER)
        .filter_map(|window| {
            windows
                .iter()
                .find(|listed| {
                    i64::from(listed.pid) == i64::from(window.pid) && near(listed.frame, window.frame)
                })
                .map(|listed| listed.id)
        })
        .collect())
}

/// Switch to the third workspace, run `scenario` there, and come back.
pub(super) fn on_the_third_workspace(
    context: &mut Context,
    scenario: impl FnOnce(&mut Context) -> Result<Outcome, String>,
) -> Result<Outcome, String> {
    context.press(&context.switch_key(3)?)?;
    context.wait_active("workspace-3")?;
    let outcome = scenario(context);
    context.close_all();
    context.press(&context.switch_key(1)?)?;
    context.wait_active("workspace-1")?;
    outcome
}

/// Window `index` of the stack, front to back: the further back, the further up and left.
pub(super) fn stacked(display: &Display, index: usize) -> Rect {
    let area = display.work_area;
    let back = STACK_STEP * (STACK.len() - 1 - index) as f64;
    Rect::new(
        area.x + area.width * 0.2 + back,
        area.y + 100.0 + back,
        area.width * 0.4,
        area.height * 0.5,
    )
}

/// Open the stack back to front, so stacked-a ends up in front, focused. Returns each
/// window's frame, front to back.
pub(super) fn open_stack(context: &mut Context) -> Result<[Rect; 3], String> {
    for (index, id) in STACK.iter().enumerate().rev() {
        context.open(id, stacked(&context.display, index))?;
    }
    wait_frontmost(context, "stacked-a")?;
    let mut frames = [Rect::ZERO; 3];
    for (frame, id) in frames.iter_mut().zip(STACK) {
        *frame = context.fixture.frame(id)?;
    }
    Ok(frames)
}

pub(super) fn send_to_back_combo(context: &Context) -> Result<Combo, String> {
    Ok(context
        .navigation_key(NavigationShortcut::SendToBack)?
        .combo(context.settings.snappy_key))
}

/// The stack's window the window server shows in front of the others, if any.
fn frontmost(context: &mut Context) -> Result<Option<&'static str>, String> {
    let pid = i64::from(context.fixture.pid());
    let front = on_screen_windows()?
        .into_iter()
        .find(|window| i64::from(window.pid) == pid && window.layer == NORMAL_LAYER);
    let Some(front) = front else {
        return Ok(None);
    };
    for id in STACK {
        if near(context.fixture.frame(id)?, front.frame) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub(super) fn wait_frontmost(context: &mut Context, expected: &str) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let front = frontmost(context)?;
        if front == Some(expected) {
            return Ok(());
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!("{expected} never came to the front; {front:?} is"));
        }
        pause(20);
    }
}

/// Wait until the window list shows `expected`, front to back.
pub(super) fn wait_order(windows: &[StackWindow], expected: &[&'static str]) -> Result<(), String> {
    let started = Instant::now();
    loop {
        let listed = stack_order(windows)?;
        if listed == expected {
            return Ok(());
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!(
                "the stack never went back to {expected:?}; it's {listed:?}"
            ));
        }
        pause(10);
    }
}

/// A Terminal-like app's windows in three zones of their own, top left, bottom left, and
/// bottom middle, with a browser-like second app's window in the bottom left, and the first
/// app's window floating over all three, in front.
pub(super) const FLOATING: &str = "floating";
pub(super) const TOP_LEFT: &str = "zone-top-left";
pub(super) const BOTTOM_LEFT: &str = "zone-bottom-left";
pub(super) const BOTTOM_MIDDLE: &str = "zone-bottom-middle";
/// Space around each zone's window, wider than a title bar or border, so that only the
/// floating window overlaps them, as windows in zones with gaps do.
const ZONE_GAP: f64 = 40.0;

/// Open the floating window over three zones' windows, and return every window. The stack
/// opens front to back as floating, bottom middle, bottom left, top left.
pub(super) fn open_floating_over_three_zones(
    context: &mut Context,
    other_app: &mut Fixture,
) -> Result<Vec<StackWindow>, String> {
    let area = context.display.work_area;
    let (third, half) = (area.width / 3.0, area.height / 2.0);
    let zone = |column: f64, row: f64| {
        Rect::new(
            area.x + column * third + ZONE_GAP,
            area.y + row * half + ZONE_GAP,
            third - 2.0 * ZONE_GAP,
            half - 2.0 * ZONE_GAP,
        )
    };
    context.open(TOP_LEFT, zone(0.0, 0.0))?;
    other_app.open(BOTTOM_LEFT, zone(0.0, 1.0), None, None)?;
    other_app.focus(BOTTOM_LEFT)?;
    pause(300);
    context.open(BOTTOM_MIDDLE, zone(1.0, 1.0))?;
    let floating = Rect::new(area.x + third / 2.0, area.y + half / 2.0, third * 1.5, half);
    context.open(FLOATING, floating)?;
    let pid = context.fixture.pid();
    let mut windows = Vec::new();
    for id in [FLOATING, BOTTOM_MIDDLE, TOP_LEFT] {
        windows.push(StackWindow {
            id,
            pid,
            frame: context.fixture.frame(id)?,
        });
    }
    windows.push(StackWindow {
        id: BOTTOM_LEFT,
        pid: other_app.pid(),
        frame: other_app.frame(BOTTOM_LEFT)?,
    });
    wait_order(&windows, &[FLOATING, BOTTOM_MIDDLE, BOTTOM_LEFT, TOP_LEFT])?;
    Ok(windows)
}
