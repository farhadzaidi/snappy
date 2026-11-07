//! The overlay thread: draws and shows overlays on the event connection, and hands menu
//! updates to the tray, so the engine thread never waits for either.

pub mod glyphs;
pub mod render;
mod surface;

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};
use std::thread::JoinHandle;

use snappy_platform::{OverlayScene, PlatformEvent, TrayMenu};

use crate::shared::Shared;
use crate::tray::TrayIcon;
use surface::Overlays;

pub use surface::negotiate;

pub enum UiCommand {
    Prepare(String, OverlayScene),
    Show(String, OverlayScene),
    Hide,
    Retain(Vec<String>),
    Tray(TrayMenu),
    Stop,
}

/// Starts the overlay thread. The tray connects to the session bus from there, so a slow
/// bus never delays startup.
pub fn spawn(
    shared: Arc<Shared>,
    events: Sender<PlatformEvent>,
    commands: Receiver<UiCommand>,
) -> std::io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("snappy-overlay".into())
        .spawn(move || run(shared, events, commands))
}

fn run(shared: Arc<Shared>, events: Sender<PlatformEvent>, commands: Receiver<UiCommand>) {
    let mut overlays = Overlays::new(shared);
    let tray = TrayIcon::start(events);
    for command in commands {
        match command {
            UiCommand::Prepare(display, scene) => overlays.prepare(&display, &scene),
            UiCommand::Show(display, scene) => overlays.show(&display, &scene),
            UiCommand::Hide => overlays.hide(),
            UiCommand::Retain(displays) => overlays.retain(&displays),
            UiCommand::Tray(menu) => {
                if let Some(tray) = &tray {
                    tray.update(menu);
                }
            }
            UiCommand::Stop => break,
        }
    }
    overlays.close_all();
    if let Some(tray) = tray {
        tray.stop();
    }
}
