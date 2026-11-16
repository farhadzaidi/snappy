//! The settings app: a Tauri shell around the web UI. It relays daemon messages to the UI as
//! events and the UI's commands back to the daemon, and quits when its window closes.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod daemon;
mod window;

use snappy_ipc::Command;
use tauri::{AppHandle, State};

use crate::daemon::{Inbox, Outbox};

/// The web UI is listening; deliver the messages that arrived before it was.
#[tauri::command]
fn ui_ready(app: AppHandle, inbox: State<'_, Inbox>) {
    inbox.open(&app);
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

fn main() {
    let (outbox, link) = daemon::open();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Inbox::default())
        .manage(outbox.clone());
    // Only macOS needs a menu: there it is the system menu bar, and its Edit items give text
    // fields the standard shortcuts. Elsewhere it would sit inside the window, and the web
    // view handles those shortcuts itself.
    #[cfg(target_os = "macos")]
    let builder = builder.menu(window::menu);
    let result = builder
        .invoke_handler(tauri::generate_handler![
            ui_ready,
            send_command,
            window_display_frame
        ])
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            if let Some(link) = link {
                link.start(app.handle());
            }
            window::focus(app.handle());
            Ok(())
        })
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("snappy-settings: {error}");
        std::process::exit(1);
    }
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
        let commands = ["ui_ready", "send_command", "window_display_frame"];
        for name in std::iter::once(crate::daemon::MESSAGE_EVENT).chain(commands) {
            assert!(bridge.contains(&format!("\"{name}\"")), "{name}");
        }
    }
}
