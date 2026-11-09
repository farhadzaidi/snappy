//! The Snappy daemon: menu bar presence, window management, and the settings app's host.
// No console window on Windows.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod paths;
mod runtime;
mod settings_app;

#[cfg(target_os = "macos")]
use std::fs::{File, TryLockError};
use std::process::ExitCode;

fn main() -> ExitCode {
    let background = std::env::args().any(|arg| arg == "--background");
    let Some(paths) = paths::snappy_paths() else {
        eprintln!("snappy: no home folder");
        return ExitCode::FAILURE;
    };
    #[cfg(target_os = "macos")]
    let Some(_lock) = single_instance_lock(&paths.state_dir) else {
        // Another daemon runs; on macOS, opening the app bundle reaches it as a reopen event.
        return ExitCode::SUCCESS;
    };
    match platform::run(paths, !background) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("snappy: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Hold an exclusive lock for the daemon's lifetime; `None` when another daemon has it.
#[cfg(target_os = "macos")]
fn single_instance_lock(state_dir: &std::path::Path) -> Option<File> {
    std::fs::create_dir_all(state_dir).ok()?;
    let file = File::create(state_dir.join("daemon.lock")).ok()?;
    match file.try_lock() {
        Ok(()) => Some(file),
        Err(TryLockError::WouldBlock) => None,
        Err(TryLockError::Error(_)) => Some(file),
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use snappy_engine::Paths;

    use crate::runtime::{forward_events, run_engine};

    /// AppKit owns the main thread; the engine runs on its own.
    pub fn run(paths: Paths, open_settings: bool) -> std::io::Result<()> {
        let (events, platform_events) = std::sync::mpsc::channel();
        let app = snappy_macos::App::new(events);
        let platform = app.platform();

        let (inputs, incoming) = tokio::sync::mpsc::unbounded_channel();
        forward_events(platform_events, inputs.clone());
        let engine = std::thread::Builder::new()
            .name("snappy-engine".into())
            .spawn(move || {
                let result = run_engine(platform, paths, inputs, incoming, open_settings);
                snappy_macos::request_termination();
                result
            })?;

        app.run();
        engine
            .join()
            .map_err(|_| std::io::Error::other("engine thread panicked"))?
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::sync::mpsc::{Receiver, Sender};

    use snappy_engine::Paths;
    use snappy_linux::Claim;
    use snappy_linux::session::Session;
    use snappy_platform::{Platform, PlatformEvent};

    use crate::runtime::{forward_events, run_engine};

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
                let app = snappy_gnome::App::new(events.clone()).map_err(std::io::Error::other)?;
                let (platform, stopper) = (app.platform(), app.stopper());
                drive(
                    paths,
                    open_settings,
                    events,
                    platform_events,
                    platform,
                    move || stopper.stop(),
                    || app.run().map_err(std::io::Error::other),
                )
            }
            Session::KdeWayland => {
                let app = snappy_kwin::App::new(events.clone()).map_err(std::io::Error::other)?;
                let (platform, stopper) = (app.platform(), app.stopper());
                drive(
                    paths,
                    open_settings,
                    events,
                    platform_events,
                    platform,
                    move || stopper.stop(),
                    || app.run().map_err(std::io::Error::other),
                )
            }
            Session::X11 | Session::OtherWayland => {
                let app = snappy_x11::App::new(events.clone()).map_err(std::io::Error::other)?;
                let (platform, stopper) = (app.platform(), app.stopper());
                drive(
                    paths,
                    open_settings,
                    events,
                    platform_events,
                    platform,
                    move || stopper.stop(),
                    || app.run().map_err(std::io::Error::other),
                )
            }
        }
    }

    /// Runs the engine on its own thread and the adapter's loop on this one. When the
    /// adapter's connection fails, the engine is asked to quit too, so parked windows come
    /// back before the daemon exits.
    fn drive<P: Platform + Send + 'static>(
        paths: Paths,
        open_settings: bool,
        events: Sender<PlatformEvent>,
        platform_events: Receiver<PlatformEvent>,
        platform: P,
        stop_adapter: impl FnOnce() + Send + 'static,
        run_adapter: impl FnOnce() -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let (inputs, incoming) = tokio::sync::mpsc::unbounded_channel();
        forward_events(platform_events, inputs.clone());
        let engine = std::thread::Builder::new()
            .name("snappy-engine".into())
            .spawn(move || {
                let result = run_engine(platform, paths, inputs, incoming, open_settings);
                stop_adapter();
                result
            })?;

        let ran = run_adapter();
        if ran.is_err() {
            let _ = events.send(PlatformEvent::QuitRequested);
        }
        let stopped = engine
            .join()
            .map_err(|_| std::io::Error::other("engine thread panicked"))?;
        ran.and(stopped)
    }
}

#[cfg(windows)]
mod platform {
    use snappy_engine::Paths;
    use snappy_windows::Claim;

    use crate::runtime::{forward_events, run_engine};

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

        let (inputs, incoming) = tokio::sync::mpsc::unbounded_channel();
        forward_events(platform_events, inputs.clone());
        let engine = std::thread::Builder::new()
            .name("snappy-engine".into())
            .spawn(move || {
                let result = run_engine(platform, paths, inputs, incoming, open_settings);
                stopper.stop();
                result
            })?;

        app.run();
        engine
            .join()
            .map_err(|_| std::io::Error::other("engine thread panicked"))?
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod platform {
    use snappy_engine::Paths;

    pub fn run(_paths: Paths, _open_settings: bool) -> std::io::Result<()> {
        Err(std::io::Error::other("this platform's adapter isn't built yet"))
    }
}
