//! The app: a Tauri shell around the web UI. It relays daemon messages to the UI as
//! events and the UI's commands back to the daemon, and quits when its window closes.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daemon;
mod settings_file;
mod visibility;
mod window;

use std::path::PathBuf;

use snappy_ipc::Command;
use tauri::{AppHandle, State};

use crate::daemon::{Inbox, Outbox};
use crate::settings_file::SettingsFileError;
use crate::visibility::Visibility;

/// The web UI is listening; deliver the messages that arrived before it was.
#[tauri::command]
fn ui_ready(app: AppHandle, inbox: State<'_, Inbox>) {
    inbox.open(&app);
}

/// The UI has drawn: show the window.
#[tauri::command]
fn show_window(app: AppHandle, visibility: State<'_, Visibility>) {
    visibility.drawn(&app);
}

/// The frame of the display the window is on, or null when it can't be read.
#[tauri::command]
fn window_display_frame(app: AppHandle) -> Option<window::Frame> {
    window::current_display_frame(&app)
}

#[tauri::command]
fn send_command(id: u64, command: Command, outbox: State<'_, Outbox>) {
    outbox.send_command(id, command);
}

/// The text of the file the user chose to import. Async, so the file is read off the main thread.
#[tauri::command]
async fn read_settings_file(path: PathBuf) -> Result<String, SettingsFileError> {
    settings_file::read(&path)
}

/// Write exported settings to the file the user chose. Async, so it's written off the main thread.
#[tauri::command]
async fn write_settings_file(path: PathBuf, contents: String) -> Result<(), SettingsFileError> {
    settings_file::write(&path, &contents)
}

fn main() {
    let (outbox, link) = daemon::open();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Inbox::default())
        .manage(Visibility::default())
        .manage(outbox.clone());
    // Only macOS needs a menu: there it is the system menu bar, and its Edit items give text
    // fields the standard shortcuts. Elsewhere it would sit inside the window, and the web
    // view handles those shortcuts itself.
    #[cfg(target_os = "macos")]
    let builder = builder.menu(window::menu);
    let built = builder
        .invoke_handler(tauri::generate_handler![
            ui_ready,
            show_window,
            send_command,
            window_display_frame,
            read_settings_file,
            write_settings_file
        ])
        .setup(move |app| {
            if let Some(link) = link {
                link.start(app.handle());
            }
            Visibility::start_deadline(app.handle());
            Ok(())
        })
        .build(tauri::generate_context!());

    let app = match built {
        Ok(app) => app,
        Err(error) => {
            eprintln!("snappy-app: {error}");
            std::process::exit(1);
        }
    };
    #[cfg(target_os = "macos")]
    let app = without_dock_icon(app);
    app.run(|_, _| {});
}

/// An accessory app, with no Dock icon or Command-Tab entry. Set before the event loop starts,
/// so it applies at launch: changed any later, macOS hands activation back to the app that had
/// it, and the window stays in front without taking clicks until the user switches away and back.
#[cfg(target_os = "macos")]
fn without_dock_icon(mut app: tauri::App) -> tauri::App {
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    app
}

#[cfg(test)]
mod tests {
    /// Tauri takes the app's version from Cargo; the UI's package says the same.
    #[test]
    fn the_ui_package_carries_snappys_version() {
        let package = include_str!("../../ui/package.json");
        assert!(package.contains(&format!("\"version\": \"{}\"", env!("CARGO_PKG_VERSION"))));
    }

    /// The UI listens on the same event and invokes these commands by the same names.
    #[test]
    fn the_ui_names_the_shells_event_and_commands_alike() {
        let bridge = include_str!("../../ui/src/bridge/tauriBridge.ts");
        let commands = [
            "ui_ready",
            "show_window",
            "send_command",
            "window_display_frame",
            "read_settings_file",
            "write_settings_file",
        ];
        for name in std::iter::once(crate::daemon::MESSAGE_EVENT).chain(commands) {
            assert!(bridge.contains(&format!("\"{name}\"")), "{name}");
        }
    }

    /// The UI shows the same messages when a file fails without one.
    #[test]
    fn the_ui_words_file_failures_as_the_shell_does() {
        use crate::settings_file::SettingsFileError;
        let bridge = include_str!("../../ui/src/bridge/bridge.ts");
        for error in [SettingsFileError::Unreadable, SettingsFileError::Unwritable] {
            assert!(bridge.contains(&format!("\"{error}\"")), "{error}");
        }
    }
}
