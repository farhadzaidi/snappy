//! The settings window: its Edit menu on macOS, display, and focus.

#[cfg(target_os = "macos")]
use tauri::Wry;
#[cfg(target_os = "macos")]
use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager};

pub const MAIN_WINDOW: &str = "main";

/// The menu the web view's text fields need for the standard editing shortcuts on macOS.
#[cfg(target_os = "macos")]
pub fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )?;
    // macOS turns the first submenu into the application menu.
    let application = Submenu::with_items(app, "Snappy", true, &[&PredefinedMenuItem::quit(app, None)?])?;
    Menu::with_items(app, &[&application, &edit])
}

/// A display frame in the units and shape `AppState.displays` uses.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Frame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The frame of the display the window is on, so the display chooser can start there.
pub fn current_display_frame(app: &AppHandle) -> Option<Frame> {
    let window = app.get_webview_window(MAIN_WINDOW)?;
    let monitor = window.current_monitor().ok().flatten()?;
    // Tauri measures monitors in physical pixels. The daemon's frames are logical on macOS
    // (points) and on Wayland, as GNOME Shell and KWin report them, and physical on Windows
    // and X11. On Linux Tauri scales GDK's logical geometry up by this same factor, so
    // dividing by it gives the logical frame back exactly.
    let pixels_per_unit = if daemon_frames_are_logical() {
        monitor.scale_factor()
    } else {
        1.0
    };
    let (position, size) = (monitor.position(), monitor.size());
    Some(Frame {
        x: f64::from(position.x) / pixels_per_unit,
        y: f64::from(position.y) / pixels_per_unit,
        width: f64::from(size.width) / pixels_per_unit,
        height: f64::from(size.height) / pixels_per_unit,
    })
}

#[cfg(target_os = "macos")]
fn daemon_frames_are_logical() -> bool {
    true
}

#[cfg(target_os = "linux")]
fn daemon_frames_are_logical() -> bool {
    let variable = |name: &str| std::env::var(name).ok();
    is_wayland(
        variable("XDG_SESSION_TYPE").as_deref(),
        variable("WAYLAND_DISPLAY").as_deref(),
    )
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn daemon_frames_are_logical() -> bool {
    false
}

/// A Wayland session, by the rule the daemon's Linux adapters use
/// (`snappy_linux::session::is_wayland`): `XDG_SESSION_TYPE` says so, or `WAYLAND_DISPLAY`
/// is set and the session doesn't declare itself X11.
#[cfg(any(target_os = "linux", test))]
fn is_wayland(session_type: Option<&str>, wayland_display: Option<&str>) -> bool {
    match session_type.map(str::to_ascii_lowercase).as_deref() {
        Some("wayland") => true,
        Some("x11") => false,
        _ => wayland_display.is_some_and(|display| !display.is_empty()),
    }
}

pub fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW)
        && let Err(error) = window.set_focus()
    {
        eprintln!("snappy-settings: couldn't focus the window: {error}");
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    fn window_config() -> Value {
        let config: Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        config["app"]["windows"][0].clone()
    }

    #[test]
    fn the_window_has_its_size_minimum_and_dark_transparent_title_bar() {
        let window = window_config();
        assert_eq!(window["label"], super::MAIN_WINDOW);
        assert_eq!(
            window["title"], "Snappy",
            "the window is Snappy's, not a separate settings app"
        );
        assert_eq!(
            (window["width"].as_f64(), window["height"].as_f64()),
            (Some(1080.0), Some(760.0))
        );
        assert_eq!(
            (window["minWidth"].as_f64(), window["minHeight"].as_f64()),
            (Some(940.0), Some(680.0))
        );
        assert_eq!(window["theme"], "Dark");
        assert_eq!(window["hiddenTitle"], true);
        assert_eq!(window["titleBarStyle"], "Transparent");
        assert_eq!(window["backgroundColor"], "#0E1115");
    }

    #[test]
    fn a_wayland_session_is_told_by_its_type_or_else_by_its_display() {
        assert!(super::is_wayland(Some("wayland"), None));
        assert!(!super::is_wayland(Some("x11"), Some("wayland-0")));
        assert!(super::is_wayland(Some("tty"), Some("wayland-0")));
        assert!(!super::is_wayland(None, Some("")));
        assert!(!super::is_wayland(None, None));
    }
}
