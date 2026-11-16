//! The Snappy daemon: menu bar presence, window management, and the settings app's host.
// No console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod paths;
mod runtime;
mod settings_app;

use std::process::ExitCode;

use snappy_platform::launch;

/// How long `launch::STOP_FOR_RESTART` waits for the running daemon to exit.
#[cfg(windows)]
const RESTART_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

fn main() -> ExitCode {
    #[cfg(windows)]
    if std::env::args().any(|arg| arg == launch::STOP_FOR_RESTART) {
        return stop_for_restart();
    }
    let background = std::env::args().any(|arg| arg == launch::BACKGROUND);
    let Some(paths) = paths::snappy_paths() else {
        eprintln!("snappy: no home folder");
        return ExitCode::FAILURE;
    };
    match platform::run(paths, !background) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("snappy: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Stop the running daemon so a new one can take over with every window where it is, the
/// way `SIGUSR2` does elsewhere: Windows has no such signal.
#[cfg(windows)]
fn stop_for_restart() -> ExitCode {
    match snappy_windows::request_restart(RESTART_WAIT) {
        snappy_windows::RestartRequest::Stopped | snappy_windows::RestartRequest::NotRunning => {
            ExitCode::SUCCESS
        }
        snappy_windows::RestartRequest::Unsupported => {
            eprintln!("snappy: the running Snappy can't stop for a restart; end it with taskkill /F instead");
            ExitCode::FAILURE
        }
        snappy_windows::RestartRequest::StillRunning => {
            eprintln!("snappy: the running Snappy didn't stop");
            ExitCode::FAILURE
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use snappy_engine::Paths;
    use snappy_macos::Claim;

    use crate::runtime::{join_engine, spawn_engine};

    /// AppKit owns the main thread; the engine runs on its own. A second launch exits;
    /// opening the app bundle reaches the running daemon as a reopen event.
    pub fn run(paths: Paths, open_settings: bool) -> std::io::Result<()> {
        let Claim::Primary(_instance) = snappy_macos::claim(&paths.state_dir)? else {
            return Ok(());
        };
        let (events, platform_events) = std::sync::mpsc::channel();
        let app = snappy_macos::App::new(events).map_err(std::io::Error::other)?;
        let platform = app.platform();
        let engine = spawn_engine(
            platform,
            paths,
            platform_events,
            open_settings,
            snappy_macos::request_termination,
        )?;
        app.run();
        join_engine(engine)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::sync::mpsc::{Receiver, Sender};

    use snappy_engine::Paths;
    use snappy_linux::Claim;
    use snappy_linux::adapter::Adapter;
    use snappy_linux::session::Session;
    use snappy_platform::PlatformEvent;

    use crate::runtime::{join_engine, spawn_engine};

    /// GNOME on Wayland goes through GNOME Shell's extension, KDE Plasma on Wayland through
    /// KWin's script; every other session through the X server, which in other Wayland
    /// sessions reports that it can't control windows.
    /// A second launch signals the running daemon to open the settings window, then exits.
    pub fn run(paths: Paths, open_settings: bool) -> std::io::Result<()> {
        let Claim::Primary(mut instance) = snappy_linux::claim(&snappy_linux::lock_path(&paths.state_dir))?
        else {
            return Ok(());
        };
        let (events, platform_events) = std::sync::mpsc::channel();
        instance.publish(events.clone())?;
        match snappy_linux::session::current() {
            Session::GnomeWayland => {
                drive::<snappy_gnome::App>(paths, open_settings, events, platform_events)
            }
            Session::KdeWayland => drive::<snappy_kwin::App>(paths, open_settings, events, platform_events),
            Session::X11 | Session::OtherWayland => {
                drive::<snappy_x11::App>(paths, open_settings, events, platform_events)
            }
        }
    }

    /// Starts the adapter, runs the engine on its own thread and the adapter's loop on this
    /// one. When the adapter's connection fails, the engine is asked to quit too, so parked
    /// windows come back before the daemon exits.
    fn drive<A: Adapter>(
        paths: Paths,
        open_settings: bool,
        events: Sender<PlatformEvent>,
        platform_events: Receiver<PlatformEvent>,
    ) -> std::io::Result<()> {
        let app = A::start(events.clone()).map_err(std::io::Error::other)?;
        let (platform, stop_adapter) = (app.platform(), app.stop_handle());
        let engine = spawn_engine(platform, paths, platform_events, open_settings, stop_adapter)?;
        let ran = app.run().map_err(std::io::Error::other);
        if ran.is_err() {
            let _ = events.send(PlatformEvent::QuitRequested);
        }
        let stopped = join_engine(engine);
        ran.and(stopped)
    }
}

#[cfg(windows)]
mod platform {
    use snappy_engine::Paths;
    use snappy_windows::Claim;

    use crate::runtime::{join_engine, spawn_engine};

    /// The Win32 message loop runs on this thread and the engine on its own. A second
    /// launch signals the running daemon to open the settings window, then exits.
    pub fn run(paths: Paths, open_settings: bool) -> std::io::Result<()> {
        let Claim::Primary(mut instance) = snappy_windows::claim()? else {
            return Ok(());
        };
        let (events, platform_events) = std::sync::mpsc::channel();
        instance.publish(events.clone())?;
        let app = snappy_windows::App::new(events).map_err(std::io::Error::other)?;
        let platform = app.platform();
        let stopper = app.stopper();
        let engine = spawn_engine(platform, paths, platform_events, open_settings, move || {
            stopper.stop();
        })?;
        app.run();
        join_engine(engine)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    use snappy_engine::Paths;

    pub fn run(_paths: Paths, _open_settings: bool) -> std::io::Result<()> {
        Err(std::io::Error::other("this platform's adapter isn't built yet"))
    }
}
