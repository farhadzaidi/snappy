//! Uninstalling: `snappy --uninstall`, as the uninstall scripts run it, brings back the
//! windows Snappy hid, whether Snappy was running or ended without quitting, and deletes
//! Snappy's files. Each scenario puts the settings back and starts Snappy again after.
//!
//! On GNOME uninstalling also removes Snappy's extension, which GNOME loads again only at the
//! next login, so the GNOME suite runs these on their own, logging in again between them.

use super::Outcome;
use super::support::{Context, floating, timed};

pub(super) fn uninstalling_quits_and_returns_windows(context: &mut Context) -> Result<Outcome, String> {
    let before = park(context, "uninstalled")?;
    context.begin();
    let said = context.daemon.uninstall()?;
    let elapsed = context.wait_shown("uninstalled", before)?;
    deleted_its_files(context, &said)?;
    start_again(context)?;
    Ok(with_what_it_said(timed(elapsed), &said))
}

/// A crash leaves the window hidden; uninstalling starts Snappy once to bring it back.
pub(super) fn uninstalling_after_a_crash_returns_windows(context: &mut Context) -> Result<Outcome, String> {
    let before = park(context, "crashed")?;
    context.daemon.kill();
    context.begin();
    let said = context.daemon.uninstall()?;
    let elapsed = context.wait_shown("crashed", before)?;
    deleted_its_files(context, &said)?;
    start_again(context)?;
    Ok(with_what_it_said(timed(elapsed), &said))
}

/// Keeps what uninstalling said with the result, to show which steps it took.
fn with_what_it_said(outcome: Outcome, said: &str) -> Outcome {
    Outcome {
        note: Some(said.trim().replace('\n', " | ")),
        ..outcome
    }
}

/// Opens a window and sends it to workspace 2, returning where it was.
fn park(context: &mut Context, name: &str) -> Result<snappy_core::geometry::Rect, String> {
    context.open(name, floating(&context.display, 260.0))?;
    let before = context.fixture.frame(name)?;
    context.press(&context.send_key(2)?)?;
    context.wait_parked(name)?;
    Ok(before)
}

/// Snappy's files are gone, and on GNOME its extension.
fn deleted_its_files(context: &Context, said: &str) -> Result<(), String> {
    let paths = context.daemon.paths();
    for path in [paths.settings, paths.state_dir, paths.log_dir] {
        if path.exists() {
            return Err(format!(
                "{} is still there after uninstalling, which said:\n{said}",
                path.display()
            ));
        }
    }
    if extension_installed() {
        return Err(format!(
            "Snappy's GNOME extension is still installed after uninstalling, which said:\n{said}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn extension_installed() -> bool {
    crate::desktop::gnome::is_session() && crate::desktop::gnome::snappy_extension_installed()
}

#[cfg(not(target_os = "linux"))]
fn extension_installed() -> bool {
    false
}

/// Puts the run's settings back and starts Snappy for the scenarios that follow.
fn start_again(context: &Context) -> Result<(), String> {
    context.daemon.write_settings(context.settings)?;
    context.daemon.start()
}
