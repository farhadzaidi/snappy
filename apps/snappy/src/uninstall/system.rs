//! Each system's part of `snappy --uninstall`: asking the running daemon to quit, and what
//! its adapters added outside Snappy's folders.

use std::time::Duration;

use snappy_engine::Paths;
use snappy_platform::uninstall::{QuitRequest, Removal};

/// How long the running daemon gets to quit, as the installers allow.
const QUIT_WAIT: Duration = Duration::from_secs(30);

#[cfg(target_os = "linux")]
pub fn request_quit(paths: &Paths) -> QuitRequest {
    snappy_linux::request_quit(&snappy_linux::lock_path(&paths.state_dir), QUIT_WAIT)
}

/// Whichever desktop Snappy ran on, as it may have run on more than one. KDE's keyboard
/// settings stop naming Snappy's XKB option before the option's files go.
#[cfg(target_os = "linux")]
pub fn remove_additions() -> Vec<Removal> {
    vec![
        snappy_linux::uninstall::remove_login_item(),
        snappy_gnome::uninstall::remove_extension(),
        snappy_kwin::uninstall::remove_keyboard_settings(),
        snappy_kwin::uninstall::remove_shortcuts(),
        snappy_linux::uninstall::remove_keyboard_option(),
    ]
}

#[cfg(target_os = "macos")]
pub fn request_quit(paths: &Paths) -> QuitRequest {
    snappy_macos::request_quit(&paths.state_dir, QUIT_WAIT)
}

#[cfg(target_os = "macos")]
pub fn remove_additions() -> Vec<Removal> {
    vec![snappy_macos::uninstall::remove_login_item()]
}

#[cfg(windows)]
pub fn request_quit(_paths: &Paths) -> QuitRequest {
    snappy_windows::request_quit(QUIT_WAIT)
}

#[cfg(windows)]
pub fn remove_additions() -> Vec<Removal> {
    vec![snappy_windows::uninstall::remove_login_item()]
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn request_quit(_paths: &Paths) -> QuitRequest {
    QuitRequest::NotRunning
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn remove_additions() -> Vec<Removal> {
    Vec::new()
}
