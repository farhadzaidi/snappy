//! The settings window: its Edit menu, display, and focus.

use tauri::menu::{Menu, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Wry};

pub const MAIN_WINDOW: &str = "main";

/// The menu the web view's text fields need for the standard editing shortcuts.
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
    let application = Submenu::with_items(
        app,
        "Snappy Settings",
        true,
        &[&PredefinedMenuItem::quit(app, None)?],
    )?;
    Menu::with_items(app, &[&application, &edit])
}

/// A display frame in the platform's window units, the shape `AppState.displays` uses.
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
    // macOS windows are measured in points; Windows and X11 use physical pixels.
    let pixels_per_unit = if cfg!(target_os = "macos") {
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

pub fn focus(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW)
        && let Err(error) = window.set_focus()
    {
        eprintln!("snappy-settings: could not focus the window: {error}");
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
}
