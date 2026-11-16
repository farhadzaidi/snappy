//! Each desktop's own way to inject input, place and read windows, list what's on screen,
//! and take screenshots. The cross-platform front doors (`input::platform_input`,
//! `screen::on_screen_windows`, `screen::screenshot`, `display_mode`) pick the one in use;
//! on Linux, `wayland::Shell` picks GNOME or KWin, and X11 is the rest.

#[cfg(target_os = "linux")]
pub mod gnome;
#[cfg(target_os = "linux")]
pub mod kwin;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "linux")]
pub mod wayland;
#[cfg(windows)]
pub mod windows;
#[cfg(target_os = "linux")]
pub mod x11;
