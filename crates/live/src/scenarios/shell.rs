//! What Snappy shows of its own: the zone overlay and the menu bar item.

#[cfg(not(target_os = "macos"))]
use std::time::Instant;

use snappy_core::geometry::Point;
use snappy_core::keys::Mods;

use super::Outcome;
#[cfg(not(target_os = "macos"))]
use super::support::SETTLE_TIMEOUT;
use super::support::{Context, daemon_windows_above, floating};
use crate::input::pause;
use crate::screen::{FLOATING_LAYER, NORMAL_LAYER, on_screen_windows, screenshot};

/// The process owning the frontmost normal window.
fn frontmost_normal_window_pid() -> Result<Option<i32>, String> {
    Ok(on_screen_windows()?
        .into_iter()
        .find(|window| window.layer == NORMAL_LAYER)
        .map(|window| window.pid))
}

/// Also checks the hovered zone's caption: none for the setting's own action, and the
/// opposite action named while the swap key is held, saved as `overlay_caption.png`.
pub(super) fn overlay_floats(context: &mut Context) -> Result<Outcome, String> {
    context.open("overlay", floating(&context.display, 260.0))?;
    let frame = context.fixture.frame("overlay")?;
    let front_before = frontmost_normal_window_pid()?;
    let shift = context.drag_mods();
    let reversing = context.inverting_mods()?;
    let swap_key: Mods = reversing
        .iter()
        .filter(|modifier| !shift.contains(*modifier))
        .collect();
    let reversed = if context.settings.snapping.when_taken.swaps(true) {
        "Swap"
    } else {
        "Stack"
    };
    let hovered = context.zone(0)?.center();
    let seen = context.captions().len();
    let grip = Context::grip(frame);
    let input = &mut *context.input;
    input.modifiers(shift, true)?;
    input.pointer_down(grip, shift)?;
    pause(30);
    for step in 1..=6 {
        input.pointer_drag(
            Point::new(grip.x - 20.0 * f64::from(step), grip.y + 20.0 * f64::from(step)),
            shift,
        )?;
        pause(16);
    }
    input.pointer_drag(hovered, shift)?;
    pause(200);
    let shown = daemon_windows_above(context, FLOATING_LAYER)?;
    let front_during = frontmost_normal_window_pid()?;
    let plain_captions = context.captions().split_off(seen);

    let input = &mut *context.input;
    input.modifiers(swap_key, true)?;
    input.pointer_drag(Point::new(hovered.x + 10.0, hovered.y + 10.0), reversing)?;
    let captioned = context.wait_caption(seen, reversed);
    screenshot(&context.out.join("overlay_caption.png"));

    // Letting go of the modifiers first cancels the snap, so the drop leaves the window be.
    let input = &mut *context.input;
    input.modifiers(reversing, false)?;
    pause(50);
    input.pointer_up(grip, Mods::NONE)?;
    pause(300);
    let after = daemon_windows_above(context, FLOATING_LAYER)?;

    let covers_display = shown
        .iter()
        .any(|overlay| overlay.contains_rect(context.display.work_area));
    if !covers_display {
        return Err(format!(
            "no overlay covered the display during the drag: {shown:?}"
        ));
    }
    if front_during != front_before {
        return Err("the overlay changed which window is in front".to_owned());
    }
    if !plain_captions.is_empty() {
        return Err(format!(
            "the setting's own action showed a caption: {plain_captions:?}"
        ));
    }
    captioned?;
    if after
        .iter()
        .any(|overlay| overlay.contains_rect(context.display.work_area))
    {
        return Err("the overlay stayed on screen after the drop".to_owned());
    }
    Ok(Outcome::default())
}

/// Windows shows the item as a notification-area icon; the shell reports where it is.
#[cfg(windows)]
pub(super) fn menu_bar_item(context: &mut Context) -> Result<Outcome, String> {
    let pid = context.daemon.pid().ok_or("the daemon isn't running")?;
    let pid = u32::try_from(pid).map_err(|error| error.to_string())?;
    let started = Instant::now();
    while !snappy_windows::reference::has_tray_icon(pid) {
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!("process {pid} has no notification-area icon"));
        }
        pause(100);
    }
    Ok(Outcome::default())
}

#[cfg(target_os = "macos")]
pub(super) fn menu_bar_item(context: &mut Context) -> Result<Outcome, String> {
    let items = daemon_windows_above(context, crate::screen::STATUS_BAR_LAYER)?;
    let menu_bar_bottom = context.display.work_area.y;
    if items
        .iter()
        .any(|item| item.max_y() <= menu_bar_bottom + 1.0 && item.width > 0.0)
    {
        Ok(Outcome::default())
    } else {
        Err(format!("no status item in the menu bar: {items:?}"))
    }
}

/// X11's status item is a StatusNotifierItem on the session bus, not a window. It is
/// drawn by the desktop's tray host, so this checks that the daemon registered it. On GNOME
/// on Wayland, Snappy's extension puts the item in the top bar.
#[cfg(target_os = "linux")]
pub(super) fn menu_bar_item(context: &mut Context) -> Result<Outcome, String> {
    if crate::gnome::is_session() {
        let started = Instant::now();
        while !crate::gnome::indicator_shown()? {
            if started.elapsed() > SETTLE_TIMEOUT {
                return Err("Snappy's item isn't in the top bar".to_owned());
            }
            pause(100);
        }
        return Ok(Outcome::default());
    }
    let pid = context.daemon.pid().ok_or("the daemon isn't running")?;
    let started = Instant::now();
    loop {
        let names = crate::screen::session_bus_names()?;
        let prefix = format!("org.kde.StatusNotifierItem-{pid}-");
        if let Some(name) = names.iter().find(|name| name.starts_with(&prefix)) {
            return Ok(Outcome {
                elapsed_ms: None,
                note: Some(format!("registered as {name}; no tray host runs here to draw it")),
                ..Outcome::default()
            });
        }
        if started.elapsed() > SETTLE_TIMEOUT {
            return Err(format!(
                "no StatusNotifierItem of process {pid} on the session bus"
            ));
        }
        pause(100);
    }
}
